//! A thread's call over the device link (design §7.12): Talk in an opened thread POSTs
//! `chat_thread` and looks no folder up, the Kai conversation's own thread binds by the folder, a
//! temporary thread is refused at once, the feed's facts for the bound thread reach the call (the
//! expected prompt, K16, also for a turn running before the bind and over the Kai conversation's
//! rollover), Talk elsewhere moves a running call at its next quiet point, Stay calls that call's
//! move off, a deleted target calls it off too, and a thread lmgw does not know offers no Reconnect.

use std::time::Duration;

use kai_mobile_core::{
    CallStart, CoreError, TransportState,
    views::Update,
    voice::{CallPhase, EndReason},
};
use serde_json::json;

use super::{
    mock::{Mock, detail},
    pki::{Clients, PKI},
    rig::{live, reply, stored},
    voice::{Kotlin, OFFER, connected, posts},
};

/// The opened thread the tests talk in (the Kai conversation is folder 5, thread 7).
const THREAD: i64 = 12;

fn in_thread(id: i64) -> CallStart {
    CallStart {
        thread: Some(id),
        ..CallStart::default()
    }
}

fn created(thread: i64) -> String {
    json!({"type": "session.created", "session": {"type": "realtime",
        "lmgw": {"turn_mode": "automatic",
            "resolved": {"chat_thread": {"id": thread, "title": "Example thread"}}}}})
    .to_string()
}

#[tokio::test]
async fn talk_in_a_thread_binds_it_by_chat_thread_and_expects_its_holder() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(
        &mock,
        json!([{"thread_id": THREAD, "by": "device 'desktop'"}]),
    )
    .await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(in_thread(THREAD), sink).await.unwrap();
    assert_eq!(
        k.next().await,
        "mic Off",
        "the feed shows the thread's holder"
    );
    voice.offer(n, OFFER.into()).await.unwrap();
    let paths: Vec<String> = posts(&mock).into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/v1/realtime/calls?chat_thread=12&takeover=ask"]);
}

#[tokio::test]
async fn the_kai_conversations_own_thread_binds_by_the_folder_and_a_temporary_one_is_refused() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, _k) = Kotlin::new();
    let err = voice.start(in_thread(-2), sink).await.unwrap_err();
    assert!(
        matches!(&err, CoreError::NotReady { reason } if reason.contains("temporary")),
        "{err:?}"
    );
    let err = voice.switch_to(in_thread(-2)).await.unwrap_err();
    assert!(matches!(err, CoreError::NotReady { .. }), "{err:?}");
    assert!(posts(&mock).is_empty(), "nothing reached lmgw");
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(in_thread(7), sink).await.unwrap();
    assert_eq!(k.next().await, "mic Live");
    voice.offer(n, OFFER.into()).await.unwrap();
    let paths: Vec<String> = posts(&mock).into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/v1/realtime/calls?chat_folder=5&takeover=ask"]);
}

#[tokio::test]
async fn the_bound_threads_turns_hold_the_microphone_back() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(in_thread(THREAD), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    k.until("cue").await;
    voice.channel_open(n);
    voice.channel_message(n, created(THREAD));
    let u = rig
        .next(
            "live",
            |u| matches!(u, Update::Voice { view } if view.phase == CallPhase::Live),
        )
        .await;
    let Update::Voice { view } = u else {
        unreachable!()
    };
    assert_eq!(view.bind_thread, Some(THREAD));
    assert_eq!(view.thread_title.as_deref(), Some("Example thread"));
    // A turn in the Kai conversation's thread is another thread's.
    feed.send(&live(
        "turn.started",
        json!({"thread_id": 7, "by": "device 'desktop'", "voice": false}),
    ));
    feed.send(&live(
        "turn.started",
        json!({"thread_id": THREAD, "by": "device 'desktop'", "voice": false}),
    ));
    let u = rig
        .next(
            "waiting",
            |u| matches!(u, Update::Voice { view } if view.waiting_for.is_some()),
        )
        .await;
    let Update::Voice { view } = u else {
        unreachable!()
    };
    assert_eq!(view.waiting_for.as_deref(), Some("device 'desktop'"));
    assert_eq!(k.next().await, "mic Silent", "K16 in the bound thread");
}

#[tokio::test]
async fn talk_elsewhere_moves_the_running_call_and_without_one_says_so() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    assert!(
        !voice.switch_to(in_thread(THREAD)).await.unwrap(),
        "no call runs"
    );
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    k.until("cue").await;
    voice.channel_open(n);
    voice.channel_message(n, created(7));
    voice.channel_message(
        n,
        json!({"type": "response.created", "response": {"id": "r1"}}).to_string(),
    );
    voice.channel_message(
        n,
        json!({"type": "output_audio_buffer.started", "response_id": "r1"}).to_string(),
    );
    assert!(voice.switch_to(in_thread(THREAD)).await.unwrap());
    let u = rig
        .next(
            "moving",
            |u| matches!(u, Update::Voice { view } if view.switching.is_some()),
        )
        .await;
    let Update::Voice { view } = u else {
        unreachable!()
    };
    assert_eq!(view.switching, Some(in_thread(THREAD)));
    assert_eq!(view.phase, CallPhase::Live, "Kai still speaks");
    voice.channel_message(
        n,
        json!({"type": "response.done", "response": {"id": "r1", "status": "completed"}})
            .to_string(),
    );
    voice.channel_message(
        n,
        json!({"type": "output_audio_buffer.stopped", "response_id": "r1"}).to_string(),
    );
    k.until("ended").await;
    let end = k.ends.lock().unwrap()[0].clone();
    assert_eq!(end.reason, EndReason::Rebind);
    assert_eq!(end.restart, Some(in_thread(THREAD)));
    assert_eq!(end.thread, None);
}

/// During a thread's call the owner hears only that thread: a reply in the Kai conversation
/// notifies (the app is not visible), as it does with no call (§12.3, §7.12).
#[tokio::test]
async fn a_thread_call_keeps_the_kai_conversations_replies_notifying() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, _k) = Kotlin::new();
    voice.start(in_thread(THREAD), sink).await.unwrap();
    feed.send(&stored(
        "e1:5:bb",
        "message.added",
        reply(7, 30, "Example reply."),
    ));
    rig.next("the Kai thread's reply notifies", |u| {
        matches!(u, Update::Notify { .. })
    })
    .await;
    // The Kai call's rule stands: none notifies while it runs.
    voice.end();
    rig.next(
        "the call's end",
        |u| matches!(u, Update::Voice { view } if view.phase == CallPhase::Ended),
    )
    .await;
    let (sink, _k) = Kotlin::new();
    voice.start(CallStart::default(), sink).await.unwrap();
    feed.send(&stored(
        "e1:6:cc",
        "message.added",
        reply(7, 31, "Another example reply."),
    ));
    assert!(
        rig.within(Duration::from_millis(800), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none()
    );
}

const DESKTOP: &str = "device 'desktop'";

/// A thread's call bound to [`THREAD`] and live (automatic mode), Kotlin's lines up to the cue
/// read.
async fn live_thread_call(voice: &kai_mobile_core::voice::Voice, k: &mut Kotlin, n: u64) {
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    k.until("cue").await;
    voice.channel_open(n);
    voice.channel_message(n, created(THREAD));
}

/// K16 for a turn that already runs in the thread when the call starts: the link says it once,
/// before the bind, and the call keeps it for when it is live (review of 2026-10-19).
#[tokio::test]
async fn a_turn_running_before_the_bind_holds_the_microphone_once_bound() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    feed.send(&live(
        "turn.started",
        json!({"thread_id": THREAD, "by": DESKTOP, "voice": false}),
    ));
    // The link handled the turn once this reply's notification comes (the feed is in order).
    feed.send(&stored(
        "e1:5:bb",
        "message.added",
        reply(7, 30, "Example reply."),
    ));
    rig.next("in order", |u| matches!(u, Update::Notify { .. }))
        .await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(in_thread(THREAD), sink).await.unwrap();
    live_thread_call(&voice, &mut k, n).await;
    k.until("mic Silent").await;
}

/// The Kai conversation moving to another thread (`folder.current`) leaves the bound thread's
/// live facts alone: the microphone stays held for the desktop's turn there.
#[tokio::test]
async fn the_kai_conversation_moving_on_keeps_the_bound_threads_k16() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(in_thread(THREAD), sink).await.unwrap();
    live_thread_call(&voice, &mut k, n).await;
    feed.send(&live(
        "turn.started",
        json!({"thread_id": THREAD, "by": DESKTOP, "voice": false}),
    ));
    k.until("mic Silent").await;
    mock.state
        .lock()
        .unwrap()
        .details
        .insert(8, detail(8, "Kai 2", vec![]));
    feed.send(&stored(
        "e1:6:cc",
        "folder.current",
        json!({"folder_id": 5, "thread_id": 8, "previous_thread_id": 7, "reason": "idle", "by": null}),
    ));
    rig.next(
        "moved",
        |u| matches!(u, Update::Messages { delta } if delta.reset && delta.thread_id == Some(8)),
    )
    .await;
    let lines = k.within(Duration::from_millis(500)).await;
    assert!(!lines.iter().any(|l| l == "mic Live"), "{lines:?}");
}

/// Stay names its call and needs nothing of the link: the move is called off, and a Stay for
/// another call is refused with words instead of touching the current one.
#[tokio::test]
async fn stay_calls_the_move_off_and_never_touches_another_call() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    k.until("cue").await;
    voice.channel_open(n);
    voice.channel_message(n, created(7));
    voice.channel_message(
        n,
        json!({"type": "response.created", "response": {"id": "r1"}}).to_string(),
    );
    assert!(voice.switch_to(in_thread(THREAD)).await.unwrap());
    rig.next(
        "moving",
        |u| matches!(u, Update::Voice { view } if view.switching.is_some()),
    )
    .await;
    let err = voice.stay(n + 1).await.unwrap_err();
    assert!(
        matches!(&err, CoreError::NotReady { reason } if reason.contains("too late")),
        "{err:?}"
    );
    voice.stay(n).await.unwrap();
    rig.next(
        "stays",
        |u| matches!(u, Update::Voice { view } if view.switching.is_none() && view.phase == CallPhase::Live),
    )
    .await;
}

/// The thread a move waits for is deleted: the move is called off and the call stays.
#[tokio::test]
async fn a_deleted_target_calls_the_move_off() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    k.until("cue").await;
    voice.channel_open(n);
    voice.channel_message(n, created(7));
    voice.channel_message(
        n,
        json!({"type": "response.created", "response": {"id": "r1"}}).to_string(),
    );
    assert!(voice.switch_to(in_thread(THREAD)).await.unwrap());
    rig.next(
        "moving",
        |u| matches!(u, Update::Voice { view } if view.switching.is_some()),
    )
    .await;
    feed.send(&stored(
        "e1:6:cc",
        "thread.deleted",
        json!({"thread_id": THREAD, "deleted": true, "by": null}),
    ));
    rig.next(
        "called off",
        |u| matches!(u, Update::Voice { view } if view.switching.is_none() && view.phase == CallPhase::Live),
    )
    .await;
}

/// 404 `chat_thread_not_found` on a thread's call is the thread gone: lmgw's words under the
/// gone thread's, and no Reconnect, which would bind it again.
#[tokio::test]
async fn a_thread_lmgw_does_not_know_ends_the_call_without_reconnect() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    mock.state.lock().unwrap().call_answers.push_back((
        404,
        json!({"error": {"type": "invalid_request_error", "code": "chat_thread_not_found",
            "message": "chat thread 12 not found"}})
        .to_string(),
    ));
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(in_thread(THREAD), sink).await.unwrap();
    let err = voice.offer(n, OFFER.into()).await.unwrap_err();
    assert!(
        matches!(&err, CoreError::Refused { code, .. } if code == "chat_thread_not_found"),
        "{err:?}"
    );
    k.until("ended").await;
    let end = k.ends.lock().unwrap()[0].clone();
    assert_eq!(end.reason, EndReason::Refused);
    assert!(end.words.unwrap().starts_with("This thread is gone"));
    assert_eq!(
        end.message.as_deref(),
        Some("lmgw refused the call: chat thread 12 not found (chat_thread_not_found)")
    );
    assert!(end.actions.is_empty(), "no Reconnect");
    assert_eq!(end.thread, Some(THREAD));
}
