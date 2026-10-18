//! The device key (design §3.2, §4.3): a non-extractable P-256 key in the Android Keystore, which
//! Kotlin generates under the alias the core names and signs with. Across the FFI travel only its
//! DER SubjectPublicKeyInfo (once, at generation), the handshake message to sign and its DER
//! signature; none of it is secret. [`KeystoreKey`] is rustls's signing key over the callback
//! (design M7).

use std::sync::Arc;

use rustls::{
    SignatureAlgorithm, SignatureScheme,
    pki_types::SubjectPublicKeyInfoDer,
    sign::{Signer, SigningKey},
};

/// A Keystore operation that failed on Kotlin's side, in its words.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum KeystoreError {
    // (`reason`, not `message`: UniFFI's Kotlin errors extend Throwable.)
    /// The key is gone for good (`KeyPermanentlyInvalidatedException`, `UnrecoverableKeyException`,
    /// no entry under the alias): the pairing is useless, pair again.
    #[error("{reason}")]
    Gone { reason: String },
    /// Any other failure (the Keystore busy, a binder error, …): tried again.
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

/// Kotlin's Keystore (design §3.2, §4.3): `KeyPairGenerator` for EC in `AndroidKeyStore`
/// (`PURPOSE_SIGN`, `secp256r1`, SHA-256, TEE, no user authentication), `Signature
/// ("SHA256withECDSA")`.
#[uniffi::export(callback_interface)]
pub trait Keystore: Send + Sync {
    /// A new EC P-256 signing key under `alias`; its DER SubjectPublicKeyInfo.
    fn generate(&self, alias: String) -> Result<Vec<u8>, KeystoreError>;
    /// `SHA256withECDSA` over `message` with the key under `alias`; the DER signature.
    fn sign(&self, alias: String, message: Vec<u8>) -> Result<Vec<u8>, KeystoreError>;
    /// Deletes the key under `alias` (nothing when there is none).
    fn delete(&self, alias: String);
    /// The aliases of Kai's device keys the Keystore holds (those starting with [`KEY_PREFIX`]):
    /// the core deletes the ones neither the stored pairing nor a pending token names (§4.3).
    fn aliases(&self) -> Result<Vec<String>, KeystoreError>;
}

/// The start of every device key's alias.
pub const KEY_PREFIX: &str = "kai-device-";

/// The start of every rustls error a Keystore signature failure becomes: how the link tells a key
/// gone for good ([`SIGN_GONE`]) and any other Keystore failure ([`SIGN_FAILED`]) from any other
/// handshake failure ([`crate::link::failure`]).
pub(crate) const SIGN_GONE: &str = "the Keystore's key is gone: ";
pub(crate) const SIGN_FAILED: &str = "the Keystore's signature failed: ";

/// rustls's signing key over the Keystore callback (design M7): scheme `ECDSA_NISTP256_SHA256`,
/// its public key the stored SubjectPublicKeyInfo. `Signer::sign` is synchronous, so a handshake
/// blocks one tokio worker for one Keystore signature.
pub struct KeystoreKey {
    keystore: Arc<dyn Keystore>,
    alias: String,
    spki: Vec<u8>,
}

impl KeystoreKey {
    pub fn new(keystore: Arc<dyn Keystore>, alias: String, spki: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            keystore,
            alias,
            spki,
        })
    }
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
        self.keystore
            .sign(self.alias.clone(), message.to_vec())
            .map_err(|e| {
                rustls::Error::General(match e {
                    KeystoreError::Gone { reason } => format!("{SIGN_GONE}{reason}"),
                    KeystoreError::Failed { reason } => format!("{SIGN_FAILED}{reason}"),
                })
            })
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ECDSA_NISTP256_SHA256
    }
}

/// A fresh alias for a device key: `kai-device-` and 16 random hex digits.
pub(crate) fn fresh_alias() -> String {
    let mut bytes = [0u8; 8];
    let random = rustls::crypto::ring::default_provider().secure_random;
    if random.fill(&mut bytes).is_err() {
        // The system's random source failed; the time still names a key no other has.
        let t = crate::clock::Clock::wall_ms(&crate::clock::SystemClock);
        bytes = t.to_be_bytes();
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{KEY_PREFIX}{hex}")
}

/// A software P-256 key behind the [`Keystore`] callback, for the host tests (never on a phone):
/// keys by alias, signatures by ring, every call counted.
pub mod soft {
    use std::{
        collections::HashMap,
        sync::{
            Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use ring::{
        rand::SystemRandom,
        signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair},
    };

    use super::{Keystore, KeystoreError};

    /// The DER SubjectPublicKeyInfo of a P-256 key starts with these 26 bytes; the 65-byte
    /// uncompressed point follows.
    const P256_SPKI_PREFIX: [u8; 26] = [
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
        0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
    ];

    #[derive(Default)]
    pub struct SoftKeystore {
        keys: Mutex<HashMap<String, EcdsaKeyPair>>,
        pub generated: AtomicUsize,
        pub signed: AtomicUsize,
        pub deleted: Mutex<Vec<String>>,
        /// Every signature fails while set: the key gone from the Keystore.
        pub fail_signing: AtomicBool,
        /// Every signature fails while set, the key still there (the Keystore busy).
        pub fail_signing_transient: AtomicBool,
    }

    impl SoftKeystore {
        pub fn new() -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self::default())
        }

        /// The aliases that hold a key now.
        pub fn aliases(&self) -> Vec<String> {
            let mut v: Vec<String> = self.keys.lock().unwrap().keys().cloned().collect();
            v.sort();
            v
        }
    }

    impl Keystore for SoftKeystore {
        fn generate(&self, alias: String) -> Result<Vec<u8>, KeystoreError> {
            let rng = SystemRandom::new();
            let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
                .map_err(|_| KeystoreError::Failed {
                    reason: "ring refused to generate".into(),
                })?;
            let pair =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                    .map_err(|e| KeystoreError::Failed {
                        reason: e.to_string(),
                    })?;
            let mut spki = P256_SPKI_PREFIX.to_vec();
            spki.extend_from_slice(pair.public_key().as_ref());
            self.keys.lock().unwrap().insert(alias, pair);
            self.generated.fetch_add(1, Ordering::SeqCst);
            Ok(spki)
        }

        fn sign(&self, alias: String, message: Vec<u8>) -> Result<Vec<u8>, KeystoreError> {
            if self.fail_signing.load(Ordering::SeqCst) {
                return Err(KeystoreError::Gone {
                    reason: format!("KeyPermanentlyInvalidatedException: {alias}"),
                });
            }
            if self.fail_signing_transient.load(Ordering::SeqCst) {
                return Err(KeystoreError::Failed {
                    reason: "KeyStoreException: the Keystore is busy".into(),
                });
            }
            let keys = self.keys.lock().unwrap();
            let pair = keys.get(&alias).ok_or_else(|| KeystoreError::Gone {
                reason: format!("no key under {alias}"),
            })?;
            let sig =
                pair.sign(&SystemRandom::new(), &message)
                    .map_err(|_| KeystoreError::Failed {
                        reason: "ring refused to sign".into(),
                    })?;
            self.signed.fetch_add(1, Ordering::SeqCst);
            Ok(sig.as_ref().to_vec())
        }

        fn delete(&self, alias: String) {
            self.keys.lock().unwrap().remove(&alias);
            self.deleted.lock().unwrap().push(alias);
        }

        fn aliases(&self) -> Result<Vec<String>, KeystoreError> {
            Ok(self
                .aliases()
                .into_iter()
                .filter(|a| a.starts_with(super::KEY_PREFIX))
                .collect())
        }
    }
}
