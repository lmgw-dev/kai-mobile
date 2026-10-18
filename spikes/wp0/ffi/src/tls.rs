//! Full TLS 1.3 handshakes with client authentication on the phone (design §4.4, WP0 (e)): a
//! loopback rustls server in this process, configured as lmgw's device listener is (TLS 1.3, its
//! leaf with the CA after it, client certificates verified against the CA, no resumption), and
//! the device's side configured by `lmgw_client::tls::client_config`, whose client certificate
//! signs through the Keystore callback. A test CA, the server's leaf and the device's
//! certificate for the Keystore key are made at runtime with rcgen. ring on both sides.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use lmgw_client::{pairing::Urls, requests::Credentials};
use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, PublicKeyData, date_time_ymd,
};
use rustls::{
    ClientConnection, RootCertStore, ServerConfig, ServerConnection, SignatureAlgorithm,
    SignatureScheme,
    crypto::CryptoProvider,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, SubjectPublicKeyInfoDer},
    server::{NoServerSessionStorage, WebPkiClientVerifier},
    sign::{CertifiedKey, Signer, SigningKey, SingleCertAndKey},
};

use crate::{
    Keystore, SpikeError, Timings,
    keystore::{on_worker, p256_point, us},
};

/// One handshake's times, in microseconds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HandshakeTiming {
    /// TCP connect to the loopback server.
    pub connect_us: f64,
    /// From the client's first flight to the handshake's end on the client, the signature included.
    pub handshake_us: f64,
    /// The Keystore signature inside it (the callback, timed around the call).
    pub sign_us: f64,
    /// One application-data byte each way after the handshake.
    pub round_trip_us: f64,
}

/// What [`tls_bench`] measured.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TlsReport {
    pub key_generate_us: f64,
    pub ca_us: f64,
    pub server_leaf_us: f64,
    pub device_cert_us: f64,
    /// Every handshake in order; the first is the process's first through this path.
    pub handshakes: Vec<HandshakeTiming>,
    pub handshake: Timings,
    pub handshake_warm: Timings,
    pub sign: Timings,
    pub protocol: String,
    pub cipher_suite: String,
    pub key_exchange: String,
    /// Whether the server received and verified the device's certificate every time.
    pub client_cert_verified: bool,
}

/// `handshakes` full handshakes (at least 2) with a fresh Keystore key under `alias`, which is
/// deleted afterwards.
#[uniffi::export]
pub fn tls_bench(
    keystore: Box<dyn Keystore>,
    alias: String,
    handshakes: u32,
) -> Result<TlsReport, SpikeError> {
    let keystore: Arc<dyn Keystore> = Arc::from(keystore);
    let handshakes = handshakes.max(2);
    let result = {
        let keystore = keystore.clone();
        let alias = alias.clone();
        on_worker(move || run(keystore, alias, handshakes))
    };
    keystore.delete(alias);
    result
}

fn ring() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn run(
    keystore: Arc<dyn Keystore>,
    alias: String,
    handshakes: u32,
) -> Result<TlsReport, SpikeError> {
    let t = Instant::now();
    let spki = keystore
        .generate(alias.clone())
        .map_err(|e| SpikeError::at("generate the device key", e))?;
    let key_generate = t.elapsed();
    let point = p256_point(&spki)
        .map_err(|e| SpikeError::at("the device key", e))?
        .to_vec();

    let t = Instant::now();
    let ca = ca().map_err(|e| SpikeError::at("the test CA", e))?;
    let ca_time = t.elapsed();
    let t = Instant::now();
    let leaf_key = KeyPair::generate().map_err(|e| SpikeError::at("the server key", e))?;
    let leaf = params(
        &["127.0.0.1", "localhost"],
        ExtendedKeyUsagePurpose::ServerAuth,
    )
    .and_then(|p| p.signed_by(&leaf_key, &*ca))
    .map_err(|e| SpikeError::at("the server certificate", e))?;
    let leaf_time = t.elapsed();
    let t = Instant::now();
    let device = params(&[], ExtendedKeyUsagePurpose::ClientAuth)
        .and_then(|p| p.signed_by(&ForeignP256(point), &*ca))
        .map_err(|e| SpikeError::at("the device certificate", e))?;
    let device_time = t.elapsed();

    let signs: Arc<Mutex<Vec<Duration>>> = Arc::default();
    let signing_key = Arc::new(KeystoreKey {
        keystore,
        alias,
        spki: spki.clone(),
        signs: signs.clone(),
    });
    let credentials = Credentials {
        device_id: 0,
        certificate: device.der().to_vec(),
        ca_certificate: ca.der().to_vec(),
        not_after_ms: 0,
        notice_after_ms: 0,
        urls: Urls::try_from(vec!["https://127.0.0.1:1".to_owned()])
            .map_err(|e| SpikeError::at("the URL list", e))?,
        confirm_code: String::new(),
    };
    let mut client = lmgw_client::tls::client_config(ring(), &credentials, signing_key)
        .map_err(|e| SpikeError::at("the client configuration", e))?;
    client.resumption = rustls::client::Resumption::disabled();
    let client = Arc::new(client);
    let server = Arc::new(
        server_config(&ca, leaf.der(), &leaf_key)
            .map_err(|e| SpikeError::at("the server configuration", e))?,
    );

    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| SpikeError::at("bind the server", e))?;
    let addr = listener
        .local_addr()
        .map_err(|e| SpikeError::at("the server's address", e))?;
    let serving = std::thread::Builder::new()
        .name("spike-tls-server".into())
        .spawn(move || serve(listener, server, handshakes))
        .map_err(|e| SpikeError::at("start the server", e))?;

    let name = ServerName::try_from("127.0.0.1").map_err(|e| SpikeError::at("server name", e))?;
    let mut timings = Vec::new();
    let mut facts = (String::new(), String::new(), String::new());
    for i in 0..handshakes {
        let step = |what: &str| format!("handshake {}: {what}", i + 1);
        let t0 = Instant::now();
        let mut tcp = TcpStream::connect(addr).map_err(|e| SpikeError::at(&step("connect"), e))?;
        let connect = t0.elapsed();
        tcp.set_nodelay(true).ok();
        let t1 = Instant::now();
        let mut conn = ClientConnection::new(client.clone(), name.clone())
            .map_err(|e| SpikeError::at(&step("client"), e))?;
        while conn.is_handshaking() {
            conn.complete_io(&mut tcp)
                .map_err(|e| SpikeError::at(&step("handshake"), e))?;
        }
        let handshake = t1.elapsed();
        let t2 = Instant::now();
        let mut stream = rustls::Stream::new(&mut conn, &mut tcp);
        stream
            .write_all(b"p")
            .and_then(|()| stream.flush())
            .map_err(|e| SpikeError::at(&step("write"), e))?;
        let mut byte = [0u8; 1];
        stream
            .read_exact(&mut byte)
            .map_err(|e| SpikeError::at(&step("read"), e))?;
        let round_trip = t2.elapsed();
        if i == 0 {
            facts = (
                conn.protocol_version()
                    .map_or("none".into(), |v| format!("{v:?}")),
                conn.negotiated_cipher_suite()
                    .map_or("none".into(), |s| format!("{:?}", s.suite())),
                conn.negotiated_key_exchange_group()
                    .map_or("none".into(), |g| format!("{:?}", g.name())),
            );
        }
        conn.send_close_notify();
        conn.complete_io(&mut tcp).ok();
        let sign = signs
            .lock()
            .map(|mut s| s.drain(..).sum::<Duration>())
            .unwrap_or_default();
        timings.push((connect, handshake, sign, round_trip));
    }
    let verified = serving
        .join()
        .map_err(|_| SpikeError::at("the server", "panicked"))?
        .map_err(|e| SpikeError::at("the server", e))?;

    let all: Vec<Duration> = timings.iter().map(|t| t.1).collect();
    let sign: Vec<Duration> = timings.iter().map(|t| t.2).collect();
    Ok(TlsReport {
        key_generate_us: us(key_generate),
        ca_us: us(ca_time),
        server_leaf_us: us(leaf_time),
        device_cert_us: us(device_time),
        handshakes: timings
            .iter()
            .map(|&(c, h, s, r)| HandshakeTiming {
                connect_us: us(c),
                handshake_us: us(h),
                sign_us: us(s),
                round_trip_us: us(r),
            })
            .collect(),
        handshake: Timings::of(&all),
        handshake_warm: Timings::of(&all[1..]),
        sign: Timings::of(&sign),
        protocol: facts.0,
        cipher_suite: facts.1,
        key_exchange: facts.2,
        client_cert_verified: verified == handshakes,
    })
}

/// Accepts `n` connections in turn; how many presented a client certificate the verifier took.
fn serve(listener: TcpListener, config: Arc<ServerConfig>, n: u32) -> Result<u32, String> {
    let mut verified = 0;
    for _ in 0..n {
        let (mut tcp, _) = listener.accept().map_err(|e| format!("accept: {e}"))?;
        tcp.set_nodelay(true).ok();
        let mut conn = ServerConnection::new(config.clone()).map_err(|e| e.to_string())?;
        while conn.is_handshaking() {
            conn.complete_io(&mut tcp)
                .map_err(|e| format!("handshake: {e}"))?;
        }
        if conn.peer_certificates().is_some_and(|c| !c.is_empty()) {
            verified += 1;
        }
        let mut stream = rustls::Stream::new(&mut conn, &mut tcp);
        let mut byte = [0u8; 1];
        stream
            .read_exact(&mut byte)
            .map_err(|e| format!("read: {e}"))?;
        stream
            .write_all(b"q")
            .and_then(|()| stream.flush())
            .map_err(|e| format!("write: {e}"))?;
        // Read until the client's close_notify (or its close).
        let mut rest = Vec::new();
        let _ = stream.read_to_end(&mut rest);
    }
    Ok(verified)
}

/// A test CA as lmgw makes its device CA (P-256, `pathlen 0`).
fn ca() -> Result<CertifiedIssuer<'static, KeyPair>, rcgen::Error> {
    let mut p = CertificateParams::new(Vec::<String>::new())?;
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "kai wp0 spike test CA");
    p.distinguished_name = dn;
    p.not_before = date_time_ymd(2020, 1, 1);
    p.not_after = date_time_ymd(2100, 1, 1);
    CertifiedIssuer::self_signed(p, KeyPair::generate()?)
}

fn params(names: &[&str], eku: ExtendedKeyUsagePurpose) -> Result<CertificateParams, rcgen::Error> {
    let mut p = CertificateParams::new(names.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;
    p.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    p.extended_key_usages = vec![eku];
    p.use_authority_key_identifier_extension = true;
    p.not_before = date_time_ymd(2020, 1, 1);
    p.not_after = date_time_ymd(2100, 1, 1);
    Ok(p)
}

fn server_config(
    ca: &CertifiedIssuer<'static, KeyPair>,
    leaf: &[u8],
    key: &KeyPair,
) -> Result<ServerConfig, rustls::Error> {
    let provider = ring();
    let mut roots = RootCertStore::empty();
    roots.add(ca.der().clone())?;
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .map_err(|e| rustls::Error::General(e.to_string()))?;
    let signing = provider
        .key_provider
        .load_private_key(PrivateKeyDer::Pkcs8(key.serialize_der().into()))?;
    let chain = vec![CertificateDer::from(leaf.to_vec()), ca.der().clone()];
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(verifier)
        .with_cert_resolver(Arc::new(SingleCertAndKey::from(CertifiedKey::new(
            chain, signing,
        ))));
    cfg.session_storage = Arc::new(NoServerSessionStorage {});
    cfg.send_tls13_tickets = 0;
    Ok(cfg)
}

/// A P-256 public key held elsewhere (the Keystore), for rcgen to certify.
struct ForeignP256(Vec<u8>);

impl PublicKeyData for ForeignP256 {
    fn der_bytes(&self) -> &[u8] {
        &self.0
    }

    fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
        &rcgen::PKCS_ECDSA_P256_SHA256
    }
}

/// rustls's signing key over the Keystore callback (design M7's `SigningKey`), timing each
/// signature.
struct KeystoreKey {
    keystore: Arc<dyn Keystore>,
    alias: String,
    spki: Vec<u8>,
    signs: Arc<Mutex<Vec<Duration>>>,
}

impl std::fmt::Debug for KeystoreKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeystoreKey")
            .field("alias", &self.alias)
            .finish_non_exhaustive()
    }
}

impl SigningKey for KeystoreKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| {
                Box::new(KeystoreSigner {
                    keystore: self.keystore.clone(),
                    alias: self.alias.clone(),
                    signs: self.signs.clone(),
                }) as Box<dyn Signer>
            })
    }

    fn public_key(&self) -> Option<SubjectPublicKeyInfoDer<'_>> {
        Some(SubjectPublicKeyInfoDer::from(self.spki.as_slice()))
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

struct KeystoreSigner {
    keystore: Arc<dyn Keystore>,
    alias: String,
    signs: Arc<Mutex<Vec<Duration>>>,
}

impl std::fmt::Debug for KeystoreSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeystoreSigner")
            .field("alias", &self.alias)
            .finish_non_exhaustive()
    }
}

impl Signer for KeystoreSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let t = Instant::now();
        let signature = self
            .keystore
            .sign(self.alias.clone(), message.to_vec())
            .map_err(|e| rustls::Error::General(format!("the Keystore's signature: {e}")))?;
        if let Ok(mut s) = self.signs.lock() {
            s.push(t.elapsed());
        }
        Ok(signature)
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ECDSA_NISTP256_SHA256
    }
}
