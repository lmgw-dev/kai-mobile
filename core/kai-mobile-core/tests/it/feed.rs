//! The link and the held feed (design §5, §12, §15.3): the URL race, the reconnects, the waits and
//! their deadlines on a clock that jumps across a sleep, the backoff's two ceilings, the refusals
//! and revocation kinds, the cursor, `hello`'s device facts.

use std::{sync::atomic::Ordering, time::Duration};

use kai_mobile_core::views::{ConnectionAction, ConnectionState, KeyRefusal, NetworkFacts, Update};
use serde_json::json;

use super::{
    mock::Mock,
    pki::{BAD_CERTIFICATE, CERTIFICATE_EXPIRED, Clients, FOREIGN, PKI, Pki, Squatter},
    rig::{Rig, caught_up, hello, hello_with, stored},
};

/// A URL nothing listens at: the connection is refused.
async fn closed_url() -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://{}", l.local_addr().unwrap());
    drop(l);
    url
}

/// The cursor is written after each stored event and resumed with `Last-Event-ID` after a restart;
/// the first feed after a pairing starts at now.
#[tokio::test]
async fn the_cursor_is_kept_and_resumed() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    assert_eq!(feed.req.header("last-event-id"), None, "from now");
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    feed.send(&stored(
        "e1:5:bb",
        "thread.deleted",
        json!({"thread_id": 99, "deleted": true, "by": null}),
    ));
    feed.send(": keep-alive\nid: e1:6:cc\n\n");
    tokio::time::timeout(Duration::from_secs(5), async {
        while rig.link_file().unwrap().cursor.as_deref() != Some("e1:6:cc") {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the cursor is written");
    drop(feed);
    let rig = rig.restart();
    let feed = mock.feed().await;
    assert_eq!(feed.req.header("last-event-id"), Some("e1:6:cc"));
    drop(rig);
}

/// `hello` says what lmgw holds about the device (device link §7.2): its URLs replace the list
/// (the `https` ones only), its certificate's dates the stored ones; the hold is a badge.
#[tokio::test]
async fn hello_refreshes_the_urls_and_the_certificate_s_dates() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello_with(
        "e1:4:aa",
        &[
            &mock.url,
            "http://198.51.100.20:8787",
            "https://192.0.2.1:8788",
        ],
        Some(("2099-10-14T14:03:11Z", "2099-06-14T14:03:11Z")),
    ));
    let v = rig
        .connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    assert!(v.held);
    assert_eq!(v.hold_fallback.as_deref(), Some("cloud"));
    let g = v.gateway.unwrap();
    assert_eq!(
        g.urls,
        [mock.url.clone(), "https://192.0.2.1:8788".to_owned()]
    );
    assert_eq!(g.not_after, "2099-10-14");
    assert_eq!(
        g.dashboard_url.as_deref(),
        Some("http://198.51.100.20:8001/")
    );
    let stored = rig.link_file().unwrap();
    assert_eq!(stored.credentials.urls.as_slice().len(), 2);
    assert_eq!(stored.last_good.as_deref(), Some(mock.url.as_str()));
}

/// The URL race (M5): the next URL a stagger after the previous one when it gives no TCP answer,
/// the first verified handshake wins and goes first next time.
#[tokio::test]
async fn the_race_tries_the_next_url_after_the_stagger_and_remembers_the_winner() {
    // A black hole: accepts TCP, never answers TLS.
    let hole = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hole_url = format!("https://{}", hole.local_addr().unwrap());
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let acc = accepted.clone();
    let _h = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = hole.accept().await {
            acc.fetch_add(1, Ordering::SeqCst);
            held.push(s);
        }
    });
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&hole_url, &mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    assert_eq!(
        rig.link_file().unwrap().last_good.as_deref(),
        Some(mock.url.as_str())
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    drop(feed);
    // The next connection goes to the winner first; the black hole is not raced within the
    // stagger while it answers.
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "the last good URL went first"
    );
}

/// A new default network drops the connection and races again at once (§5.2).
#[tokio::test]
async fn a_new_network_reconnects_at_once() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    rig.core.set_network(NetworkFacts {
        available: true,
        id: "100".into(),
        transport: "wifi".into(),
    });
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    let before = mock.connections();
    rig.core.set_network(NetworkFacts {
        available: true,
        id: "101".into(),
        transport: "cellular".into(),
    });
    let feed2 = mock.feed().await;
    assert_eq!(feed2.req.header("last-event-id"), Some("e1:4:aa"));
    assert!(
        mock.connections() > before,
        "a new connection (the pool was dropped)"
    );
    rig.core.set_network(NetworkFacts {
        available: false,
        id: String::new(),
        transport: String::new(),
    });
    rig.connection("no network", |v| v.state == ConnectionState::NoNetwork)
        .await;
    drop(feed);
}

/// The dead-link verdict counts sleep (§12.2): after two keep-alives of a boot clock that jumped
/// across a sleep, a wake finds the link dead and reconnects; the deadline is published as WakeAt.
#[tokio::test]
async fn a_silent_link_is_dead_after_a_sleep() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    let now = rig.clock_boot();
    let wake = rig
        .next(
            "WakeAt",
            |u| matches!(u, Update::WakeAt { boot_ms: Some(t) } if *t >= now + 200_000),
        )
        .await;
    let Update::WakeAt { boot_ms: Some(t) } = wake else {
        unreachable!()
    };
    assert!(t <= now + 241_000, "two keep-alives of 120 s: {}", t - now);
    rig.clock.sleep(241_000);
    rig.core.wake();
    let v = rig
        .connection("dead", |v| v.state == ConnectionState::Unreachable)
        .await;
    assert!(v.words.contains("the link is dead"), "{}", v.words);
    rig.clock.sleep(2_000);
    rig.core.wake();
    let feed2 = mock.feed().await;
    assert_eq!(feed2.req.header("last-event-id"), Some("e1:4:aa"));
    drop(feed);
}

impl Rig {
    fn clock_boot(&self) -> u64 {
        use kai_mobile_core::clock::Clock;
        self.clock.boot_ms()
    }
}

/// The backoff doubles from 1 s to `lmgw.retry_max_s` while visible and to
/// `lmgw.retry_max_background_s` while not (§12.2). Each delay is read off a fresh view (its next
/// try still ahead) and skipped by jumping the boot clock, as a sleep would.
#[tokio::test]
async fn the_backoff_has_two_ceilings() {
    use kai_mobile_core::clock::Clock;
    let url = closed_url().await;
    let mut rig = Rig::paired(&PKI, &[&url]).await;
    async fn delays(rig: &mut Rig, n: usize) -> Vec<i64> {
        let mut out = Vec::new();
        for _ in 0..n {
            let clock = rig.clock.clone();
            let v = rig
                .connection("a fresh retry", |v| {
                    v.state == ConnectionState::NotRunning
                        && v.next_try_at_ms.is_some_and(|t| t > clock.wall_ms())
                })
                .await;
            let d = v.next_try_at_ms.unwrap() - rig.clock.wall_ms();
            out.push((d + 500) / 1000);
            rig.clock.sleep(u64::try_from(d).unwrap() + 10);
            rig.core.wake();
        }
        out
    }
    assert_eq!(
        delays(&mut rig, 5).await,
        [1, 2, 4, 8, 8],
        "background: up to retry_max_background_s"
    );
    // Visible: at once, the backoff reset, then up to retry_max_s.
    rig.core.set_foreground(true);
    assert_eq!(
        delays(&mut rig, 4).await,
        [1, 2, 4, 4],
        "visible: up to retry_max_s"
    );
    let v = rig
        .connection("words", |v| v.state == ConnectionState::NotRunning)
        .await;
    assert!(
        v.words.contains("127.0.0.1") && v.words.contains("is not running on"),
        "{}",
        v.words
    );
}

/// A 401 by code over the authenticated link stops the retries until Try again (§5.5); a
/// disabled device keeps its key.
#[tokio::test]
async fn a_refused_key_stops_the_retries_until_try_again() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.state.lock().unwrap().feed_answers.push_back((
        401,
        r#"{"code":"device_disabled","message":"device 'phone' is disabled"}"#.into(),
    ));
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let v = rig
        .connection("disabled", |v| v.state == ConnectionState::KeyRefused)
        .await;
    assert_eq!(v.refusal, Some(KeyRefusal::Disabled));
    assert_eq!(
        v.actions,
        [ConnectionAction::TryAgain, ConnectionAction::PairAgain]
    );
    assert_eq!(v.next_try_at_ms, None);
    rig.clock.sleep(600_000);
    rig.core.wake();
    assert!(
        mock.feed_within(Duration::from_millis(500)).await.is_none(),
        "stopped"
    );
    assert_eq!(rig.keystore.aliases().len(), 1, "the key stays");
    rig.core.retry();
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
}

/// The feed's `revoked` by kind (§5.5): `certificate_expired` is the lapse; a kind this build does
/// not know is said and tried again on the backoff.
#[tokio::test]
async fn revocation_kinds() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    feed.send(&super::rig::live("revoked", json!({"reason": "key_paused", "message": "device 'phone' was paused", "kind": "key_paused"})));
    let v = rig
        .connection("unknown kind", |v| v.state == ConnectionState::KeyRefused)
        .await;
    assert_eq!(v.refusal, Some(KeyRefusal::Unknown));
    assert!(v.next_try_at_ms.is_some(), "tried again on the backoff");
    rig.clock.sleep(2_000);
    rig.core.wake();
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    feed.send(&super::rig::live("revoked", json!({"reason": "certificate_expired", "message": "the certificate of device 'phone' expired — pair it again", "kind": "certificate_expired"})));
    let v = rig
        .connection("lapsed", |v| v.state == ConnectionState::CertificateLapsed)
        .await;
    assert_eq!(v.next_try_at_ms, None);
}

/// A handshake's alert is never a refusal (Kai desktop's review B1): a squatter's
/// `certificate_expired` or `bad_certificate` is said and tried again; the pairing stays.
#[tokio::test]
async fn an_alert_from_whoever_answers_is_said_and_tried_again() {
    for alert in [CERTIFICATE_EXPIRED, BAD_CERTIFICATE] {
        let squatter = Squatter::bind().await;
        let url = squatter.url.clone();
        let _s = squatter.serve(alert);
        let mut rig = Rig::paired(&PKI, &[&url]).await;
        let v = rig
            .connection("an alert", |v| {
                v.state == ConnectionState::Unreachable && v.next_try_at_ms.is_some()
            })
            .await;
        assert!(
            v.words.contains("before it proved to be your lmgw"),
            "{}",
            v.words
        );
        assert!(rig.link_file().is_some());
        assert_eq!(rig.keystore.aliases().len(), 1);
    }
}

/// The steady-state verifier: a server under another CA is "not this lmgw" (tried again: a
/// stranger on a foreign network proves nothing about the owner's lmgw); a host the leaf does not
/// name is said with the host.
#[tokio::test]
async fn the_pinned_verifier_refuses_another_ca_and_an_unnamed_host() {
    let foreign = Mock::start(&FOREIGN, Clients::None).await;
    let mut rig = Rig::paired(&PKI, &[&foreign.url]).await;
    let v = rig
        .connection("not this lmgw", |v| v.state == ConnectionState::NotThisLmgw)
        .await;
    assert!(v.next_try_at_ms.is_some());
    assert!(foreign.requests("/").is_empty(), "nothing was sent to it");
    drop(rig);

    let unnamed = Pki::naming("lmgw unnamed", &["lmgw.example"], (2020, 2100));
    let mock = Mock::start(&unnamed, Clients::Required).await;
    let mut rig = Rig::paired(&unnamed, &[&mock.url]).await;
    let v = rig
        .connection("host not named", |v| {
            v.state == ConnectionState::HostNotNamed
        })
        .await;
    assert!(v.words.contains("127.0.0.1"), "{}", v.words);
}

/// A key the Keystore can no longer sign with: pair again, no retries.
#[tokio::test]
async fn a_key_gone_from_the_keystore_asks_to_pair_again() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::with(
        |ks, _| {
            ks.fail_signing.store(true, Ordering::SeqCst);
            Some(super::rig::paired_file(
                &PKI,
                ks,
                &[&mock.url],
                (2020, 2100),
            ))
        },
        super::rig::fast_settings(),
    )
    .await;
    let v = rig
        .connection("key missing", |v| v.state == ConnectionState::KeyMissing)
        .await;
    assert_eq!(v.actions, [ConnectionAction::PairAgain]);
    assert_eq!(v.next_try_at_ms, None);
}

/// A Keystore failure that is not a lost key (busy, a binder error) is said once in a notice and
/// tried again on the backoff; the pairing and its key stay.
#[tokio::test]
async fn a_keystore_failure_short_of_a_lost_key_is_tried_again() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::with(
        |ks, _| {
            ks.fail_signing_transient.store(true, Ordering::SeqCst);
            Some(super::rig::paired_file(
                &PKI,
                ks,
                &[&mock.url],
                (2020, 2100),
            ))
        },
        super::rig::fast_settings(),
    )
    .await;
    let v = rig
        .connection("tried again", |v| {
            v.state == ConnectionState::Unreachable && v.next_try_at_ms.is_some()
        })
        .await;
    assert!(v.words.contains("could not sign"), "{}", v.words);
    let noticed = rig
        .seen
        .iter()
        .filter(|u| matches!(u, Update::Notice { text } if text.contains("could not sign")))
        .count();
    assert_eq!(noticed, 1);
    rig.keystore
        .fail_signing_transient
        .store(false, Ordering::SeqCst);
    rig.clock.sleep(2_000);
    rig.core.wake();
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    assert!(rig.link_file().is_some());
}

/// The feed's first-bytes wait: lmgw answered but sent nothing within `lmgw.timeout_s` (on the
/// boot clock, which a wake compares).
#[tokio::test]
async fn an_answer_without_bytes_runs_out_of_its_wait() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    rig.clock.sleep(2_100);
    rig.core.wake();
    let v = rig
        .connection("no bytes", |v| v.state == ConnectionState::Unreachable)
        .await;
    assert!(
        v.words.contains("sent no hello, nor any byte"),
        "{}",
        v.words
    );
    drop(feed);
}

/// The whole race is bounded by `lmgw.timeout_s`: two URLs that never answer TLS are "not
/// answering".
#[tokio::test]
async fn the_whole_race_is_bounded() {
    let mut holes = Vec::new();
    let mut urls = Vec::new();
    for _ in 0..2 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        urls.push(format!("https://{}", l.local_addr().unwrap()));
        holes.push(tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((s, _)) = l.accept().await {
                held.push(s);
            }
        }));
    }
    let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let mut rig = Rig::paired(&PKI, &refs).await;
    let started = std::time::Instant::now();
    let v = rig
        .connection("not answering", |v| {
            v.state == ConnectionState::NotAnswering
        })
        .await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(v.words.contains("2 s (lmgw.timeout_s)"), "{}", v.words);
    assert_eq!(v.detail.len(), 2);
}

/// A server leaf outside its dates is said as the phone's clock (and tried again).
#[tokio::test]
async fn a_lapsed_server_leaf_is_said() {
    let lapsed = Pki::naming("lmgw lapsed leaf", &["127.0.0.1"], (2020, 2021));
    let mock = Mock::start(&lapsed, Clients::Required).await;
    let mut rig = Rig::paired(&lapsed, &[&mock.url]).await;
    let v = rig
        .connection("dates", |v| v.state == ConnectionState::Unreachable)
        .await;
    assert!(
        v.words.contains("not valid at this device's time"),
        "{}",
        v.words
    );
    assert!(v.next_try_at_ms.is_some());
}

/// Each 401 by code (§5.5): what it asks, whether the retries stop; a code this build does not
/// know is lmgw's sentence in a notice and the backoff, never "pair again".
#[tokio::test]
async fn each_401_by_its_code() {
    for (code, state, refusal, stops) in [
        (
            "device_key_unknown",
            ConnectionState::KeyRefused,
            Some(KeyRefusal::PairAgain),
            true,
        ),
        (
            "key_expired",
            ConnectionState::KeyRefused,
            Some(KeyRefusal::Expired),
            true,
        ),
        (
            "device_certificate_expired",
            ConnectionState::CertificateLapsed,
            None,
            true,
        ),
        (
            "client_certificate_required",
            ConnectionState::KeyRefused,
            Some(KeyRefusal::Bug),
            false,
        ),
        (
            "device_paused",
            ConnectionState::KeyRefused,
            Some(KeyRefusal::Unknown),
            false,
        ),
    ] {
        let mock = Mock::start(&PKI, Clients::Required).await;
        mock.state.lock().unwrap().feed_answers.push_back((
            401,
            format!(r#"{{"code":"{code}","message":"lmgw says {code}"}}"#),
        ));
        let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
        let v = rig.connection(code, |v| v.state == state).await;
        assert_eq!(v.refusal, refusal, "{code}");
        assert_eq!(v.next_try_at_ms.is_none(), stops, "{code}");
        if code == "client_certificate_required" {
            assert!(
                v.words.contains("please report it with the log"),
                "{}",
                v.words
            );
        }
        if code == "device_paused" {
            // An unknown code: lmgw's sentence in a notice, the key kept, the backoff on.
            assert!(v.words.contains("lmgw says device_paused"), "{}", v.words);
            assert_eq!(v.actions, [ConnectionAction::TryAgain]);
            // The notice goes out before the state that says it.
            let noticed = rig.seen.iter().any(|u| {
                matches!(u, Update::Notice { text } if text.contains("lmgw says device_paused"))
            });
            assert!(noticed, "the notice");
            assert_eq!(rig.keystore.aliases().len(), 1, "the key stays");
            rig.clock.sleep(2_000);
            rig.core.wake();
            let feed = mock.feed().await;
            feed.send(&hello("e1:4:aa", 120, "[]"));
            rig.connection("connected again", |v| v.state == ConnectionState::Connected)
                .await;
        }
    }
}
