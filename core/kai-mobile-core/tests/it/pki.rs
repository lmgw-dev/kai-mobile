//! The tests' device link (as Kai desktop's `test_pki`): a device CA, server leaves and device
//! certificates made with rcgen as lmgw makes them (device link design §2: P-256, the CA with
//! `pathlen 0`, leaves for `serverAuth` naming their hosts, device certificates for `clientAuth`),
//! a loopback TLS listener as lmgw's device listener serves ([`Gateway`]: TLS 1.3, the client's
//! certificate checked against the CA, required or optional), and a [`Squatter`] that answers a
//! ClientHello with a plaintext fatal alert.

use std::{
    net::SocketAddr,
    sync::{Arc, LazyLock},
};

use kai_mobile_core::lmgw::pairing::Fingerprint;
use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, PublicKeyData, date_time_ymd,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

pub type Ca = CertifiedIssuer<'static, KeyPair>;

/// A device CA named as lmgw names its own; `cn` tells them apart.
pub fn ca(cn: &str) -> Ca {
    let mut p = CertificateParams::new(Vec::<String>::new()).unwrap();
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    dn.push(DnType::OrganizationName, "lmgw");
    p.distinguished_name = dn;
    p.not_before = date_time_ymd(2026, 1, 1);
    p.not_after = date_time_ymd(9999, 12, 31);
    CertifiedIssuer::self_signed(p, KeyPair::generate().unwrap()).unwrap()
}

/// The `fp` that pins `ca`.
pub fn fp(ca: &Ca) -> Fingerprint {
    Fingerprint::of_spki(&ca.key().subject_public_key_info())
}

fn leaf_params(
    names: &[&str],
    eku: ExtendedKeyUsagePurpose,
    years: (i32, i32),
) -> CertificateParams {
    let mut p =
        CertificateParams::new(names.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
    p.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    p.extended_key_usages = vec![eku];
    p.use_authority_key_identifier_extension = true;
    p.not_before = date_time_ymd(years.0, 1, 1);
    p.not_after = date_time_ymd(years.1, 1, 1);
    p
}

/// A device's public key as enrolment sends it (a DER SubjectPublicKeyInfo of a P-256 key), as
/// rcgen signs for it.
pub struct DevicePub(pub Vec<u8>);

impl PublicKeyData for DevicePub {
    fn der_bytes(&self) -> &[u8] {
        // The uncompressed point closes a canonical P-256 SubjectPublicKeyInfo.
        &self.0[self.0.len() - 65..]
    }
    fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
        &rcgen::PKCS_ECDSA_P256_SHA256
    }
}

/// A device certificate for the public key `spki`, valid for `years`, as lmgw issues it.
pub fn device_cert(ca: &Ca, spki: &[u8], years: (i32, i32)) -> Vec<u8> {
    leaf_params(&[], ExtendedKeyUsagePurpose::ClientAuth, years)
        .signed_by(&DevicePub(spki.to_vec()), ca)
        .unwrap()
        .der()
        .to_vec()
}

/// One gateway's PKI: its CA and a server leaf for `names`.
pub struct Pki {
    pub ca: Ca,
    leaf: (Vec<u8>, KeyPair),
}

impl Pki {
    pub fn new(cn: &str) -> Self {
        Self::naming(cn, &["127.0.0.1", "localhost"], (2020, 2100))
    }

    /// A gateway whose leaf names `names` and is valid for `years`.
    pub fn naming(cn: &str, names: &[&str], years: (i32, i32)) -> Self {
        let ca = ca(cn);
        let key = KeyPair::generate().unwrap();
        let der = leaf_params(names, ExtendedKeyUsagePurpose::ServerAuth, years)
            .signed_by(&key, &ca)
            .unwrap()
            .der()
            .to_vec();
        Self {
            ca,
            leaf: (der, key),
        }
    }

    pub fn server_config(&self, clients: Clients) -> rustls::ServerConfig {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap();
        let builder = match clients {
            Clients::None => builder.with_no_client_auth(),
            Clients::Required | Clients::Optional => {
                let mut roots = rustls::RootCertStore::empty();
                roots.add(self.ca.der().clone()).unwrap();
                let v = rustls::server::WebPkiClientVerifier::builder_with_provider(
                    Arc::new(roots),
                    provider,
                );
                let v = match clients {
                    Clients::Optional => v.allow_unauthenticated(),
                    _ => v,
                };
                builder.with_client_cert_verifier(v.build().unwrap())
            }
        };
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(self.leaf.1.serialize_der().into());
        builder
            .with_single_cert(vec![self.leaf.0.clone().into(), self.ca.der().clone()], key)
            .unwrap()
    }
}

/// What a test gateway asks of its clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clients {
    /// A certificate under the device CA, as lmgw's device listener while no token is pending.
    Required,
    /// A certificate if there is one, as while a token is pending (enrolment).
    Optional,
    None,
}

/// The tests' gateway (made once: key generation and signing cost time).
pub static PKI: LazyLock<Pki> = LazyLock::new(|| Pki::new("lmgw devices test"));

/// Another gateway, whose CA the paired device does not trust.
pub static FOREIGN: LazyLock<Pki> = LazyLock::new(|| Pki::new("lmgw devices foreign"));

/// A loopback TLS listener as lmgw's device listener serves.
pub struct Gateway {
    listener: TcpListener,
    acceptor: TlsAcceptor,
    pub url: String,
}

impl Gateway {
    pub async fn bind_as(pki: &Pki, clients: Clients) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        Self {
            listener,
            acceptor: TlsAcceptor::from(Arc::new(pki.server_config(clients))),
            url,
        }
    }

    /// The next connection's TLS stream, once its handshake is done.
    pub async fn accept(&self) -> std::io::Result<(TlsStream<TcpStream>, SocketAddr)> {
        let (tcp, peer) = self.listener.accept().await?;
        let tls = self.acceptor.accept(tcp).await?;
        Ok((tls, peer))
    }
}

/// TLS's alert numbers (RFC 8446 §6) a [`Squatter`] sends.
pub const BAD_CERTIFICATE: u8 = 42;
pub const CERTIFICATE_EXPIRED: u8 = 45;
pub const CERTIFICATE_REQUIRED: u8 = 116;

/// A process squatting on lmgw's port (Kai desktop's security review B1): plain TCP that answers
/// each ClientHello with a fatal alert in the clear, before it proved anything.
pub struct Squatter {
    listener: TcpListener,
    pub url: String,
}

impl Squatter {
    pub async fn bind() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        Self { listener, url }
    }

    /// Answers every connection with `alert`, until dropped.
    pub fn serve(self, alert: u8) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            loop {
                let Ok((mut s, _)) = self.listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut head = [0u8; 5];
                    if s.read_exact(&mut head).await.is_err() {
                        return;
                    }
                    let mut hello = vec![0u8; usize::from(u16::from_be_bytes([head[3], head[4]]))];
                    if s.read_exact(&mut hello).await.is_err() {
                        return;
                    }
                    // content type 21 (alert), legacy version 3.3, length 2, fatal, the alert.
                    let _ = s.write_all(&[21, 3, 3, 0, 2, 2, alert]).await;
                    let _ = s.shutdown().await;
                });
            }
        })
    }
}
