//! The `Keystore` callback of design §3.2: Kotlin generates a P-256 key in the Android Keystore
//! and signs with `SHA256withECDSA`; the core sees the DER SubjectPublicKeyInfo and DER
//! signatures. [`keystore_bench`] times the generation and the signatures (the first one cold)
//! from a tokio worker, as the core will call it, and verifies every signature with ring.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{SpikeError, Timings, runtime};

/// A Keystore operation that failed on Kotlin's side, in its words.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KeystoreError {
    #[error("{reason}")]
    Failed { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for KeystoreError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed {
            reason: format!("the Keystore callback threw: {}", e.reason),
        }
    }
}

/// Kotlin's Keystore (design §3.2, §4.3).
#[uniffi::export(callback_interface)]
pub trait Keystore: Send + Sync {
    /// A new EC P-256 signing key under `alias`; its DER SubjectPublicKeyInfo.
    fn generate(&self, alias: String) -> Result<Vec<u8>, KeystoreError>;
    /// `SHA256withECDSA` over `message` with the key under `alias`; the DER signature.
    fn sign(&self, alias: String, message: Vec<u8>) -> Result<Vec<u8>, KeystoreError>;
    fn delete(&self, alias: String);
}

/// The DER SubjectPublicKeyInfo of a P-256 key starts with these 26 bytes; the 65-byte
/// uncompressed point follows.
const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

/// The uncompressed point of a P-256 SubjectPublicKeyInfo, or why it is not one.
pub(crate) fn p256_point(spki: &[u8]) -> Result<&[u8], String> {
    match spki.strip_prefix(&P256_SPKI_PREFIX[..]) {
        Some(point) if point.len() == 65 && point[0] == 0x04 => Ok(point),
        _ => Err(format!(
            "not a P-256 SubjectPublicKeyInfo ({} bytes)",
            spki.len()
        )),
    }
}

/// Whether `signature` (DER) is `SHA256withECDSA` over `message` by the key `point`.
pub(crate) fn verify(point: &[u8], message: &[u8], signature: &[u8]) -> bool {
    ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_ASN1, point)
        .verify(message, signature)
        .is_ok()
}

/// What [`keystore_bench`] measured.
#[derive(Debug, Clone, uniffi::Record)]
pub struct KeystoreReport {
    pub generate_us: f64,
    pub spki_bytes: u32,
    /// Every signature in order; `first_us` is the key's first (cold) one.
    pub sign: Timings,
    /// The signatures after the first.
    pub sign_warm: Timings,
    pub verified: u32,
    pub signature_bytes: u32,
}

/// Generates a key under `alias`, signs `signs` messages of TLS 1.3 CertificateVerify size (130
/// bytes) from a tokio worker, verifies each, and deletes the key.
#[uniffi::export]
pub fn keystore_bench(
    keystore: Box<dyn Keystore>,
    alias: String,
    signs: u32,
) -> Result<KeystoreReport, SpikeError> {
    let keystore: Arc<dyn Keystore> = Arc::from(keystore);
    let signs = signs.max(2);
    let work = {
        let keystore = keystore.clone();
        let alias = alias.clone();
        move || -> Result<KeystoreReport, SpikeError> {
            let t = Instant::now();
            let spki = keystore
                .generate(alias.clone())
                .map_err(|e| SpikeError::at("generate", e))?;
            let generate = t.elapsed();
            let point = p256_point(&spki).map_err(|e| SpikeError::at("generate", e))?;
            let mut took = Vec::with_capacity(signs as usize);
            let mut verified = 0;
            let mut signature_bytes = 0;
            for i in 0..signs {
                // 64 spaces, the context string, a zero byte and a SHA-256 transcript hash: the
                // shape of what TLS 1.3 signs.
                let mut message = vec![0x20u8; 64];
                message.extend_from_slice(b"TLS 1.3, client CertificateVerify\0");
                message.extend((0..32u32).map(|b| (b * 7 + i) as u8));
                let t = Instant::now();
                let signature = keystore
                    .sign(alias.clone(), message.clone())
                    .map_err(|e| SpikeError::at("sign", e))?;
                took.push(t.elapsed());
                signature_bytes = signature.len();
                if verify(point, &message, &signature) {
                    verified += 1;
                }
            }
            Ok(KeystoreReport {
                generate_us: generate.as_secs_f64() * 1e6,
                spki_bytes: u32::try_from(spki.len()).unwrap_or(u32::MAX),
                sign: Timings::of(&took),
                sign_warm: Timings::of(&took[1..]),
                verified,
                signature_bytes: u32::try_from(signature_bytes).unwrap_or(u32::MAX),
            })
        }
    };
    let result = on_worker(work);
    keystore.delete(alias);
    result
}

/// Runs `f` on a tokio worker thread and waits for it.
pub(crate) fn on_worker<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, SpikeError> + Send + 'static,
) -> Result<T, SpikeError> {
    runtime::get().block_on(async move {
        tokio::spawn(async move { f() })
            .await
            .map_err(|e| SpikeError::at("worker", e))?
    })
}

/// A duration in microseconds.
pub(crate) fn us(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}
