//! The phone's audio around the call (design §8.2, §8.4, §8.5): the focus, the headset's buttons,
//! half duplex.

use super::*;
use crate::voice::{AudioFocus, HeadsetPress};

fn ptt_bound() -> Call {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.channel_message(created(7, "push_to_talk"), 0);
    c.drain();
    c
}

#[test]
fn a_transient_focus_loss_stops_kai_and_mutes_until_the_owner_unmutes() {
    let mut c = bound();
    speaking(&mut c, "r1", 100);
    c.drain();
    c.focus(AudioFocus::LostTransient, 200);
    let effects = c.drain();
    assert!(effects.contains(&Effect::Playback(false)), "{effects:?}");
    assert!(
        effects.contains(&Effect::Mic(MicState::Silent)),
        "{effects:?}"
    );
    let r = stats_request(&effects).expect("the stop's stats");
    let v = c.view(200);
    assert!(v.muted && v.paused);
    assert_eq!(v.ring, RingState::Muted);
    assert_eq!(v.words, "Paused: another app took the audio");
    c.stats(
        r,
        PlayoutStats {
            estimated_playout_timestamp: Some(5_000.0),
            ..Default::default()
        },
        210,
    );
    let effects = c.drain();
    assert_eq!(types(&effects), ["lmgw.response.stop"]);
    assert_eq!(sent(&effects)[0]["playout_ntp_ms"], 5_000.0);
    // lmgw's answer to the stop.
    c.channel_message(
        ev(
            json!({"type": "response.done", "response": {"id": "r1", "status": "cancelled",
            "status_details": {"reason": "client_cancelled"}}}),
        ),
        220,
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.cleared", "response_id": "r1"})),
        220,
    );
    c.drain();
    // The focus back: no surprise listening.
    c.focus(AudioFocus::Gained, 300);
    assert!(c.drain().is_empty(), "the microphone stays silent");
    let v = c.view(300);
    assert!(v.muted && !v.paused);
    assert_eq!(v.words, "Muted");
    // The owner's unmute (a tap with nothing open) brings the microphone back.
    c.tap(400);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    assert!(!c.view(400).muted);
}

#[test]
fn an_unmute_while_paused_is_the_owner_s_choice_and_lifts_the_pause() {
    let mut c = bound();
    c.focus(AudioFocus::LostTransient, 10);
    c.drain();
    c.set_muted(false, 20);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    let v = c.view(20);
    assert!(!v.muted && !v.paused);
    assert_eq!(v.words, "Listening");
    // A later focus back changes nothing.
    c.focus(AudioFocus::Gained, 30);
    assert!(c.drain().is_empty());
    assert!(!c.view(30).muted);
}

#[test]
fn a_transient_loss_commits_a_held_press_and_its_stop_cancels_the_owed_response() {
    let mut c = ptt_bound();
    c.press(10);
    c.drain();
    c.focus(AudioFocus::LostTransient, 500);
    let effects = c.drain();
    // The turn is kept (committed), and the stop goes behind it, so lmgw cancels the response the
    // commit asked for: Kai does not answer into the other app's audio.
    assert_eq!(
        types(&effects),
        ["input_audio_buffer.commit", "response.create"]
    );
    let r = stats_request(&effects).expect("the stop");
    c.stats(r, PlayoutStats::default(), 510);
    assert_eq!(types(&c.drain()), ["lmgw.response.stop"]);
    let v = c.view(510);
    assert!(v.paused && v.muted && !v.pressed);
    // A press while paused is ignored, as while muted.
    c.press(600);
    assert!(c.drain().is_empty());
}

#[test]
fn a_pause_stops_the_idle_clock_however_long_and_the_focus_back_starts_it_again() {
    // Quiet from t=0: voice.idle_end_s (30 s) would end the call at 30 000.
    let mut c = bound();
    c.focus(AudioFocus::LostTransient, 10_000);
    c.drain();
    // A phone call far longer than voice.idle_end_s ends nothing.
    c.tick(600_000);
    assert!(ended(&c.drain()).is_none(), "a pause is not idle");
    assert_eq!(c.deadline(600_000), None, "no idle deadline while paused");
    // The focus back starts the clock again from its return.
    c.focus(AudioFocus::Gained, 600_000);
    c.drain();
    assert_eq!(c.deadline(600_000), Some(630_000));
    c.tick(629_999);
    assert!(ended(&c.drain()).is_none());
    c.tick(630_000);
    assert_eq!(ended(&c.drain()).unwrap().reason, EndReason::Idle);

    // The owner's unmute lifts the pause, not the other app's hold on the audio: the clock stands
    // until the focus is back.
    let mut c = bound();
    c.focus(AudioFocus::LostTransient, 10_000);
    c.set_muted(false, 20_000);
    c.drain();
    assert!(!c.view(20_000).paused);
    c.tick(600_000);
    assert!(ended(&c.drain()).is_none());
    c.focus(AudioFocus::Gained, 700_000);
    assert_eq!(c.deadline(700_000), Some(730_000));

    // A pending bind's idle clock stands still too.
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_open(0);
    c.channel_message(pending(DESKTOP), 1000);
    c.focus(AudioFocus::LostTransient, 2000);
    c.drain();
    c.tick(600_000);
    assert!(ended(&c.drain()).is_none());
    c.focus(AudioFocus::Gained, 600_000);
    c.tick(630_000);
    assert_eq!(ended(&c.drain()).unwrap().reason, EndReason::Idle);
}

#[test]
fn a_paused_call_still_ends_on_a_failure_and_on_lmgw_s_word() {
    let mut c = bound();
    c.focus(AudioFocus::LostTransient, 10);
    c.drain();
    c.failed("libwebrtc could not send".into(), 20);
    let end = ended(&c.drain()).unwrap();
    assert_eq!(end.reason, EndReason::Failed);

    let mut c = bound();
    c.focus(AudioFocus::LostTransient, 10);
    c.drain();
    c.channel_message(
        closed(
            "shutting_down",
            Value::Null,
            "lmgw is stopping or restarting",
        ),
        20,
    );
    assert_eq!(
        ended(&c.drain()).unwrap().reason,
        EndReason::ShuttingDown,
        "lmgw's word ends a paused call"
    );
}

#[test]
fn a_permanent_focus_loss_ends_the_call_by_the_stop_and_the_close() {
    let mut c = bound();
    speaking(&mut c, "r1", 0);
    c.drain();
    c.focus(AudioFocus::Lost, 100);
    let effects = c.drain();
    let end = ended(&effects).unwrap();
    assert_eq!(end.reason, EndReason::AudioTaken);
    assert_eq!(
        end.words.as_deref(),
        Some("voice ended: another app took the audio")
    );
    assert!(end.actions.is_empty(), "Talk starts the next call");
    assert!(!effects.contains(&Effect::CloseChannel), "after the stop");
    let r = stats_request(&effects).unwrap();
    c.stats(r, PlayoutStats::default(), 110);
    let effects = c.drain();
    assert_eq!(types(&effects), ["lmgw.response.stop"]);
    assert_eq!(effects.last(), Some(&Effect::CloseChannel));
    c.channel_closed("closed", 150);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), None, "a clean end");
    assert_eq!(c.view(150).words, "voice ended: another app took the audio");
}

#[test]
fn a_permanent_loss_before_lmgw_s_first_word_hangs_the_call_up_later() {
    let mut c = call();
    c.focus(AudioFocus::Lost, 50);
    let effects = c.drain();
    assert_eq!(ended(&effects).unwrap().reason, EndReason::AudioTaken);
    assert!(effects.contains(&Effect::ClosePeer));
    assert_eq!(c.stale_id(), Some("rtc_1"));
    // An ended call takes no more focus changes.
    c.focus(AudioFocus::LostTransient, 60);
    assert!(c.drain().is_empty());
    assert!(!c.view(60).paused);
}

#[test]
fn a_headset_press_in_automatic_mode_is_the_ring_s_tap_and_a_long_press_ends() {
    let mut c = bound();
    c.headset(HeadsetPress::Click, 10);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Silent)]);
    assert!(c.view(10).muted);
    c.headset(HeadsetPress::Click, 20);
    assert_eq!(c.drain(), vec![Effect::Mic(MicState::Live)]);
    assert!(!c.view(20).muted);
    // While Kai speaks the press stops it, and does not also mute (kai K6).
    speaking(&mut c, "r1", 30);
    c.drain();
    c.headset(HeadsetPress::Click, 40);
    let effects = c.drain();
    assert!(effects.contains(&Effect::Playback(false)), "{effects:?}");
    assert!(stats_request(&effects).is_some());
    assert!(!c.view(40).muted);
    c.headset(HeadsetPress::Long, 50);
    let end = ended(&c.drain()).unwrap();
    assert_eq!((end.reason, end.words), (EndReason::Ended, None));
}

#[test]
fn a_headset_press_in_push_to_talk_starts_the_turn_and_the_next_commits_it() {
    let mut c = ptt_bound();
    c.headset(HeadsetPress::Click, 100);
    let effects = c.drain();
    assert_eq!(types(&effects), ["input_audio_buffer.clear"]);
    assert!(effects.contains(&Effect::Mic(MicState::Live)));
    assert!(c.view(100).pressed);
    c.headset(HeadsetPress::Click, 900);
    assert_eq!(
        types(&c.drain()),
        ["input_audio_buffer.commit", "response.create"]
    );
    assert!(!c.view(900).pressed);
}

#[test]
fn half_duplex_travels_in_the_session_part_with_its_tail() {
    let mut f = facts();
    f.half_duplex = true;
    f.echo_tail_ms = Some(300);
    let c = call_with(f.clone(), CallStart::default(), None);
    assert_eq!(
        c.session_part().unwrap().to_value(),
        json!({"type": "realtime", "lmgw": {"half_duplex": true, "echo_tail_ms": 300}})
    );
    assert!(c.view(0).half_duplex);
    // Without a tail of the phone's, lmgw's own value stands.
    f.echo_tail_ms = None;
    let c = call_with(f, CallStart::default(), None);
    assert_eq!(
        c.session_part().unwrap().to_value(),
        json!({"type": "realtime", "lmgw": {"half_duplex": true}})
    );
    // Full duplex sends no tail, whatever the setting.
    let mut f = facts();
    f.echo_tail_ms = Some(300);
    let c = call_with(f, CallStart::default(), None);
    assert_eq!(c.session_part(), None);
    assert!(!c.view(0).half_duplex);
}
