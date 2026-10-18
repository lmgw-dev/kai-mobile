//! The device link (design §4, §15.3): enrolment's answer and its refusals, the pinned verifiers,
//! one key per token, the confirmation's wait, the certificate's dates.

use std::time::Duration;

use kai_mobile_core::{
    CoreError, Keystore, PairingFailure,
    lmgw::types::device_link::EnrolRequest,
    views::{ConnectionAction, ConnectionState},
};

use super::{
    mock::{Enrol, Mock},
    pki::{CERTIFICATE_REQUIRED, Clients, FOREIGN, PKI, Pki, Squatter, device_cert, fp},
    rig::{Rig, caught_up, hello},
};

const TOKEN: &str = "lmgw-enrol-0123456789abcdef0123456789abcdef";

fn link(urls: &[&str], pki: &Pki) -> String {
    let urls: String = urls
        .iter()
        .map(|u| format!("&url={}", u.replace(':', "%3A").replace('/', "%2F")))
        .collect();
    format!(
        "lmgw-pair:?v=2&name=phone{urls}&fp={}&token={TOKEN}",
        fp(&pki.ca)
    )
}

/// lmgw's enrolment as `pki`'s gateway answers it at `url` (device link §3.4).
fn enrolment(pki: &Pki, url: &str, body: &[u8], confirmed: bool) -> String {
    let req: EnrolRequest = serde_json::from_slice(body).unwrap();
    let spki = kai_mobile_core::lmgw::base64::decode(&req.public_key).unwrap();
    let cert = device_cert(&pki.ca, &spki, (2020, 2100));
    serde_json::json!({
        "device": {"id": 12, "name": "phone"},
        "certificate": kai_mobile_core::lmgw::base64::encode(&cert),
        "ca_certificate": kai_mobile_core::lmgw::base64::encode(pki.ca.der()),
        "not_after": "2100-01-01T00:00:00Z",
        "notice_after": "2099-01-01T00:00:00Z",
        "urls": [url, "http://198.51.100.20:8787"],
        "confirm_code": kai_mobile_core::lmgw::requests::confirm_code(&spki),
        "confirmed": confirmed,
    })
    .to_string()
}

fn answering(mock: &Mock, confirmed: bool) {
    let url = mock.url.clone();
    mock.state.lock().unwrap().enrol = Some(Box::new(move |body| {
        Enrol::Answer(201, enrolment(&PKI, &url, body, confirmed))
    }));
}

/// Enrolment's answer: the pairing is stored (the https URLs only), the code shown, the feed waits
/// for the owner's Confirm (403 `device_unconfirmed`) presenting the Keystore key's certificate,
/// then `hello` connects it. The token never reaches the log of the stored files.
#[tokio::test]
async fn enrolment_then_the_confirmation_s_wait_then_connected() {
    let mock = Mock::start(&PKI, Clients::Optional).await;
    answering(&mock, false);
    mock.state.lock().unwrap().feed_answers.push_back((
        403,
        r#"{"code":"device_unconfirmed","message":"confirm the code on the card"}"#.into(),
    ));
    let mut rig = Rig::unpaired().await;
    rig.connection("not paired", |v| {
        v.state == ConnectionState::NotPaired && v.actions == [ConnectionAction::Pair]
    })
    .await;
    let preview = rig
        .core
        .pairing_preview(link(&[&mock.url], &PKI))
        .await
        .unwrap();
    assert_eq!(preview.name, "phone");
    assert_eq!(preview.replaces, None);
    let paired = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap();
    assert!(!paired.confirmed);
    assert_eq!(
        paired.urls,
        std::slice::from_ref(&mock.url),
        "the http URL is dropped"
    );
    let stored = rig.link_file().expect("link.json");
    assert_eq!(stored.credentials.confirm_code, paired.confirm_code);
    assert_eq!(stored.cursor, None, "the first feed starts at now");
    let text = std::fs::read_to_string(rig.dir.path().join("link.json")).unwrap();
    assert!(!text.contains(TOKEN), "the token is never stored");
    assert!(rig.core.paired());

    let v = rig
        .connection("waiting", |v| {
            v.state == ConnectionState::WaitingForConfirmation
        })
        .await;
    assert_eq!(
        v.confirm_code.as_deref(),
        Some(paired.confirm_code.as_str())
    );
    assert!(v.words.contains(&paired.confirm_code));
    assert_eq!(
        v.actions,
        [ConnectionAction::CheckNow, ConnectionAction::PairAgain]
    );
    // Check now asks at once.
    rig.core.retry();
    let feed = mock.feed().await;
    assert!(feed.req.cert, "the feed presents the device's certificate");
    assert_eq!(feed.req.path, "/chat/api/feed?keepalive_s=120");
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    let v = rig
        .connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    assert_eq!(v.device_name.as_deref(), Some("phone"));
    let enrols = mock.requests("/devices/enrol");
    assert_eq!(enrols.len(), 1);
    assert!(
        !enrols[0].cert,
        "enrolment goes without a client certificate"
    );
    let body: EnrolRequest = serde_json::from_slice(&enrols[0].body).unwrap();
    assert_eq!(
        body.client.as_deref(),
        Some("Kai mobile 0.1.0+test (Android 16)")
    );
}

/// Each refusal of the enrolment reads by its code, nothing is stored and the new key goes.
#[tokio::test]
async fn enrolment_refusals_read_by_code_and_store_nothing() {
    for (status, code, kind) in [
        (410, "pairing_token_used", PairingFailure::CodeUsedElsewhere),
        (410, "pairing_token_expired", PairingFailure::LinkExpired),
        (401, "pairing_token_unknown", PairingFailure::LinkUnknown),
        (409, "public_key_in_use", PairingFailure::Refused),
        (401, "device_disabled", PairingFailure::Refused),
    ] {
        let mock = Mock::start(&PKI, Clients::Optional).await;
        let body = format!(r#"{{"code":"{code}","message":"lmgw says {code}"}}"#);
        mock.state.lock().unwrap().enrol =
            Some(Box::new(move |_| Enrol::Answer(status, body.clone())));
        let rig = Rig::unpaired().await;
        let err = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap_err();
        let CoreError::Pairing { kind: got, reason } = err else {
            panic!("{code}: {err:?}")
        };
        assert_eq!(got, kind, "{code}: {reason}");
        assert!(rig.link_file().is_none(), "{code}: nothing stored");
        assert!(
            rig.keystore.aliases().is_empty(),
            "{code}: the new key is deleted"
        );
        assert!(!rig.core.paired());
        if code == "pairing_token_used" {
            assert!(
                reason.contains("Someone else used this pairing code"),
                "{reason}"
            );
        }
    }
}

/// An answer whose CA is not the one `fp` pins is "not this lmgw" and stores nothing.
#[tokio::test]
async fn an_answer_with_another_ca_is_refused() {
    let mock = Mock::start(&PKI, Clients::Optional).await;
    let url = mock.url.clone();
    mock.state.lock().unwrap().enrol = Some(Box::new(move |body| {
        // The right server, a CA certificate that is not the pinned one.
        let text = enrolment(&PKI, &url, body, false);
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["ca_certificate"] = kai_mobile_core::lmgw::base64::encode(FOREIGN.ca.der()).into();
        Enrol::Answer(201, v.to_string())
    }));
    let rig = Rig::unpaired().await;
    let err = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap_err();
    assert!(
        matches!(
            err,
            CoreError::Pairing {
                kind: PairingFailure::NotThisLmgw,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(rig.link_file().is_none());
    assert_eq!(
        rig.keystore.aliases().len(),
        1,
        "lmgw-client's own check keeps the key for the retry"
    );
}

/// Only a 4xx refusal with lmgw's code deletes the enrolment's key: a 5xx, a 429 and a 4xx
/// without lmgw's code keep it, and the retry sends the same key.
#[tokio::test]
async fn a_5xx_or_a_429_keeps_the_key_for_the_retry() {
    for (status, body) in [
        (
            503,
            r#"{"code":"devices_ca_unavailable","message":"the device CA is not readable"}"#,
        ),
        (
            429,
            r#"{"code":"pairing_rate_limited","message":"too many enrolments"}"#,
        ),
        (500, "internal error"),
        (404, "not here"),
    ] {
        let mock = Mock::start(&PKI, Clients::Optional).await;
        let url = mock.url.clone();
        let mut first = true;
        let body = body.to_owned();
        mock.state.lock().unwrap().enrol = Some(Box::new(move |req| {
            if std::mem::take(&mut first) {
                Enrol::Answer(status, body.clone())
            } else {
                Enrol::Answer(201, enrolment(&PKI, &url, req, true))
            }
        }));
        let rig = Rig::unpaired().await;
        let err = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap_err();
        assert!(
            matches!(
                err,
                CoreError::Pairing {
                    kind: PairingFailure::Refused,
                    ..
                }
            ),
            "{status}: {err:?}"
        );
        assert!(rig.link_file().is_none(), "{status}: nothing stored");
        assert_eq!(rig.keystore.aliases().len(), 1, "{status}: the key stays");
        rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap();
        let keys: Vec<String> = mock
            .requests("/devices/enrol")
            .iter()
            .map(|r| {
                serde_json::from_slice::<EnrolRequest>(&r.body)
                    .unwrap()
                    .public_key
            })
            .collect();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], keys[1], "{status}: the same key again");
    }
}

/// The enrolment's verifier against a server holding no certificate that hashes to `fp`: nothing
/// is sent to it.
#[tokio::test]
async fn a_server_under_another_ca_is_not_the_lmgw_the_link_pairs_with() {
    let foreign = Mock::start(&FOREIGN, Clients::Optional).await;
    answering(&foreign, false);
    let rig = Rig::unpaired().await;
    let err = rig
        .core
        .pair(link(&[&foreign.url], &PKI))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            CoreError::Pairing {
                kind: PairingFailure::NotThisLmgw,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(foreign.requests("/").is_empty(), "nothing was sent");
}

/// One key per token (device link D8): an answer lost on the way is asked for again with the same
/// key, which gets the same certificate; a failure that reached no answer keeps the key.
#[tokio::test]
async fn a_lost_answer_is_asked_again_with_the_same_key() {
    let mock = Mock::start(&PKI, Clients::Optional).await;
    let url = mock.url.clone();
    let mut first = true;
    mock.state.lock().unwrap().enrol = Some(Box::new(move |body| {
        if std::mem::take(&mut first) {
            Enrol::Drop
        } else {
            Enrol::Answer(201, enrolment(&PKI, &url, body, true))
        }
    }));
    let rig = Rig::unpaired().await;
    let err = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap_err();
    assert!(
        matches!(
            err,
            CoreError::Pairing {
                kind: PairingFailure::Unreachable,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(
        rig.keystore.aliases().len(),
        1,
        "the key stays for the retry"
    );
    let paired = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap();
    assert!(paired.confirmed);
    let keys: Vec<String> = mock
        .requests("/devices/enrol")
        .iter()
        .map(|r| {
            serde_json::from_slice::<EnrolRequest>(&r.body)
                .unwrap()
                .public_key
        })
        .collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1], "the same key both times");
    assert_eq!(
        rig.keystore
            .generated
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

/// A `certificate_required` alert at enrolment (lmgw-client's `Unproven`): a failed handshake to
/// try again; a new link is suggested, never stated; the key stays.
#[tokio::test]
async fn a_certificate_required_alert_at_enrolment_is_unproven() {
    let squatter = Squatter::bind().await;
    let url = squatter.url.clone();
    let _s = squatter.serve(CERTIFICATE_REQUIRED);
    let rig = Rig::unpaired().await;
    let err = rig.core.pair(link(&[&url], &PKI)).await.unwrap_err();
    let CoreError::Pairing { kind, reason } = err else {
        panic!()
    };
    assert_eq!(kind, PairingFailure::Unproven);
    assert!(
        reason.contains("Try again") && reason.contains("if it keeps failing"),
        "{reason}"
    );
    assert_eq!(rig.keystore.aliases().len(), 1);
}

/// A v1 link is told it is from before TLS.
#[tokio::test]
async fn a_v1_link_is_from_before_tls() {
    let rig = Rig::unpaired().await;
    let err = rig
        .core
        .pairing_preview("lmgw-pair:?v=1&url=http%3A%2F%2Fh&name=p&key=lmgw-device-x".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("before TLS"), "{err}");
}

/// A re-pair with another link replaces the pairing: the old key goes once the new one is stored.
#[tokio::test]
async fn a_re_pair_replaces_the_key() {
    let mock = Mock::start(&PKI, Clients::Optional).await;
    answering(&mock, true);
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let old = rig.link_file().unwrap().alias;
    let preview = rig
        .core
        .pairing_preview(link(&[&mock.url], &PKI))
        .await
        .unwrap();
    assert_eq!(preview.replaces.as_deref(), Some(mock.url.as_str()));
    assert!(preview.lines.iter().any(|l| l.warning));
    rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap();
    let new = rig.link_file().unwrap().alias;
    assert_ne!(old, new);
    assert!(rig.keystore.deleted.lock().unwrap().contains(&old));
    assert_eq!(rig.keystore.aliases(), [new]);
    let _ = rig.connection("connecting", |_| true).await;
}

/// At start every device key that no pairing names is deleted (an enrolment's key whose pairing
/// was never stored, a key an older build left); the stored pairing's and other apps' stay.
#[tokio::test]
async fn orphan_device_keys_are_swept_at_start() {
    let mut rig = Rig::with(
        |ks, _| {
            ks.generate("kai-device-0123456789abcdef".into()).unwrap();
            ks.generate("someone-else".into()).unwrap();
            Some(super::rig::paired_file(
                &PKI,
                ks,
                &["https://127.0.0.1:9"],
                (2020, 2100),
            ))
        },
        super::rig::fast_settings(),
    )
    .await;
    let _ = rig.connection("started", |_| true).await;
    assert_eq!(
        rig.keystore.aliases(),
        ["kai-device-test", "someone-else"],
        "the orphan went"
    );
    // Nothing stored: every device key goes.
    let rig = Rig::with(
        |ks, _| {
            ks.generate("kai-device-fedcba9876543210".into()).unwrap();
            None
        },
        super::rig::fast_settings(),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !rig.keystore.aliases().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("swept");
}

/// A re-pair whose `link.json` cannot be written leaves the old pairing as it was, its key
/// included: nothing of it goes before the new one is stored.
#[tokio::test]
async fn a_re_pair_that_cannot_be_stored_keeps_the_old_pairing() {
    let mock = Mock::start(&PKI, Clients::Optional).await;
    answering(&mock, true);
    let rig = Rig::paired(&PKI, &[&mock.url]).await;
    let old = rig.link_file().unwrap();
    // The atomic write's temp file cannot be made (even as root): link.json cannot be written.
    std::fs::create_dir(rig.dir.path().join("link.json.tmp")).unwrap();
    let err = rig.core.pair(link(&[&mock.url], &PKI)).await.unwrap_err();
    assert!(
        matches!(&err, CoreError::Storage { what, .. } if what.contains("link.json")),
        "{err:?}"
    );
    assert_eq!(rig.link_file().unwrap(), old, "the old pairing stands");
    assert!(
        !rig.keystore.deleted.lock().unwrap().contains(&old.alias),
        "the old key stays"
    );
    assert!(rig.keystore.aliases().contains(&old.alias));
    assert!(rig.core.paired());
}

/// The certificate's dates: from `notice_after` a badge; past `not_after` by the phone's own date
/// the state is lapsed and nothing connects.
#[tokio::test]
async fn the_certificate_s_dates_by_the_phone_s_clock() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let url = mock.url.clone();
    let mut rig = Rig::with(
        |ks, _| {
            let mut f = super::rig::paired_file(&PKI, ks, &[&url], (2020, 2100));
            f.credentials.notice_after_ms = 0;
            Some(f)
        },
        super::rig::fast_settings(),
    )
    .await;
    let v = rig
        .connection("a notice", |v| v.cert_notice.is_some())
        .await;
    assert!(v.cert_notice.unwrap().contains("2100-01-01"));
    drop(rig);

    let mut rig = Rig::with(
        |ks, _| Some(super::rig::paired_file(&PKI, ks, &[&url], (2020, 2021))),
        super::rig::fast_settings(),
    )
    .await;
    let v = rig
        .connection("lapsed", |v| v.state == ConnectionState::CertificateLapsed)
        .await;
    assert!(v.words.contains("2021-01-01"), "{}", v.words);
    assert_eq!(v.actions, [ConnectionAction::PairAgain]);
    assert!(
        mock.feed_within(Duration::from_millis(500)).await.is_none(),
        "nothing connects"
    );
}

/// While the app shows the code, the core asks every `pairing.confirm_poll_s` on the kept-alive
/// connection (§4.2: no new handshake).
#[tokio::test]
async fn the_confirmation_s_poll_keeps_its_connection() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    for _ in 0..3 {
        mock.state.lock().unwrap().feed_answers.push_back((
            403,
            r#"{"code":"device_unconfirmed","message":"confirm the code"}"#.into(),
        ));
    }
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    rig.core.set_foreground(true);
    rig.connection("waiting", |v| {
        v.state == ConnectionState::WaitingForConfirmation
    })
    .await;
    for _ in 0..2 {
        let v = rig
            .connection("the next poll", |v| {
                v.state == ConnectionState::WaitingForConfirmation && v.next_try_at_ms.is_some()
            })
            .await;
        use kai_mobile_core::clock::Clock;
        let d = v.next_try_at_ms.unwrap() - rig.clock.wall_ms();
        assert!(d <= 2_000, "pairing.confirm_poll_s: {d}");
        rig.clock.sleep(2_100);
        rig.core.wake();
    }
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    let polls = mock.requests("/chat/api/feed");
    assert_eq!(polls.len(), 4);
    assert_eq!(
        polls[1].conn, polls[2].conn,
        "the polls share one connection"
    );
    assert_eq!(polls[2].conn, polls[3].conn);
}

/// A refusal of a request made under the earlier pairing does not tear the new one down: requests
/// are tagged with the pairing's identity.
#[tokio::test]
async fn an_old_pairing_s_refusal_is_ignored_after_a_re_pair() {
    let mock = Mock::start(&PKI, Clients::Optional).await;
    mock.with_conversation(5, 7, vec![]);
    answering(&mock, false);
    mock.state.lock().unwrap().send_refusal = Some((
        Duration::from_millis(1500),
        401,
        r#"{"code":"device_key_unknown","message":"the key of device 'phone' was replaced"}"#
            .into(),
    ));
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.next("ready", |u| matches!(u, kai_mobile_core::views::Update::Conversation { view } if view.status == kai_mobile_core::views::ConversationStatus::Ready))
        .await;
    let send = {
        let core = &rig.core;
        core.send_text("hi".into())
    };
    // The re-pair lands while the old pairing's send waits for its answer.
    let (sent, paired) = tokio::join!(send, async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        rig.core.pair(link(&[&mock.url], &PKI)).await
    });
    assert!(paired.is_ok(), "{paired:?}");
    assert!(sent.is_err(), "the old send was refused");
    assert!(
        rig.within(Duration::from_millis(800), |u| matches!(u, kai_mobile_core::views::Update::Connection { view } if view.state == ConnectionState::KeyRefused))
            .await
            .is_none(),
        "the new pairing stands"
    );
    drop(feed);
}
