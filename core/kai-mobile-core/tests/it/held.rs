//! The held feed's approvals and notifications (design §5.4, §11, §12.3, §15.3): the open set
//! reconciled at every connection through `FeedReader::knowing` with the notified and the dismissed
//! ids, `state`'s set before and after `caught_up` and `null`, the replies collapsed per thread at
//! `caught_up`, nothing while the app is visible, stable ids across a restart, a decision from a
//! notification.

use std::time::Duration;

use kai_mobile_core::{
    store::{self, ApprovalsFile},
    views::{ConnectionState, Decided, NotificationId, NotificationKind, Update},
};
use serde_json::json;

use super::{
    mock::{Feed, Mock, message},
    pki::{Clients, PKI},
    rig::{Rig, caught_up, decided, hello, live, reply, requested, stored},
};

fn approval(id: &str) -> NotificationId {
    NotificationId {
        kind: NotificationKind::Approval,
        key: id.into(),
    }
}

fn is_notify(u: &Update, id: &NotificationId) -> bool {
    matches!(u, Update::Notify { notification } if &notification.id == id)
}

fn is_withdraw(u: &Update, id: &NotificationId) -> bool {
    matches!(u, Update::Withdraw { id: w } if w == id)
}

async fn connected(rig: &mut Rig, mock: &Mock, open: &str) -> Feed {
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, open));
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    feed
}

fn open_set(ids: &[(&str, i64, i64)]) -> String {
    serde_json::to_string(
        &ids.iter()
            .map(|(id, t, m)| requested(id, *t, *m))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

/// `hello.open_approvals` with a catch-up that holds a request and its decision: nothing is raised
/// for the decided call, nothing before `caught_up`, and the open one at it.
#[tokio::test]
async fn the_open_set_is_raised_at_caught_up_not_replayed() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = connected(&mut rig, &mock, &open_set(&[("a", 7, 3)])).await;
    feed.send(&stored(
        "e1:5:b1",
        "approval.requested",
        requested("b", 7, 2),
    ));
    feed.send(&stored("e1:6:b2", "approval.decided", decided("b", 7, 2)));
    assert!(
        rig.within(Duration::from_millis(300), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none(),
        "nothing before caught_up"
    );
    feed.send(&caught_up("e1:6:b2"));
    let n = rig
        .next("a raised", |u| matches!(u, Update::Notify { .. }))
        .await;
    assert!(is_notify(&n, &approval("a")), "{n:?}");
    let Update::Notify { notification } = n else {
        unreachable!()
    };
    assert_eq!(notification.title, "Kai wants to run desktop__open_file");
    assert!(notification.text.contains("path: /tmp/x"));
    assert!(
        rig.within(Duration::from_millis(300), |u| is_notify(u, &approval("b")))
            .await
            .is_none()
    );
}

/// A reconnect after a decision made while the link was down withdraws the notification; one
/// after a dismissal raises nothing again; a restart with the shell's active notifications
/// raises nothing twice.
#[tokio::test]
async fn reconnects_reconcile_with_the_notified_and_the_dismissed_ids() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = connected(&mut rig, &mock, &open_set(&[("a", 7, 3), ("c", 8, 1)])).await;
    feed.send(&caught_up("e1:4:aa"));
    rig.next("a", |u| is_notify(u, &approval("a"))).await;
    rig.next("c", |u| is_notify(u, &approval("c"))).await;
    rig.core.dismiss_approval("c".into()).await.unwrap();
    // Acknowledged: approvals.json holds the dismissal once the call answered.
    let f: ApprovalsFile = store::read(&rig.dir.path().join("approvals.json"))
        .unwrap()
        .unwrap();
    assert_eq!(f.dismissed, ["c"]);
    drop(feed);
    // a decided while down; c still open (dismissed).
    rig.clock.sleep(2_000);
    rig.core.wake();
    let feed = connected(&mut rig, &mock, &open_set(&[("c", 8, 1)])).await;
    feed.send(&caught_up("e1:4:aa"));
    rig.next("a withdrawn", |u| is_withdraw(u, &approval("a")))
        .await;
    assert!(
        rig.within(Duration::from_millis(300), |u| is_notify(u, &approval("c")))
            .await
            .is_none(),
        "dismissed"
    );
    let f: ApprovalsFile = store::read(&rig.dir.path().join("approvals.json"))
        .unwrap()
        .unwrap();
    assert_eq!(f.dismissed, ["c"]);
    // c leaves the reach and comes back (a state's set): still dismissed.
    feed.send(&live("state", json!({"self_admin": "off", "hold": {"active": false}, "voice": [], "turns": [], "open_approvals": []})));
    feed.send(&live("state", json!({"self_admin": "off", "hold": {"active": false}, "voice": [], "turns": [], "open_approvals": [requested("c", 8, 1)]})));
    assert!(
        rig.within(Duration::from_millis(300), |u| is_notify(u, &approval("c")))
            .await
            .is_none()
    );
    // A new call after caught_up is raised; a restart that hands over its active notification
    // raises it no second time.
    feed.send(&stored(
        "e1:7:dd",
        "approval.requested",
        requested("d", 9, 4),
    ));
    rig.next("d", |u| is_notify(u, &approval("d"))).await;
    drop(feed);
    let mut rig = rig.restart_with(kai_mobile_core::PlatformFacts {
        notified_approvals: vec!["d".into()],
        ..super::rig::facts()
    });
    let feed = connected(&mut rig, &mock, &open_set(&[("c", 8, 1), ("d", 9, 4)])).await;
    feed.send(&caught_up("e1:7:dd"));
    assert!(
        rig.within(Duration::from_millis(400), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none()
    );
    // caught_up with c gone cuts the dismissed file.
    drop(feed);
    rig.clock.sleep(2_000);
    rig.core.wake();
    let feed = connected(&mut rig, &mock, &open_set(&[("d", 9, 4)])).await;
    feed.send(&caught_up("e1:7:dd"));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let f: Option<ApprovalsFile> =
                store::read(&rig.dir.path().join("approvals.json")).unwrap();
            if f.is_some_and(|f| f.dismissed.is_empty()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("approvals.json cut to the open set");
}

/// A `state`'s set: silent before `caught_up`; after it `Added` and `Withdrawn` by id; `null` keeps
/// the set.
#[tokio::test]
async fn a_state_s_set_before_and_after_caught_up() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = connected(&mut rig, &mock, "[]").await;
    let state = |set: serde_json::Value| {
        live(
            "state",
            json!({"self_admin": "off", "hold": {"active": false}, "voice": [], "turns": [], "open_approvals": set}),
        )
    };
    feed.send(&state(json!([requested("a", 7, 3)])));
    assert!(
        rig.within(Duration::from_millis(300), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none()
    );
    feed.send(&caught_up("e1:4:aa"));
    rig.next("a at caught_up", |u| is_notify(u, &approval("a")))
        .await;
    feed.send(&state(serde_json::Value::Null));
    feed.send(&state(json!([requested("b", 8, 1)])));
    rig.next("a withdrawn", |u| is_withdraw(u, &approval("a")))
        .await;
    rig.next("b added", |u| is_notify(u, &approval("b"))).await;
}

/// Nothing is notified while the app is visible; leaving the screen raises what is open.
#[tokio::test]
async fn nothing_while_visible_and_the_open_calls_when_it_leaves() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    rig.core.set_foreground(true);
    let feed = connected(&mut rig, &mock, &open_set(&[("a", 7, 3)])).await;
    feed.send(&caught_up("e1:4:aa"));
    let cards = rig
        .next(
            "the card",
            |u| matches!(u, Update::Approvals { open } if !open.is_empty()),
        )
        .await;
    let Update::Approvals { open } = cards else {
        unreachable!()
    };
    assert_eq!(open[0].arguments, "path: /tmp/x");
    assert!(
        rig.within(Duration::from_millis(300), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none()
    );
    rig.core.set_foreground(false);
    rig.next("raised on leaving", |u| is_notify(u, &approval("a")))
        .await;
}

/// Replies (§12.3): collapsed per thread at `caught_up` with the count, a live one notified, its
/// update replacing the text, its deletion withdrawing it, none while visible, none for a thread
/// outside the Kai folder.
#[tokio::test]
async fn replies_notify_per_thread() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![message(1, 7, "user", "hi")]);
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = connected(&mut rig, &mock, "[]").await;
    feed.send(&stored("e1:5:a", "message.added", reply(7, 2, "one")));
    feed.send(&stored("e1:6:b", "message.added", reply(7, 3, "two")));
    feed.send(&stored(
        "e1:7:c",
        "message.added",
        reply(99, 4, "elsewhere"),
    ));
    feed.send(&caught_up("e1:7:c"));
    let thread = NotificationId {
        kind: NotificationKind::Reply,
        key: "7".into(),
    };
    let n = rig
        .next("collapsed", |u| matches!(u, Update::Notify { .. }))
        .await;
    let Update::Notify { notification } = n else {
        unreachable!()
    };
    assert_eq!(notification.id, thread);
    assert_eq!((notification.text.as_str(), notification.count), ("two", 2));
    assert!(
        rig.within(
            Duration::from_millis(300),
            |u| matches!(u, Update::Notify { notification } if notification.id.key == "99")
        )
        .await
        .is_none()
    );
    feed.send(&stored("e1:8:d", "message.added", reply(7, 5, "three")));
    let n = rig.next("live", |u| is_notify(u, &thread)).await;
    let Update::Notify { notification } = n else {
        unreachable!()
    };
    assert_eq!(
        (notification.text.as_str(), notification.count),
        ("three", 3)
    );
    feed.send(&stored(
        "e1:9:e",
        "message.updated",
        reply(7, 5, "three, edited"),
    ));
    let n = rig.next("updated", |u| is_notify(u, &thread)).await;
    let Update::Notify { notification } = n else {
        unreachable!()
    };
    assert_eq!(notification.text, "three, edited");
    feed.send(&stored(
        "e1:10:f",
        "message.deleted",
        json!({"thread_id": 7, "message_ids": [5], "by": null}),
    ));
    rig.next("withdrawn", |u| is_withdraw(u, &thread)).await;
    rig.core.set_foreground(true);
    feed.send(&stored("e1:11:g", "message.added", reply(7, 6, "seen")));
    assert!(
        rig.within(Duration::from_millis(400), |u| is_notify(u, &thread))
            .await
            .is_none(),
        "visible"
    );
}

/// A folder lookup that fails does not silence the replies (§12.3): they keep collecting, the
/// lookup is asked again on its backoff, and once the Kai folder's threads are known the reply of
/// a thread in it that is not the current one notifies. Both shapes: the whole lookup fails, and
/// only its thread list does.
#[tokio::test]
async fn a_failed_folder_lookup_is_asked_again_and_the_replies_wait_for_it() {
    for (folders, threads) in [(1, 0), (0, 1)] {
        let mock = Mock::start(&PKI, Clients::Required).await;
        mock.with_conversation(5, 7, vec![message(1, 7, "user", "hi")]);
        {
            let mut s = mock.state.lock().unwrap();
            // Thread 8 is in the Kai folder too, not its current thread: only the list says so.
            s.threads.push(kai_mobile_core::lmgw::requests::ThreadRow {
                id: 8,
                title: "Kai, earlier".into(),
                folder_id: Some(5),
                ..Default::default()
            });
            s.fail_folders = folders;
            s.fail_threads = threads;
        }
        let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
        let feed = connected(&mut rig, &mock, "[]").await;
        feed.send(&stored("e1:5:a", "message.added", reply(8, 2, "one")));
        feed.send(&caught_up("e1:5:a"));
        let thread = NotificationId {
            kind: NotificationKind::Reply,
            key: "8".into(),
        };
        assert!(
            rig.within(Duration::from_millis(600), |u| is_notify(u, &thread))
                .await
                .is_none(),
            "the reply waits for the Kai folder's threads ({folders}, {threads})"
        );
        // The lookup's backoff starts at 1 s (on the boot clock, which a wake compares).
        rig.clock.sleep(1_100);
        rig.core.wake();
        let n = rig
            .next("the reply once the threads are known", |u| {
                is_notify(u, &thread)
            })
            .await;
        let Update::Notify { notification } = n else {
            unreachable!()
        };
        assert_eq!(notification.text, "one");
        assert!(mock.requests("/chat/api/folders").len() >= 2);
        drop(feed);
    }
}

/// A decision from a notification: with one call on the reply it is sent at once, its
/// notification withdrawn, the resumed turn read; with two, it waits for the other.
#[tokio::test]
async fn a_decision_from_a_notification() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = connected(
        &mut rig,
        &mock,
        &open_set(&[("a", 7, 3), ("b", 8, 1), ("c", 8, 1)]),
    )
    .await;
    feed.send(&caught_up("e1:4:aa"));
    rig.next("a", |u| is_notify(u, &approval("a"))).await;
    assert_eq!(
        rig.core.decide("a".into(), true).await.unwrap(),
        Decided::Sent
    );
    rig.next("a withdrawn", |u| is_withdraw(u, &approval("a")))
        .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.requests("/chat/api/threads/7/approvals").is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the verdict was posted");
    let body: serde_json::Value =
        serde_json::from_slice(&mock.requests("/chat/api/threads/7/approvals")[0].body).unwrap();
    assert_eq!(body["decisions"][0]["approval_request_id"], "a");
    assert_eq!(body["decisions"][0]["approve"], true);
    assert_eq!(
        rig.core.decide("b".into(), false).await.unwrap(),
        Decided::Waiting { remaining: 1 }
    );
    assert_eq!(
        rig.core.decide("c".into(), true).await.unwrap(),
        Decided::Sent
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.requests("/chat/api/threads/8/approvals").is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both verdicts went together");
    let body: serde_json::Value =
        serde_json::from_slice(&mock.requests("/chat/api/threads/8/approvals")[0].body).unwrap();
    assert_eq!(body["decisions"].as_array().unwrap().len(), 2);
}

/// A decision from a notification that started the process comes before the open set is known:
/// held until the first `caught_up`, then sent, never "decided elsewhere" by guess (§11).
#[tokio::test]
async fn a_decision_before_the_open_set_is_known_waits_for_caught_up() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let (decided, feed) = tokio::join!(rig.core.decide("a".into(), true), async {
        let feed = mock.feed().await;
        feed.send(&hello("e1:4:aa", 120, &open_set(&[("a", 7, 3)])));
        tokio::time::sleep(Duration::from_millis(200)).await;
        feed.send(&caught_up("e1:4:aa"));
        feed
    });
    assert_eq!(decided.unwrap(), Decided::Sent);
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.requests("/chat/api/threads/7/approvals").is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the verdict was posted");
    // Once the set is known, a call it lacks is decided elsewhere at once.
    let err = rig.core.decide("zz".into(), true).await.unwrap_err();
    assert!(err.to_string().contains("waits for no decision"), "{err}");
    rig.connection("connected", |v| v.state == ConnectionState::Connected)
        .await;
    drop(feed);
}

/// Without a connection the held decision is refused once `lmgw.timeout_s` passed (on the boot
/// clock), saying it was not sent.
#[tokio::test]
async fn a_held_decision_is_not_sent_when_lmgw_is_not_reached_in_time() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    // The feed is answered (and held) but lmgw never sends hello, so caught_up never comes.
    let rig = Rig::paired(&PKI, &[&mock.url]).await;
    let (decided, feed) = tokio::join!(rig.core.decide("a".into(), true), async {
        let feed = mock.feed().await;
        rig.clock.sleep(2_100);
        rig.core.wake();
        feed
    });
    let err = decided.unwrap_err();
    let kai_mobile_core::CoreError::Unreachable { reason, setting } = err else {
        panic!("{err:?}")
    };
    assert!(reason.contains("the decision was not sent"), "{reason}");
    assert_eq!(setting.as_deref(), Some("lmgw.timeout_s"));
    assert!(mock.requests("/chat/api/threads/7/approvals").is_empty());
    drop(feed);
}

/// A held verdict (one call of a reply decided from its notification, the other still waiting)
/// is not invisible: the waiting call is raised again naming it. It is never asked again: not when
/// the app leaves the screen, not at a reconnect's `caught_up`; the last verdict sends both.
#[tokio::test]
async fn a_held_verdict_is_shown_on_the_call_it_waits_for_and_never_asked_again() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let open = open_set(&[("b", 8, 1), ("c", 8, 1)]);
    let feed = connected(&mut rig, &mock, &open).await;
    feed.send(&caught_up("e1:4:aa"));
    rig.next("b", |u| is_notify(u, &approval("b"))).await;
    rig.next("c", |u| is_notify(u, &approval("c"))).await;
    assert_eq!(
        rig.core.decide("b".into(), true).await.unwrap(),
        Decided::Waiting { remaining: 1 }
    );
    rig.next("b withdrawn", |u| is_withdraw(u, &approval("b")))
        .await;
    let n = rig
        .next("c raised again", |u| is_notify(u, &approval("c")))
        .await;
    let Update::Notify { notification } = n else {
        unreachable!()
    };
    assert!(
        notification
            .text
            .starts_with("Your verdict on desktop__open_file waits for this one."),
        "{}",
        notification.text
    );
    // The app on screen and away again: b is not asked again.
    rig.core.set_foreground(true);
    rig.core.set_foreground(false);
    assert!(
        rig.within(Duration::from_millis(400), |u| is_notify(u, &approval("b")))
            .await
            .is_none(),
        "not asked again on leaving the screen"
    );
    // A reconnect: b comes back in the set, unknown to the new reader, and stays held.
    drop(feed);
    rig.clock.sleep(2_000);
    rig.core.wake();
    let feed = connected(&mut rig, &mock, &open).await;
    feed.send(&caught_up("e1:4:aa"));
    assert!(
        rig.within(Duration::from_millis(400), |u| is_notify(u, &approval("b")))
            .await
            .is_none(),
        "not asked again at caught_up"
    );
    assert_eq!(
        rig.core.decide("c".into(), false).await.unwrap(),
        Decided::Sent
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.requests("/chat/api/threads/8/approvals").is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both verdicts went together");
    let body: serde_json::Value =
        serde_json::from_slice(&mock.requests("/chat/api/threads/8/approvals")[0].body).unwrap();
    assert_eq!(body["decisions"].as_array().unwrap().len(), 2);
    drop(feed);
}

/// The assistant overlay's cards count as seen (§9.2, §11): while it shows them unlocked no
/// approval notification asks as well; when it hides, the calls still waiting are notified.
#[tokio::test]
async fn the_overlay_s_cards_are_seen_and_its_hide_notifies_what_waits() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = connected(&mut rig, &mock, "[]").await;
    feed.send(&caught_up("e1:4:aa"));
    rig.core.set_overlay(true);
    feed.send(&stored(
        "e1:5:bb",
        "approval.requested",
        requested("a", 7, 3),
    ));
    rig.next(
        "the card",
        |u| matches!(u, Update::Approvals { open } if open.len() == 1),
    )
    .await;
    assert!(
        rig.within(Duration::from_millis(300), |u| is_notify(u, &approval("a")))
            .await
            .is_none(),
        "the overlay shows it"
    );
    rig.core.set_overlay(false);
    rig.next("notified once the overlay hid", |u| {
        is_notify(u, &approval("a"))
    })
    .await;
}
