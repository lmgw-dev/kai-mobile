//! A thread's call and the move to another thread (design §7.12): the binding a call keeps, the
//! facts it follows, how it ends, and Talk elsewhere moving a running call at its next quiet point.

use super::*;

/// The opened thread the tests talk in.
const THREAD: i64 = 12;
const TITLE: &str = "Example thread";

fn in_thread(id: i64) -> CallStart {
    CallStart {
        thread: Some(id),
        ..CallStart::default()
    }
}

/// A call bound to [`THREAD`] by `chat_thread` (automatic) at t=0, after the cue.
fn thread_call() -> Call {
    let mut c = Call::new(
        1,
        in_thread(THREAD),
        facts(),
        None,
        Some((THREAD, TITLE.into())),
        false,
        0,
    );
    c.answered("rtc_1".into(), &answer(262_144), 0);
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.channel_message(
        ev(
            json!({"type": "session.created", "event_id": "e1", "session": {"type": "realtime",
            "lmgw": {"turn_mode": "automatic",
                "resolved": {"chat_thread": {"id": THREAD, "title": TITLE}}}}}),
        ),
        0,
    );
    c
}

/// Kai finishes response `id` and its audio plays out.
fn quiet(c: &mut Call, id: &str, now: u64) {
    c.channel_message(
        ev(json!({"type": "response.done", "response": {"id": id, "status": "completed"}})),
        now,
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.stopped", "response_id": id})),
        now,
    );
}

#[test]
fn a_thread_call_names_its_thread_and_never_follows_the_conversation() {
    let mut c = thread_call();
    let effects = c.drain();
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::FollowThread(_))),
        "the Kai conversation stays where it is: {effects:?}"
    );
    let v = c.view(0);
    assert_eq!(v.bind_thread, Some(THREAD));
    assert_eq!(v.thread_id, Some(THREAD));
    assert_eq!(v.thread_title.as_deref(), Some(TITLE));
    assert_eq!(v.switching, None);
    // The Kai conversation moving on (`folder.current`) is no rebind of this call.
    c.conversation(Some(9), vec![DESKTOP.into()], 100);
    c.tick(200);
    let effects = c.drain();
    assert!(ended(&effects).is_none(), "{effects:?}");
    assert_eq!(c.view(200).waiting_for, None, "another thread's turn");
}

#[test]
fn k16_comes_from_the_bound_threads_live_facts() {
    let mut c = thread_call();
    c.drain();
    c.live(7, vec![DESKTOP.into()], 10);
    assert!(c.drain().is_empty(), "another thread's");
    c.live(THREAD, vec![DESKTOP.into()], 20);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Silent)]);
    assert_eq!(c.view(20).waiting_for.as_deref(), Some(DESKTOP));
    c.live(THREAD, vec![], 30);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
}

#[test]
fn the_kai_call_takes_no_threads_live_facts() {
    let mut c = bound();
    c.live(7, vec![DESKTOP.into()], 10);
    assert!(c.drain().is_empty());
    assert_eq!(c.view(10).bind_thread, None);
}

#[test]
fn talk_elsewhere_moves_the_call_at_its_next_quiet_point() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.switch_to(in_thread(THREAD), 100);
    assert!(ended(&c.drain()).is_none(), "Kai still speaks");
    assert_eq!(c.view(100).switching, Some(in_thread(THREAD)));
    quiet(&mut c, "r1", 300);
    let effects = c.drain();
    let end = ended(&effects).expect("the call moves");
    assert_eq!(end.reason, EndReason::Rebind);
    assert_eq!(end.words, None);
    assert_eq!(end.restart, Some(in_thread(THREAD)));
    assert_eq!(end.thread, None, "it was the Kai conversation's call");
    // The stop goes, then the channel closes; the old id is hung up before the next POST.
    let r = stats_request(&effects).unwrap();
    c.stats(r, PlayoutStats::default(), 310);
    assert_eq!(types(&c.drain()), ["lmgw.response.stop"]);
    c.channel_closed("closed", 320);
    assert_eq!(c.stale_id(), Some("rtc_1"));
    assert_eq!(c.view(320).switching, None);
}

#[test]
fn a_thread_call_moves_back_to_the_kai_conversation_by_the_folder() {
    let mut c = thread_call();
    c.drain();
    c.switch_to(CallStart::default(), 10);
    let end = ended(&c.drain()).expect("quiet: the move goes at once");
    assert_eq!(end.restart, Some(CallStart::default()));
    assert_eq!(end.thread, Some(THREAD));
}

#[test]
fn a_later_talk_replaces_the_target_and_the_calls_own_binding_calls_it_off() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.switch_to(in_thread(THREAD), 10);
    c.switch_to(in_thread(13), 20);
    assert_eq!(c.view(20).switching, Some(in_thread(13)));
    // Stay: the call's own binding.
    c.switch_to(CallStart::default(), 30);
    assert_eq!(c.view(30).switching, None);
    quiet(&mut c, "r1", 40);
    assert!(ended(&c.drain()).is_none(), "the move was called off");
}

#[test]
fn a_pending_bind_is_left_at_once_for_another_thread() {
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_open(0);
    c.channel_message(pending(DESKTOP), 10);
    c.drain();
    c.switch_to(in_thread(THREAD), 20);
    let effects = c.drain();
    let end = ended(&effects).expect("nothing is bound: it goes now");
    assert_eq!(end.restart, Some(in_thread(THREAD)));
    assert!(sent(&effects).is_empty(), "no stop on a pending bind");
    assert!(effects.contains(&Effect::CloseChannel));
}

#[test]
fn a_move_asked_before_the_bind_waits_for_it() {
    let mut c = call();
    c.switch_to(in_thread(THREAD), 10);
    assert!(ended(&c.drain()).is_none());
    c.transport(TransportState::Connected, "connected", 20);
    c.channel_open(20);
    c.channel_message(created(7, "automatic"), 30);
    let end = ended(&c.drain()).expect("bound and quiet");
    assert_eq!(end.restart, Some(in_thread(THREAD)));
}

#[test]
fn end_during_the_wait_ends_the_call_and_starts_nothing() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.switch_to(in_thread(THREAD), 10);
    c.end(20);
    let end = ended(&c.drain()).unwrap();
    assert_eq!(end.reason, EndReason::Ended);
    assert_eq!(end.restart, None);
    assert_eq!(c.view(20).switching, None);
}

#[test]
fn a_thread_calls_ends_bind_the_thread_again_and_a_gone_thread_offers_no_reconnect() {
    let mut c = thread_call();
    c.drain();
    c.channel_message(
        ev(
            json!({"type": "lmgw.session.closed", "kind": "out_of_reach", "by": null,
            "message": "chat thread 12 is out of reach"}),
        ),
        10,
    );
    let end = ended(&c.drain()).unwrap();
    assert_eq!(end.reason, EndReason::OutOfReach);
    assert_eq!(
        end.words.as_deref(),
        Some(
            "This thread is gone: it was deleted, or is no longer within this device's reach on \
             lmgw"
        )
    );
    assert!(end.actions.is_empty());
    assert_eq!(end.thread, Some(THREAD));
    assert_eq!(
        end.message.as_deref(),
        Some("chat thread 12 is out of reach")
    );

    let mut c = thread_call();
    c.drain();
    c.channel_closed("closed", 10);
    let end = ended(&c.drain()).unwrap();
    assert_eq!(end.reason, EndReason::ConnectionLost);
    assert_eq!(end.actions, vec![VoiceAction::Reconnect]);
    assert_eq!(end.thread, Some(THREAD), "Reconnect binds the thread again");
}

#[test]
fn the_kai_calls_end_names_no_thread() {
    let mut c = bound();
    c.channel_closed("closed", 10);
    assert_eq!(ended(&c.drain()).unwrap().thread, None);
}

/// K16 for a turn already running in the bound thread when the call starts (review of
/// 2026-10-19): the link says it once, before `session.created`, and the call keeps it.
#[test]
fn a_turn_heard_before_the_bind_holds_the_microphone_once_bound() {
    let mut c = Call::new(
        1,
        in_thread(THREAD),
        facts(),
        None,
        Some((THREAD, TITLE.into())),
        false,
        0,
    );
    c.answered("rtc_1".into(), &answer(262_144), 0);
    c.live(THREAD, vec![DESKTOP.into()], 0);
    assert_eq!(c.view(0).waiting_for, None, "not live yet");
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.drain();
    c.channel_message(created(THREAD, "automatic"), 10);
    assert!(
        c.drain().contains(&Effect::Mic(MicState::Silent)),
        "K16 once bound"
    );
    assert_eq!(c.view(10).waiting_for.as_deref(), Some(DESKTOP));
}

#[test]
fn stay_calls_the_move_off_and_is_refused_once_the_move_went() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.switch_to(in_thread(THREAD), 10);
    assert_eq!(c.stay(20), Ok(()));
    assert_eq!(c.view(20).switching, None);
    assert_eq!(c.stay(25), Ok(()), "nothing left to call off");
    quiet(&mut c, "r1", 30);
    assert!(ended(&c.drain()).is_none(), "the move was called off");

    c.switch_to(in_thread(THREAD), 40);
    let end = ended(&c.drain()).expect("quiet: the move goes at once");
    assert_eq!(end.restart, Some(in_thread(THREAD)));
    let err = c.stay(50).unwrap_err();
    assert!(err.contains("too late"), "{err}");
    assert_eq!(c.end.as_ref().unwrap().restart, Some(in_thread(THREAD)));
}

#[test]
fn a_gone_target_calls_the_move_off_and_says_so() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.switch_to(in_thread(THREAD), 10);
    c.thread_gone(13, 20);
    assert_eq!(
        c.view(20).switching,
        Some(in_thread(THREAD)),
        "another thread"
    );
    c.thread_gone(THREAD, 30);
    assert_eq!(c.view(30).switching, None);
    assert!(
        c.take_captions()
            .iter()
            .any(|l| l.text.contains("thread to move to is gone"))
    );
    quiet(&mut c, "r1", 40);
    assert!(ended(&c.drain()).is_none(), "the call stays");
}

#[test]
fn a_refused_restart_stops_saying_a_call_follows() {
    let mut c = thread_call();
    c.drain();
    c.restart_refused("No network.".into());
    assert!(c.end.is_none(), "no restart owed: nothing changes");
    c.switch_to(CallStart::default(), 10);
    ended(&c.drain()).expect("quiet: the move goes at once");
    c.restart_refused("No network.".into());
    let end = c.view(20).end.unwrap();
    assert_eq!(end.reason, EndReason::Refused);
    assert_eq!(end.words.as_deref(), Some("No network."));
    assert_eq!(end.restart, None);
    assert_eq!(end.actions, vec![VoiceAction::Reconnect]);
    assert_eq!(
        end.thread, None,
        "Reconnect binds as the restart would have"
    );
    assert!(c.drain().is_empty(), "Kotlin heard the refusal itself");
}
