use std::sync::{Arc, atomic::Ordering};

use kai_wp0_spike::{Keystore, KeystoreError, keystore_bench, sink_bench, spike_info, tls_bench};

use crate::{CountingSink, SoftKeystore};

/// The test keeps a handle to see what the bench left behind.
struct Shared(Arc<SoftKeystore>);

impl Keystore for Shared {
    fn generate(&self, alias: String) -> Result<Vec<u8>, KeystoreError> {
        self.0.generate(alias)
    }
    fn sign(&self, alias: String, message: Vec<u8>) -> Result<Vec<u8>, KeystoreError> {
        self.0.sign(alias, message)
    }
    fn delete(&self, alias: String) {
        self.0.delete(alias)
    }
}

struct SharedSink(Arc<CountingSink>);

impl kai_wp0_spike::EventSink for SharedSink {
    fn send(&self, text: String) {
        self.0.send(text)
    }
}

#[test]
fn the_sink_is_called_from_every_kind_of_thread() {
    let sink = Arc::new(CountingSink::default());
    let report = sink_bench(Box::new(SharedSink(sink.clone())), 20);
    assert_eq!(report.calling_thread.count, 20);
    assert_eq!(
        report.worker_first_call.count + report.worker_repeat_call.count,
        20
    );
    assert_eq!(report.worker_first_call.count, report.workers_used);
    assert_eq!(report.fresh_thread.count, 20);
    assert_eq!(sink.0.load(Ordering::Relaxed), 60);
    assert!(report.payload_bytes > 100);
}

#[test]
fn keystore_signatures_verify_and_the_key_is_deleted() {
    let ks = Arc::new(SoftKeystore::default());
    let report = keystore_bench(Box::new(Shared(ks.clone())), "spike-ks".into(), 5).unwrap();
    assert_eq!(report.spki_bytes, 91);
    assert_eq!(report.sign.count, 5);
    assert_eq!(report.sign_warm.count, 4);
    assert_eq!(report.verified, 5);
    assert!(report.signature_bytes >= 8 && report.signature_bytes <= 72);
    assert_eq!(*ks.deleted.lock().unwrap(), vec!["spike-ks".to_owned()]);
}

#[test]
fn a_full_tls13_handshake_with_the_keystore_key_as_client_certificate() {
    let ks = Arc::new(SoftKeystore::default());
    let report = tls_bench(Box::new(Shared(ks.clone())), "spike-tls".into(), 3).unwrap();
    assert_eq!(report.handshakes.len(), 3);
    assert!(report.client_cert_verified, "{report:?}");
    assert_eq!(report.protocol, "TLSv1_3");
    assert!(
        report.cipher_suite.starts_with("TLS13_"),
        "{}",
        report.cipher_suite
    );
    // One signature per handshake: the client's CertificateVerify.
    assert!(report.handshakes.iter().all(|h| h.sign_us > 0.0));
    assert_eq!(report.handshake_warm.count, 2);
    assert_eq!(*ks.deleted.lock().unwrap(), vec!["spike-tls".to_owned()]);
}

#[test]
fn a_keystore_that_fails_fails_the_bench_with_its_words() {
    struct Broken;
    impl Keystore for Broken {
        fn generate(&self, _: String) -> Result<Vec<u8>, KeystoreError> {
            Err(KeystoreError::Failed {
                reason: "no TEE".into(),
            })
        }
        fn sign(&self, _: String, _: Vec<u8>) -> Result<Vec<u8>, KeystoreError> {
            unreachable!()
        }
        fn delete(&self, _: String) {}
    }
    let e = tls_bench(Box::new(Broken), "x".into(), 2).unwrap_err();
    assert_eq!(e.to_string(), "generate the device key: no TEE");
}

#[test]
fn spike_info_names_the_build() {
    let info = spike_info();
    assert!(info.tokio_workers >= 1);
    assert_eq!(info.target_arch, std::env::consts::ARCH);
}
