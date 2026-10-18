//! The one constructor (design M2, M7; device link D15): every lmgw-facing client is built from
//! the pinned configuration, refuses a server under another CA, and speaks https only.

use std::{sync::Arc, time::Duration};

use kai_mobile_core::{
    keystore::{Keystore, KeystoreKey, soft::SoftKeystore},
    link::{
        failure::Failure,
        http::{self, Handle, NoAnswer},
        tls::Tls,
    },
    lmgw::{requests, tls::TlsRefusal},
};

use super::{
    mock::Mock,
    pki::{Clients, FOREIGN, PKI},
    rig::paired_file,
};

fn paired_tls(keystore: &Arc<SoftKeystore>, url: &str) -> Tls {
    let f = paired_file(&PKI, keystore, &[url], (2020, 2100));
    let key = KeystoreKey::new(keystore.clone(), f.alias.clone(), f.spki().unwrap());
    Tls::paired(&f.credentials, key, Duration::from_secs(2)).unwrap()
}

/// The paired client presents the Keystore key's certificate (each handshake one signature through
/// the callback) and is served.
#[tokio::test]
async fn the_paired_client_presents_the_keystore_key() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![]);
    let ks = SoftKeystore::new();
    let tls = paired_tls(&ks, &mock.url);
    let h = Handle {
        tls,
        base: mock.url.clone(),
    };
    let (status, _) = http::call(&h, &requests::folders(), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(status, 200);
    assert!(mock.requests("/chat/api/folders")[0].cert);
    assert_eq!(ks.signed.load(std::sync::atomic::Ordering::SeqCst), 1);
    // The pool keeps the connection: a second request signs nothing more.
    http::call(&h, &requests::folders(), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(ks.signed.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// Every lmgw-facing client refuses a server under another CA: a request and a stream on the
/// paired configuration (an unknown issuer: "authority changed"), the enrolment's (not the lmgw
/// the link pairs with); nothing is sent.
#[tokio::test]
async fn every_client_refuses_a_server_under_another_ca() {
    let foreign = Mock::start(&FOREIGN, Clients::None).await;
    let ks = SoftKeystore::new();
    let h = Handle {
        tls: paired_tls(&ks, &foreign.url),
        base: foreign.url.clone(),
    };
    let e = http::call(&h, &requests::folders(), Duration::from_secs(2))
        .await
        .unwrap_err();
    assert_eq!(
        e,
        NoAnswer::Failed(Failure::Tls(TlsRefusal::AuthorityChanged))
    );
    let req = kai_mobile_core::conversation::send::send_request(7, "hi");
    let e = kai_mobile_core::conversation::send::stream(&h, &req, Duration::from_secs(2), |_| {})
        .await
        .unwrap_err();
    assert!(
        matches!(
            e,
            kai_mobile_core::conversation::send::StreamFail::NoAnswer(NoAnswer::Failed(
                Failure::Tls(TlsRefusal::AuthorityChanged)
            ))
        ),
        "{e:?}"
    );
    let enrol = Tls::enrolment(super::pki::fp(&PKI.ca), Duration::from_secs(2)).unwrap();
    let h = Handle {
        tls: enrol,
        base: foreign.url.clone(),
    };
    let e = http::call(&h, &requests::folders(), Duration::from_secs(2))
        .await
        .unwrap_err();
    assert_eq!(e, NoAnswer::Failed(Failure::Tls(TlsRefusal::NotPairedLmgw)));
    assert!(foreign.requests("/").is_empty());
    assert_eq!(
        ks.signed.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "no certificate was sent"
    );
}

/// Plain http is never spoken.
#[tokio::test]
async fn plain_http_is_refused() {
    let ks = SoftKeystore::new();
    let h = Handle {
        tls: paired_tls(&ks, "https://127.0.0.1:1"),
        base: "http://127.0.0.1:9".into(),
    };
    let e = http::call(&h, &requests::folders(), Duration::from_secs(2))
        .await
        .unwrap_err();
    assert!(matches!(e, NoAnswer::Failed(Failure::Other(_))), "{e:?}");
}

#[test]
fn lmgw_client_is_linked() {
    let bytes = [0u8, 1, 2, 250, 251, 252];
    let text = kai_mobile_core::lmgw::base64::encode(&bytes);
    assert_eq!(kai_mobile_core::lmgw::base64::decode(&text).unwrap(), bytes);
    let ks = SoftKeystore::new();
    let spki = ks.generate("x".into()).unwrap();
    assert_eq!(spki.len(), 91);
}
