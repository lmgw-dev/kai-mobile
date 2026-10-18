//! The spike's measurements on the host, with a software Keystore in place of Android's: each
//! runs end to end and reports sane facts. The phone's numbers come from the spike app.

mod bench;

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use kai_wp0_spike::{EventSink, Keystore, KeystoreError};
use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair},
};

/// The DER SubjectPublicKeyInfo prefix of a P-256 key (the 65-byte point follows).
const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

/// A Keystore in memory: ring's P-256 keys, `SHA256withECDSA` as DER.
#[derive(Default)]
pub struct SoftKeystore {
    keys: Mutex<HashMap<String, EcdsaKeyPair>>,
    pub deleted: Mutex<Vec<String>>,
}

impl Keystore for SoftKeystore {
    fn generate(&self, alias: String) -> Result<Vec<u8>, KeystoreError> {
        let rng = SystemRandom::new();
        let fail = |e: &dyn std::fmt::Display| KeystoreError::Failed {
            reason: e.to_string(),
        };
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .map_err(|e| fail(&e))?;
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .map_err(|e| fail(&e))?;
        let mut spki = P256_SPKI_PREFIX.to_vec();
        spki.extend_from_slice(key.public_key().as_ref());
        self.keys.lock().unwrap().insert(alias, key);
        Ok(spki)
    }

    fn sign(&self, alias: String, message: Vec<u8>) -> Result<Vec<u8>, KeystoreError> {
        let keys = self.keys.lock().unwrap();
        let key = keys.get(&alias).ok_or_else(|| KeystoreError::Failed {
            reason: format!("no key {alias}"),
        })?;
        key.sign(&SystemRandom::new(), &message)
            .map(|s| s.as_ref().to_vec())
            .map_err(|e| KeystoreError::Failed {
                reason: e.to_string(),
            })
    }

    fn delete(&self, alias: String) {
        self.keys.lock().unwrap().remove(&alias);
        self.deleted.lock().unwrap().push(alias);
    }
}

/// A sink that counts.
#[derive(Default)]
pub struct CountingSink(pub AtomicU32);

impl EventSink for CountingSink {
    fn send(&self, text: String) {
        assert!(text.starts_with('{'));
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
