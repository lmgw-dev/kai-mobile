//! The call's rules (design §15.3, "the events channel's rules"), on explicit times.

use serde_json::{Value, json};

use super::*;
use crate::voice::{
    PlayoutStats,
    view::{EndReason, RingState, Speaker, VoiceAction},
};

// Approvals in the call and the late task result's continuation.
mod approvals;
// The phone's audio around the call: the focus, the headset, half duplex.
mod device;
// The assistant overlay's dismissal.
mod dismiss;

const DESKTOP: &str = "device 'desktop'";

fn facts() -> Facts {
    Facts {
        mode: VoiceMode::Thread,
        connect_timeout_s: 15,
        idle_end_s: 30,
        playout_report_ms: 1000,
        ptt_tail_ms: Some(200),
        half_duplex: false,
        echo_tail_ms: None,
        timeout_ms: 10_000,
    }
}

fn answer(max: u64) -> String {
    format!("v=0\r\no=- 1 1 IN IP4 0.0.0.0\r\ns=-\r\na=max-message-size:{max}\r\n")
}

/// A call answered by lmgw at t=0, its effects drained.
fn call_with(facts: Facts, start: CallStart, expect: Option<&str>) -> Call {
    let mut c = Call::new(
        1,
        start,
        facts,
        expect.map(str::to_owned),
        Some((7, "Kai".into())),
        (false, None),
        0,
    );
    c.answered("rtc_1".into(), &answer(262_144), 0);
    c.drain();
    c
}

fn call() -> Call {
    call_with(facts(), CallStart::default(), None)
}

fn created(thread: i64, mode: &str) -> String {
    json!({"type": "session.created", "event_id": "e1", "session": {"type": "realtime",
        "lmgw": {"turn_mode": mode, "resolved": {"chat_thread": {"id": thread, "title": "Kai"}}}}})
    .to_string()
}

fn ev(v: Value) -> String {
    v.to_string()
}

/// A bound call (automatic, thread 7) at t=0, after the cue.
fn bound() -> Call {
    let mut c = call();
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.channel_message(created(7, "automatic"), 0);
    c.drain();
    c
}

fn sent(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Send(t) => Some(serde_json::from_str(t).unwrap()),
            _ => None,
        })
        .collect()
}

fn types(effects: &[Effect]) -> Vec<String> {
    sent(effects)
        .iter()
        .map(|v| v["type"].as_str().unwrap().to_owned())
        .collect()
}

fn ended(effects: &[Effect]) -> Option<CallEnd> {
    effects.iter().find_map(|e| match e {
        Effect::Ended(end) => Some(end.clone()),
        _ => None,
    })
}

fn stats_request(effects: &[Effect]) -> Option<u64> {
    effects.iter().find_map(|e| match e {
        Effect::ReadStats(r) => Some(*r),
        _ => None,
    })
}

/// Kai speaks response `id`.
fn speaking(c: &mut Call, id: &str, now: u64) {
    c.channel_message(
        ev(json!({"type": "response.created", "response": {"id": id}})),
        now,
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.started", "response_id": id})),
        now,
    );
}

fn pending(held_by: &str) -> String {
    ev(json!({"type": "lmgw.session.bind_pending", "held_by": held_by}))
}

#[test]
fn the_cue_is_dtls_connected_and_session_created_binds() {
    let mut c = Call::new(
        1,
        CallStart::default(),
        facts(),
        None,
        Some((7, "Kai".into())),
        (false, None),
        0,
    );
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    assert_eq!(c.session_part(), None, "the thread's mode: no part");
    c.answered("rtc_1".into(), &answer(262_144), 10);
    c.transport(TransportState::Connected, "connected", 300);
    assert_eq!(c.drain(), vec![Effect::Cue]);
    assert_eq!(c.view(300).ring, RingState::Listening);
    assert_eq!(c.view(300).words, "Listening");
    c.channel_open(320);
    c.channel_message(created(7, "automatic"), 340);
    assert!(
        c.drain().is_empty(),
        "same thread, the microphone stays live"
    );
    let v = c.view(340);
    assert_eq!((v.phase, v.ring), (CallPhase::Live, RingState::Listening));
    assert!(!v.push_to_talk);
    // A folder that rolled over at the bind: the conversation follows.
    let mut c = call();
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_message(created(8, "automatic"), 0);
    assert!(c.drain().contains(&Effect::FollowThread(8)));
    assert_eq!(c.view(0).thread_id, Some(8));
}

#[test]
fn an_expected_pending_bind_keeps_the_microphone_off_and_sends_only_take_over() {
    let mut c = Call::new(
        1,
        CallStart::default(),
        facts(),
        Some(DESKTOP.into()),
        Some((7, "Kai".into())),
        (false, None),
        0,
    );
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Off)]);
    c.answered("rtc_1".into(), &answer(262_144), 0);
    c.transport(TransportState::Connected, "connected", 100);
    assert!(c.drain().is_empty(), "no cue at DTLS-connected");
    c.channel_message(pending(DESKTOP), 200);
    let v = c.view(200);
    assert_eq!(
        (v.phase, v.ring, v.held_by.as_deref()),
        (CallPhase::Pending, RingState::Pending, Some(DESKTOP))
    );
    assert_eq!(v.words, "Voice is on device 'desktop'");
    assert_eq!(v.pending_note, None, "no cue had played");
    // Nothing but take_over goes while pending.
    c.press(210);
    c.tap(220);
    c.set_push_to_talk(true, 230);
    c.set_muted(true, 240);
    c.set_muted(false, 250);
    assert!(sent(&c.drain()).is_empty());
    c.take_over(300);
    c.take_over(310);
    assert_eq!(types(&c.drain()), ["lmgw.session.take_over"]);
    assert!(c.view(300).taking_over);
    // Bound: the microphone back, the cue at its first samples, the held-back switch sent.
    c.channel_message(created(7, "automatic"), 400);
    let effects = c.drain();
    assert!(
        effects.contains(&Effect::Mic(MicState::Silent)),
        "{effects:?}"
    );
    assert_eq!(types(&effects), ["session.update"]);
    assert_eq!(
        sent(&effects)[0]["session"]["lmgw"]["turn_mode"],
        "push_to_talk"
    );
    assert!(!effects.contains(&Effect::Cue));
    c.capturing(450);
    assert_eq!(c.drain(), vec![Effect::Cue]);
    c.capturing(460);
    assert!(c.drain().is_empty(), "one cue");
}

#[test]
fn a_prompt_after_the_cue_says_what_was_not_heard_and_follows_a_new_holder() {
    let mut c = call();
    c.transport(TransportState::Connected, "connected", 0);
    c.drain();
    c.channel_message(pending(DESKTOP), 50);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Off)]);
    assert!(c.view(50).pending_note.unwrap().contains("not heard"));
    c.take_over(60);
    c.drain();
    c.channel_message(pending("the dashboard"), 70);
    let v = c.view(70);
    assert_eq!(v.held_by.as_deref(), Some("the dashboard"));
    assert!(!v.taking_over, "the prompt asks about the new holder");
    // The holder ends meanwhile: bound without a tap.
    c.channel_message(created(7, "automatic"), 80);
    assert_eq!(c.view(80).phase, CallPhase::Live);
    assert!(c.drain().contains(&Effect::Mic(MicState::Live)));
}

#[test]
fn an_ended_call_s_take_over_sends_it_at_the_prompt() {
    let start = CallStart {
        new: false,
        take_over: true,
    };
    let mut c = call_with(facts(), start, Some(DESKTOP));
    c.channel_message(pending(DESKTOP), 10);
    assert_eq!(types(&c.drain()), ["lmgw.session.take_over"]);
    // Bound directly: nothing is sent.
    let mut c = call_with(facts(), start, None);
    c.channel_message(created(7, "automatic"), 10);
    assert!(sent(&c.drain()).is_empty());
}

#[test]
fn a_decline_closes_the_channel_and_sends_nothing() {
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_open(0);
    c.channel_message(pending(DESKTOP), 10);
    c.drain();
    c.end(20);
    let effects = c.drain();
    assert!(sent(&effects).is_empty());
    assert!(effects.contains(&Effect::CloseChannel));
    assert_eq!(ended(&effects).unwrap().reason, EndReason::Declined);
    assert!(!effects.contains(&Effect::ClosePeer));
    c.channel_closed("closed", 30);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert!(c.done());
    assert_eq!(c.stale_id(), None, "lmgw ends a call whose channel closed");
}

#[test]
fn an_unanswered_prompt_ends_as_thread_bound_with_take_over() {
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_message(pending(DESKTOP), 10);
    // The phone's connect bound does not end a pending bind.
    c.tick(15_000);
    assert_eq!(c.view(15_000).phase, CallPhase::Pending);
    c.drain();
    c.channel_message(
        ev(
            json!({"type": "lmgw.session.closed", "kind": "thread_bound", "by": DESKTOP,
            "message": "the thread stayed bound"}),
        ),
        15_000,
    );
    let end = ended(&c.drain()).unwrap();
    assert_eq!(end.reason, EndReason::ThreadBound);
    assert_eq!(
        end.words.as_deref(),
        Some("Voice stayed on device 'desktop'")
    );
    assert_eq!(end.actions, vec![VoiceAction::TakeOver]);
    assert_eq!(end.message.as_deref(), Some("the thread stayed bound"));
    c.channel_closed("closed", 15_100);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), None);
}

#[test]
fn push_to_talk_clears_at_the_press_commits_at_the_release_and_keeps_its_tail() {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = Call::new(1, CallStart::default(), f, None, None, (false, None), 0);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Silent)]);
    assert_eq!(
        c.session_part().unwrap().to_value(),
        json!({"type": "realtime", "lmgw": {"turn_mode": "push_to_talk"}})
    );
    c.answered("rtc_1".into(), &answer(262_144), 0);
    c.channel_message(created(7, "automatic"), 0);
    // The part's answer: lmgw's word on the mode once it echoes it.
    c.channel_message(
        ev(json!({"type": "session.updated", "session": {"lmgw": {"turn_mode": "push_to_talk"}}})),
        0,
    );
    c.drain();
    assert!(c.view(0).push_to_talk);
    assert_eq!(c.view(0).ring, RingState::Idle);
    c.press(100);
    let effects = c.drain();
    assert_eq!(types(&effects), ["input_audio_buffer.clear"]);
    assert!(effects.contains(&Effect::Mic(MicState::Live)));
    assert_eq!(c.view(100).words, "Talking");
    c.release(1100);
    let effects = c.drain();
    assert_eq!(
        types(&effects),
        ["input_audio_buffer.commit", "response.create"]
    );
    assert!(
        !effects.contains(&Effect::Mic(MicState::Silent)),
        "the tail"
    );
    assert_eq!(c.deadline(1100), Some(1300));
    c.tick(1299);
    assert!(c.drain().is_empty());
    c.tick(1300);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Silent)]);
    assert_eq!(c.view(1300).note, None);
}

#[test]
fn without_a_tail_setting_the_release_silences_at_once_and_the_view_says_so() {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    f.ptt_tail_ms = None;
    let mut c = call_with(f, CallStart::default(), None);
    c.channel_message(created(7, "push_to_talk"), 0);
    c.press(10);
    c.release(500);
    assert!(c.drain().contains(&Effect::Mic(MicState::Silent)));
    assert!(c.view(500).note.unwrap().contains("voice.ptt_tail_ms"));
}

#[test]
fn a_press_before_the_session_is_committed_after_it() {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.press(10);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    c.release(400);
    assert!(
        sent(&c.drain()).is_empty(),
        "nothing before lmgw's first word"
    );
    c.channel_message(created(7, "automatic"), 450);
    assert_eq!(
        types(&c.drain()),
        ["input_audio_buffer.commit", "response.create"]
    );
}

#[test]
fn presses_are_ignored_in_automatic_mode_and_while_muted() {
    let mut c = bound();
    c.press(10);
    c.release(20);
    assert!(c.drain().is_empty());
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.channel_message(created(7, "push_to_talk"), 0);
    c.set_muted(true, 5);
    c.drain();
    c.press(10);
    assert!(c.drain().is_empty());
    assert_eq!(c.view(10).ring, RingState::Muted);
}

#[test]
fn mute_silences_the_track_and_a_tap_with_nothing_open_toggles_it() {
    let mut c = bound();
    c.set_muted(true, 10);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Silent)]);
    assert_eq!(c.view(10).words, "Muted");
    c.tap(20);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    assert!(!c.view(20).muted);
}

#[test]
fn k16_silences_automatic_mode_while_another_turn_runs_and_a_press_still_talks() {
    let mut c = bound();
    c.conversation(Some(7), vec![DESKTOP.into()], 10);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Silent)]);
    assert_eq!(c.view(10).waiting_for.as_deref(), Some(DESKTOP));
    assert_eq!(c.view(10).words, "Waiting for device 'desktop'");
    c.conversation(Some(7), vec![], 20);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    // In push-to-talk the press talks over it.
    c.set_push_to_talk(true, 30);
    c.conversation(Some(7), vec![DESKTOP.into()], 40);
    c.drain();
    c.press(50);
    assert!(c.drain().contains(&Effect::Mic(MicState::Live)));
}

#[test]
fn the_stop_mutes_at_once_and_sends_the_playout_time_read_after_the_mute() {
    let mut c = bound();
    speaking(&mut c, "r1", 100);
    assert!(c.view(100).stoppable);
    c.drain();
    c.tap(200);
    let effects = c.drain();
    assert_eq!(
        effects[0],
        Effect::Playback(false),
        "the mute before the read"
    );
    let r = stats_request(&effects).unwrap();
    assert!(sent(&effects).is_empty(), "the stop waits for its stats");
    assert!(c.view(200).kai_muted);
    c.stats(
        r,
        PlayoutStats {
            estimated_playout_timestamp: Some(3_970_000_000_123.0),
            ..Default::default()
        },
        210,
    );
    assert_eq!(
        sent(&c.drain()),
        [json!({"type": "lmgw.response.stop", "playout_ntp_ms": 3_970_000_000_123.0})]
    );
    // A tap that stopped Kai did not also mute.
    assert!(!c.view(210).muted);
    // lmgw's answer: cancelled, cleared; the mute stays until the next started.
    c.channel_message(
        ev(
            json!({"type": "response.done", "response": {"id": "r1", "status": "cancelled",
            "status_details": {"reason": "client_cancelled"}}}),
        ),
        250,
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.cleared", "response_id": "r1"})),
        260,
    );
    assert!(c.drain().is_empty());
    assert_ne!(
        c.view(260).ring,
        RingState::Interrupted,
        "a stop shows no interrupted"
    );
    speaking(&mut c, "r2", 900);
    assert!(c.drain().contains(&Effect::Playback(true)));
}

#[test]
fn a_stop_without_the_stat_leaves_the_field_out_and_one_whose_stats_never_come_goes_bare() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.stop_kai(10);
    let r = stats_request(&c.drain()).unwrap();
    c.stats(r, PlayoutStats::default(), 20);
    assert_eq!(sent(&c.drain()), [json!({"type": "lmgw.response.stop"})]);
    // The stats never come: lmgw.timeout_s later the stop goes without a position.
    speaking(&mut c, "r2", 100);
    c.drain();
    c.stop_kai(200);
    c.drain();
    c.tick(10_199);
    assert!(sent(&c.drain()).is_empty());
    c.tick(10_200);
    assert_eq!(types(&c.drain()), ["lmgw.response.stop"]);
}

#[test]
fn what_the_owner_does_after_a_stop_waits_behind_it() {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.channel_message(created(7, "push_to_talk"), 0);
    speaking(&mut c, "r1", 10);
    c.drain();
    // A press while Kai speaks stops Kai first.
    c.press(100);
    let effects = c.drain();
    let r = stats_request(&effects).unwrap();
    assert!(sent(&effects).is_empty());
    c.release(300);
    assert!(sent(&c.drain()).is_empty());
    c.stats(r, PlayoutStats::default(), 320);
    assert_eq!(
        types(&c.drain()),
        [
            "lmgw.response.stop",
            "input_audio_buffer.clear",
            "input_audio_buffer.commit",
            "response.create"
        ]
    );
}

#[test]
fn a_barge_in_s_cleared_mutes_and_speech_started_alone_changes_nothing() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.channel_message(
        ev(json!({"type": "input_audio_buffer.speech_started"})),
        100,
    );
    assert!(c.drain().is_empty(), "speech_started does not cut");
    c.channel_message(
        ev(
            json!({"type": "response.done", "response": {"id": "r1", "status": "cancelled",
            "status_details": {"reason": "turn_detected"}}}),
        ),
        110,
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.cleared", "response_id": "r1"})),
        120,
    );
    assert_eq!(c.drain(), vec![Effect::Playback(false)]);
    assert_eq!(c.view(120).ring, RingState::Interrupted);
    assert!(c.view(120).kai_muted);
    assert_eq!(c.view(800).ring, RingState::Listening);
    speaking(&mut c, "r2", 2000);
    assert_eq!(c.drain(), vec![Effect::Playback(true)]);
}

#[test]
fn playout_reports_go_at_the_setting_s_interval_while_kai_s_audio_plays() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    assert_eq!(c.deadline(0), Some(1000));
    c.tick(1000);
    let r = stats_request(&c.drain()).unwrap();
    c.stats(
        r,
        PlayoutStats {
            estimated_playout_timestamp: Some(5000.0),
            jitter_buffer_delay: Some(288.0),
            jitter_buffer_emitted_count: Some(4800),
        },
        1010,
    );
    assert_eq!(
        sent(&c.drain()),
        [
            json!({"type": "lmgw.output_audio.playout", "playout_ntp_ms": 5000.0,
            "jitter_buffer_ms": 60.0})
        ]
    );
    c.tick(2000);
    let r = stats_request(&c.drain()).unwrap();
    // Neither stat: no report.
    c.stats(r, PlayoutStats::default(), 2010);
    assert!(sent(&c.drain()).is_empty());
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.stopped", "response_id": "r1"})),
        2500,
    );
    c.tick(3000);
    assert_eq!(
        stats_request(&c.drain()),
        None,
        "played out: no more reports"
    );
}

#[test]
fn lmgw_s_parts_are_joined_and_a_broken_set_is_said_while_the_call_goes_on() {
    let mut c = call();
    let parts =
        lmgw_client::realtime::parts::split(&created(7, "automatic"), "evt_1", 120).unwrap();
    assert!(parts.len() > 2);
    for p in parts {
        c.channel_message(p, 10);
    }
    assert_eq!(c.view(10).phase, CallPhase::Live);
    let parts = lmgw_client::realtime::parts::split(&"x".repeat(400), "evt_2", 120).unwrap();
    c.channel_message(parts[0].clone(), 20);
    c.channel_message(ev(json!({"type": "input_audio_buffer.speech_started"})), 30);
    let v = c.view(30);
    assert!(v.error.unwrap().contains("evt_2"));
    assert_eq!(v.phase, CallPhase::Live);
    assert_eq!(v.ring, RingState::Listening);
    let notes: Vec<_> = c
        .take_captions()
        .into_iter()
        .filter(|l| l.speaker == Speaker::Note)
        .collect();
    assert_eq!(notes.len(), 1);
}

#[test]
fn the_phone_s_events_are_split_to_lmgw_s_message_size() {
    let mut c = Call::new(
        1,
        CallStart::default(),
        facts(),
        None,
        None,
        (false, None),
        0,
    );
    c.answered("rtc_1".into(), &answer(90), 0);
    c.channel_message(created(7, "automatic"), 0);
    c.drain();
    c.set_push_to_talk(true, 10);
    let msgs: Vec<String> = c
        .drain()
        .into_iter()
        .filter_map(|e| match e {
            Effect::Send(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(msgs.len() > 1, "{msgs:?}");
    assert!(msgs.iter().all(|m| m.len() <= 90));
    let mut j = lmgw_client::realtime::Joiner::new();
    let joined: Vec<String> = msgs.into_iter().filter_map(|m| j.push(m).event).collect();
    let v: Value = serde_json::from_str(&joined[0]).unwrap();
    assert_eq!(v["session"]["lmgw"]["turn_mode"], "push_to_talk");
}

#[test]
fn end_is_the_stop_then_the_channel_s_close_then_the_peer() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.end(100);
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!((end.reason, end.words.as_deref()), (EndReason::Ended, None));
    assert!(!effects.contains(&Effect::CloseChannel), "after the stop");
    let r = stats_request(&effects).unwrap();
    // The mute, the stats read and the microphone off before the end is said (Kotlin drops the
    // call's audio at the end).
    let at = |e: &Effect| effects.iter().position(|x| x == e).unwrap();
    let said = at(&Effect::Ended(end.clone()));
    assert!(at(&Effect::Playback(false)) < said, "{effects:?}");
    assert!(at(&Effect::ReadStats(r)) < said, "{effects:?}");
    assert!(at(&Effect::Mic(MicState::Off)) < said, "{effects:?}");
    c.stats(
        r,
        PlayoutStats {
            estimated_playout_timestamp: Some(77.0),
            ..Default::default()
        },
        110,
    );
    let effects = c.drain();
    assert_eq!(types(&effects), ["lmgw.response.stop"]);
    assert_eq!(effects.last(), Some(&Effect::CloseChannel));
    assert_eq!(c.view(110).phase, CallPhase::Ending);
    c.channel_closed("closed", 150);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert!(c.done());
    assert_eq!(c.stale_id(), None);
    assert_eq!(c.view(150).words, "Voice ended");
}

#[test]
fn a_channel_that_never_reports_closed_is_left_after_lmgw_timeout_s_and_its_id_hung_up() {
    let mut c = bound();
    c.end(100);
    let r = stats_request(&c.drain()).unwrap();
    // The close is asked at 110: its wait counts from there.
    c.stats(r, PlayoutStats::default(), 110);
    assert_eq!(c.drain().last(), Some(&Effect::CloseChannel));
    c.tick(10_109);
    assert!(c.drain().is_empty());
    c.tick(10_110);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert!(c.done());
    assert_eq!(
        c.stale_id(),
        Some("rtc_1"),
        "neither the channel's close nor lmgw's word: lmgw may hold the binding"
    );
}

#[test]
fn end_without_stats_sends_a_bare_stop_then_the_close() {
    let mut c = bound();
    c.end(100);
    let effects = c.drain();
    assert!(stats_request(&effects).is_some());
    assert!(!effects.contains(&Effect::CloseChannel));
    c.tick(10_099);
    assert!(c.drain().is_empty());
    // The stats never came: the stop goes bare, then the close; the peer waits for the channel.
    c.tick(10_100);
    let effects = c.drain();
    assert_eq!(sent(&effects), [json!({"type": "lmgw.response.stop"})]);
    assert_eq!(effects.last(), Some(&Effect::CloseChannel));
    assert!(!effects.contains(&Effect::ClosePeer));
    assert_eq!(c.deadline(10_100), Some(20_100));
    c.channel_closed("closed", 10_300);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), None);
}

#[test]
fn stats_arriving_late_still_get_the_stop_out_before_the_close() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.end(100);
    let r = stats_request(&c.drain()).unwrap();
    c.stats(
        r,
        PlayoutStats {
            estimated_playout_timestamp: Some(5.0),
            ..Default::default()
        },
        9_900,
    );
    let effects = c.drain();
    assert_eq!(types(&effects), ["lmgw.response.stop"]);
    assert_eq!(effects.last(), Some(&Effect::CloseChannel));
    // The hang-up's lmgw.timeout_s does not cut the close short.
    c.tick(10_100);
    assert!(c.drain().is_empty());
    c.tick(19_899);
    assert!(c.drain().is_empty());
    c.tick(19_900);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), Some("rtc_1"));
}

#[test]
fn a_forced_close_while_ending_leaves_a_stale_id_unless_lmgw_said_its_word() {
    // The connection fails while the stop waits for its stats.
    let mut c = bound();
    c.end(100);
    c.drain();
    c.transport(TransportState::Failed, "failed", 200);
    let effects = c.drain();
    assert!(effects.contains(&Effect::ClosePeer));
    assert!(sent(&effects).is_empty(), "nothing goes on a closed peer");
    assert_eq!(c.stale_id(), Some("rtc_1"));
    // A decline whose channel never reports closed.
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_open(0);
    c.channel_message(pending(DESKTOP), 10);
    c.end(20);
    c.drain();
    c.tick(10_020);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), Some("rtc_1"));
    // lmgw's word while the call ends: it let the call go, so nothing is hung up.
    let mut c = bound();
    c.end(100);
    let r = stats_request(&c.drain()).unwrap();
    c.stats(r, PlayoutStats::default(), 110);
    c.drain();
    c.channel_message(closed("closed", Value::Null, "the call ended"), 120);
    c.tick(10_110);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), None);
    // A rebind's id is hung up unless lmgw said its word.
    let mut c = bound();
    c.conversation(Some(9), vec![], 100);
    assert_eq!(ended(&c.drain()).unwrap().reason, EndReason::Rebind);
    c.channel_message(closed("closed", Value::Null, "the call ended"), 120);
    c.channel_closed("closed", 130);
    assert_eq!(c.stale_id(), None);
}

fn closed(kind: &str, by: Value, message: &str) -> String {
    ev(json!({"type": "lmgw.session.closed", "kind": kind, "by": by, "message": message}))
}

#[test]
fn every_closed_kind_has_its_words() {
    for (kind, by, message, reason, words, actions) in [
        (
            "taken_over",
            json!(DESKTOP),
            "voice mode moved to device 'desktop'",
            EndReason::TakenOver,
            "Voice moved to device 'desktop'",
            vec![VoiceAction::TakeOver],
        ),
        (
            "out_of_reach",
            Value::Null,
            "chat thread 7 is out of reach",
            EndReason::OutOfReach,
            "the conversation's thread is gone",
            vec![VoiceAction::Reconnect],
        ),
        (
            "closed",
            json!("the dashboard"),
            "the call was hung up by the dashboard",
            EndReason::Closed,
            "lmgw ended the voice session",
            vec![VoiceAction::Reconnect],
        ),
        (
            "shutting_down",
            Value::Null,
            "lmgw is stopping or restarting",
            EndReason::ShuttingDown,
            "lmgw is shutting down, or its device access changed",
            vec![VoiceAction::Reconnect],
        ),
        (
            "moved_away",
            Value::Null,
            "a newer kind",
            EndReason::Other,
            "lmgw ended the voice session",
            vec![VoiceAction::Reconnect],
        ),
        (
            "revoked",
            Value::Null,
            "certificate_expired: the certificate of device 'phone' expired — pair it again",
            EndReason::Revoked,
            "This device's certificate lapsed: pair it again (New token… on lmgw's Devices card).",
            vec![],
        ),
        (
            "revoked",
            Value::Null,
            "key_moved: device 'phone' moved",
            EndReason::Revoked,
            "lmgw refused this device",
            vec![],
        ),
    ] {
        let mut c = bound();
        c.channel_message(closed(kind, by, message), 10);
        let effects = c.drain();
        let end = ended(&effects).unwrap();
        assert_eq!(end.reason, reason, "{kind}");
        assert_eq!(end.words.as_deref(), Some(words), "{kind}");
        assert_eq!(end.actions, actions, "{kind}");
        assert_eq!(end.message.as_deref(), Some(message), "{kind}");
        assert_eq!(
            effects.iter().any(|e| matches!(e, Effect::Revoked(..))),
            kind == "revoked",
            "{kind}"
        );
        assert!(sent(&effects).is_empty(), "nothing goes after lmgw's end");
        assert_eq!(c.stale_id(), None);
    }
}

#[test]
fn a_stop_whose_stats_come_after_lmgw_s_end_is_not_sent() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.tap(10);
    let r = stats_request(&c.drain()).unwrap();
    c.channel_message(closed("taken_over", json!(DESKTOP), "moved"), 20);
    c.drain();
    c.stats(r, PlayoutStats::default(), 30);
    assert!(sent(&c.drain()).is_empty());
}

#[test]
fn a_refusal_at_the_open_is_its_error_then_closed() {
    let mut c = call();
    c.channel_message(
        ev(
            json!({"type": "error", "error": {"type": "invalid_request_error",
            "code": "folder_no_model", "message": "the folder names no model"}}),
        ),
        10,
    );
    c.channel_message(closed("closed", Value::Null, "refused at the open"), 20);
    let end = ended(&c.drain()).unwrap();
    assert_eq!(
        end.words.as_deref(),
        Some("lmgw refused the voice session: the folder names no model (folder_no_model)")
    );
}

#[test]
fn a_call_that_dies_without_the_event_is_connection_lost_and_its_id_stale() {
    let mut c = bound();
    c.transport(TransportState::Disconnected, "disconnected", 10);
    assert_eq!(c.view(10).phase, CallPhase::Live, "a pause is no end");
    assert!(c.view(10).note.unwrap().contains("interrupted"));
    c.transport(TransportState::Failed, "failed", 20);
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!(end.reason, EndReason::ConnectionLost);
    assert_eq!(end.words.as_deref(), Some("connection lost (failed)"));
    assert_eq!(end.actions, vec![VoiceAction::Reconnect]);
    assert!(effects.contains(&Effect::ClosePeer));
    assert_eq!(c.stale_id(), Some("rtc_1"));
    // The channel closing without lmgw.session.closed.
    let mut c = bound();
    c.channel_closed("closed", 30);
    assert_eq!(
        ended(&c.drain()).unwrap().words.as_deref(),
        Some("connection lost (closed)")
    );
    assert_eq!(c.stale_id(), Some("rtc_1"));
}

#[test]
fn a_failure_on_the_phone_ends_the_call_in_its_words_and_hangs_it_up() {
    let words = "libwebrtc could not set up the call: setRemoteDescription: bad SDP";
    let mut c = call();
    c.failed(words.into(), 50);
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!(end.reason, EndReason::Failed);
    assert_eq!(end.words.as_deref(), Some(words));
    assert_eq!(end.actions, vec![VoiceAction::Reconnect]);
    assert!(effects.contains(&Effect::ClosePeer));
    assert_eq!(c.stale_id(), Some("rtc_1"));
    assert_eq!(c.view(50).words, words);
}

#[test]
fn no_word_from_lmgw_within_the_connect_bound_gives_the_start_up() {
    let mut c = call();
    c.transport(TransportState::Connected, "connected", 300);
    c.drain();
    assert_eq!(c.deadline(300), Some(15_000));
    c.tick(15_000);
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!(end.reason, EndReason::ConnectTimeout);
    let words = end.words.unwrap();
    assert!(words.contains("15 s (voice.connect_timeout_s)"), "{words}");
    assert!(words.contains("since the listening cue"), "{words}");
    assert!(effects.contains(&Effect::ClosePeer));
    assert_eq!(c.stale_id(), Some("rtc_1"));
}

#[test]
fn quiet_for_voice_idle_end_s_ends_the_call_and_activity_resets_it() {
    let mut c = bound();
    c.tick(10_000);
    c.channel_message(
        ev(json!({"type": "input_audio_buffer.speech_started"})),
        10_000,
    );
    c.tick(30_000);
    assert!(ended(&c.drain()).is_none(), "speech is no quiet");
    c.channel_message(
        ev(json!({"type": "input_audio_buffer.speech_stopped"})),
        11_000,
    );
    c.channel_message(ev(json!({"type": "input_audio_buffer.committed"})), 11_000);
    c.channel_message(
        ev(
            json!({"type": "error", "error": {"type": "invalid_request_error",
            "code": "empty_turn", "message": "nothing was said"}}),
        ),
        11_100,
    );
    assert_eq!(
        c.view(11_100).ring,
        RingState::Listening,
        "a refused turn is no thinking"
    );
    c.drain();
    c.tick(41_099);
    assert!(ended(&c.drain()).is_none());
    c.tick(41_100);
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!(end.reason, EndReason::Idle);
    assert_eq!(
        end.words.as_deref(),
        Some("voice ended after 30 s of quiet (voice.idle_end_s)")
    );
    // A pending bind counts as quiet.
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_open(0);
    c.channel_message(pending(DESKTOP), 1000);
    c.drain();
    c.tick(31_000);
    let effects = c.drain();
    assert_eq!(ended(&effects).unwrap().reason, EndReason::Idle);
    assert!(sent(&effects).is_empty(), "nothing was bound: no stop");
    assert!(effects.contains(&Effect::CloseChannel));
}

#[test]
fn a_moved_conversation_rebinds_at_the_next_quiet_point() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.conversation(Some(9), vec![], 100);
    assert!(ended(&c.drain()).is_none(), "Kai still speaks");
    c.channel_message(
        ev(json!({"type": "response.done", "response": {"id": "r1", "status": "completed"}})),
        200,
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.stopped", "response_id": "r1"})),
        300,
    );
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!(end.reason, EndReason::Rebind);
    assert_eq!(end.restart, Some(CallStart::default()));
    let r = stats_request(&effects).unwrap();
    c.stats(r, PlayoutStats::default(), 310);
    c.channel_closed("closed", 320);
    c.drain();
    assert_eq!(c.stale_id(), Some("rtc_1"), "hung up before the next bind");
}

#[test]
fn errors_are_shown_with_their_code_and_bind_pending_is_not() {
    let mut c = bound();
    c.channel_message(
        ev(
            json!({"type": "error", "error": {"type": "invalid_request_error",
            "code": "bind_pending", "message": "the bind is pending"}}),
        ),
        10,
    );
    assert_eq!(c.view(10).error, None);
    c.channel_message(
        ev(
            json!({"type": "error", "error": {"type": "invalid_request_error", "code": "gpu_hold",
            "message": "local models are paused and gemma has no fallback"}}),
        ),
        20,
    );
    assert_eq!(
        c.view(20).error.as_deref(),
        Some("local models are paused and gemma has no fallback (gpu_hold)")
    );
    c.channel_message(
        ev(
            json!({"type": "lmgw.model.state", "stage": "chat", "alias": "gemma", "state": "held",
            "cause": "gpu_hold"}),
        ),
        30,
    );
    assert_eq!(
        c.view(30).held_stage.as_deref(),
        Some("chat gemma (gpu_hold)")
    );
}

#[test]
fn a_tool_after_the_preamble_thinks_while_lmgw_says_starved() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.channel_message(
        ev(
            json!({"type": "lmgw.chat.frame", "response_id": "r1", "event": "tool",
            "data": {"event": "start", "index": 0, "name": "lmgw__status"}}),
        ),
        10,
    );
    assert_eq!(c.view(10).ring, RingState::Speaking, "the preamble plays");
    c.channel_message(
        ev(json!({"type": "lmgw.output_audio.starved", "response_id": "r1", "starved": true})),
        20,
    );
    let v = c.view(20);
    assert_eq!(v.ring, RingState::Thinking);
    assert_eq!(v.words, "Running lmgw__status");
    c.channel_message(
        ev(json!({"type": "lmgw.output_audio.starved", "response_id": "r1", "starved": false})),
        30,
    );
    assert_eq!(c.view(30).ring, RingState::Speaking);
}

#[test]
fn captions_follow_the_transcript_and_the_spoken_words() {
    let mut c = bound();
    c.channel_message(
        ev(json!({"type": "conversation.item.input_audio_transcription.delta", "delta": "hel"})),
        10,
    );
    c.channel_message(
        ev(
            json!({"type": "conversation.item.input_audio_transcription.completed",
            "transcript": "Hello"}),
        ),
        20,
    );
    c.channel_message(
        ev(
            json!({"type": "response.output_audio_transcript.delta", "response_id": "r1",
            "delta": "Hi there"}),
        ),
        30,
    );
    let lines = c.take_captions();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        (lines[0].speaker, lines[0].text.as_str()),
        (Speaker::Owner, "Hello")
    );
    assert_eq!(
        (lines[1].speaker, lines[1].text.as_str()),
        (Speaker::Kai, "Hi there")
    );
    assert!(lines.iter().all(|l| l.call == 1));
}

#[test]
fn a_mode_switch_during_the_call_names_session_lmgw_turn_mode() {
    let mut c = bound();
    c.set_push_to_talk(true, 10);
    let effects = c.drain();
    assert_eq!(
        sent(&effects),
        [json!({"type": "session.update",
            "session": {"type": "realtime", "lmgw": {"turn_mode": "push_to_talk"}}})]
    );
    assert!(effects.contains(&Effect::Mic(MicState::Silent)));
    assert!(c.view(10).push_to_talk);
    c.channel_message(
        ev(json!({"type": "session.updated", "session": {"lmgw": {"turn_mode": "push_to_talk"}}})),
        20,
    );
    assert!(c.view(20).push_to_talk);
}
