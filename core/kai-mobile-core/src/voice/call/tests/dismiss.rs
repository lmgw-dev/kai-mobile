//! The assistant overlay's dismissal (design §7.7's "dismissed early", §9.2): before the owner's
//! first turn it ends the call, after it the call goes on, and a turn under way decides by its words.

use super::*;

fn transcript(text: &str) -> String {
    ev(json!({"type": "conversation.item.input_audio_transcription.completed", "transcript": text}))
}

fn committed() -> String {
    ev(json!({"type": "input_audio_buffer.committed", "item_id": "item_1"}))
}

/// The end said by the stop's stats and the channel's close, as the owner's End.
fn hung_up(c: &mut Call, now: u64) -> CallEnd {
    let effects = c.drain();
    let end = ended(&effects).expect("the call ends");
    let r = stats_request(&effects).expect("the stop's stats");
    c.stats(r, PlayoutStats::default(), now);
    let effects = c.drain();
    assert_eq!(types(&effects), ["lmgw.response.stop"], "{effects:?}");
    assert_eq!(effects.last(), Some(&Effect::CloseChannel));
    end
}

#[test]
fn a_dismissal_before_the_first_turn_ends_the_call_without_words() {
    let mut c = bound();
    assert!(!c.view(0).spoken);
    c.dismissed(500);
    let end = hung_up(&mut c, 510);
    assert_eq!(end.reason, EndReason::Dismissed);
    assert_eq!(
        end.words, None,
        "an accidental invocation leaves nothing open"
    );
    assert!(end.actions.is_empty());
    assert_eq!(c.view(510).phase, CallPhase::Ending);
    c.channel_closed("closed", 520);
    assert_eq!(c.drain(), vec![Effect::ClosePeer]);
    assert_eq!(c.stale_id(), None, "lmgw ends a call whose channel closed");
}

#[test]
fn a_dismissal_before_the_bind_ends_the_start_and_hangs_its_id_up() {
    let mut c = call();
    c.transport(TransportState::Connected, "connected", 300);
    c.drain();
    c.dismissed(400);
    let effects = c.drain();
    assert_eq!(ended(&effects).unwrap().reason, EndReason::Dismissed);
    assert!(effects.contains(&Effect::ClosePeer), "{effects:?}");
    assert!(sent(&effects).is_empty());
    assert_eq!(
        c.stale_id(),
        Some("rtc_1"),
        "lmgw may hold the call already"
    );
}

#[test]
fn a_dismissal_while_the_bind_is_pending_declines() {
    let mut c = call_with(facts(), CallStart::default(), Some(DESKTOP));
    c.channel_open(0);
    c.channel_message(pending(DESKTOP), 10);
    c.drain();
    c.dismissed(20);
    let effects = c.drain();
    assert!(
        sent(&effects).is_empty(),
        "nothing but take_over goes while pending"
    );
    assert_eq!(ended(&effects).unwrap().reason, EndReason::Declined);
    assert!(effects.contains(&Effect::CloseChannel));
}

#[test]
fn after_a_turn_with_words_the_call_goes_on() {
    let mut c = bound();
    c.channel_message(committed(), 100);
    c.channel_message(transcript("What's on my calendar?"), 400);
    assert!(c.view(400).spoken);
    c.drain();
    c.dismissed(500);
    let effects = c.drain();
    assert_eq!(ended(&effects), None, "{effects:?}");
    assert!(sent(&effects).is_empty());
    assert_eq!(c.view(500).phase, CallPhase::Live);
}

#[test]
fn a_reply_kai_completed_counts_as_the_first_turn() {
    let mut c = bound();
    speaking(&mut c, "r1", 100);
    c.channel_message(
        ev(json!({"type": "response.done", "response": {"id": "r1", "status": "completed"}})),
        900,
    );
    assert!(c.view(900).spoken);
    c.drain();
    c.dismissed(1000);
    assert_eq!(ended(&c.drain()), None);
}

#[test]
fn a_turn_under_way_decides_by_its_words() {
    // With words: the call goes on.
    let mut c = bound();
    c.channel_message(committed(), 100);
    c.drain();
    c.dismissed(150);
    assert_eq!(ended(&c.drain()), None, "the turn's words decide");
    c.channel_message(transcript("Turn the heating down"), 400);
    assert_eq!(ended(&c.drain()), None);
    assert!(c.view(400).spoken);
    // A later blank turn changes nothing.
    c.channel_message(committed(), 2000);
    c.channel_message(transcript(" "), 2100);
    assert_eq!(ended(&c.drain()), None);

    // Without: it ends as dismissed once the transcript says so.
    let mut c = bound();
    c.channel_message(committed(), 100);
    c.dismissed(150);
    c.drain();
    c.channel_message(transcript(""), 400);
    assert_eq!(hung_up(&mut c, 410).reason, EndReason::Dismissed);
}

#[test]
fn a_failed_or_empty_turn_under_way_ends_the_dismissed_call() {
    // The transcript failed.
    let mut c = bound();
    c.channel_message(committed(), 100);
    c.dismissed(150);
    c.drain();
    c.channel_message(
        ev(
            json!({"type": "conversation.item.input_audio_transcription.failed",
            "error": {"type": "server_error", "code": "asr_failed", "message": "no speech"}}),
        ),
        300,
    );
    assert_eq!(hung_up(&mut c, 310).reason, EndReason::Dismissed);

    // lmgw refused the turn as empty (`empty_turn`) after its response was created.
    let mut c = bound();
    c.channel_message(committed(), 100);
    c.channel_message(
        ev(json!({"type": "response.created", "response": {"id": "r1"}})),
        120,
    );
    c.dismissed(150);
    c.drain();
    c.channel_message(
        ev(
            json!({"type": "response.done", "response": {"id": "r1", "status": "failed",
            "status_details": {"error": {"code": "empty_turn"}}}}),
        ),
        300,
    );
    assert_eq!(hung_up(&mut c, 310).reason, EndReason::Dismissed);
}

#[test]
fn a_push_to_talk_turn_released_before_the_dismissal_decides_by_its_words() {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.channel_message(created(7, "push_to_talk"), 0);
    c.press(100);
    // The overlay goes while the ring is held: the gesture's release reaches the core first.
    c.release(900);
    c.drain();
    c.dismissed(910);
    assert_eq!(ended(&c.drain()), None);
    c.channel_message(committed(), 1100);
    c.channel_message(transcript("Remind me at six"), 1300);
    assert_eq!(ended(&c.drain()), None);
    assert!(c.view(1300).spoken);
}

fn speech_started() -> String {
    ev(
        json!({"type": "input_audio_buffer.speech_started", "audio_start_ms": 0, "item_id": "item_1"}),
    )
}

#[test]
fn a_dismissal_mid_utterance_keeps_a_turn_with_words() {
    let mut c = bound();
    c.channel_message(speech_started(), 100);
    c.dismissed(300);
    assert_eq!(ended(&c.drain()), None, "the utterance's words decide");
    c.channel_message(committed(), 600);
    assert_eq!(ended(&c.drain()), None, "a commit alone decides nothing");
    c.channel_message(transcript("Turn the heating down"), 900);
    assert_eq!(ended(&c.drain()), None);
    assert!(c.view(900).spoken);
}

#[test]
fn a_cough_mid_dismissal_still_ends_the_call() {
    let mut c = bound();
    c.channel_message(speech_started(), 100);
    c.dismissed(300);
    c.drain();
    c.channel_message(committed(), 600);
    c.channel_message(transcript("  "), 900);
    assert_eq!(hung_up(&mut c, 910).reason, EndReason::Dismissed);

    // Speech that was cleared before any commit has no words either.
    let mut c = bound();
    c.channel_message(speech_started(), 100);
    c.dismissed(300);
    c.drain();
    c.channel_message(ev(json!({"type": "input_audio_buffer.cleared"})), 500);
    assert_eq!(hung_up(&mut c, 510).reason, EndReason::Dismissed);
}

#[test]
fn a_dismissal_while_push_to_talk_is_held_waits_for_the_release_and_the_words() {
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.channel_message(created(7, "push_to_talk"), 0);
    c.press(100);
    c.dismissed(400);
    assert_eq!(ended(&c.drain()), None, "the held ring is a turn under way");
    c.release(900);
    assert_eq!(
        ended(&c.drain()),
        None,
        "the release opens the turn; its words decide"
    );
    c.channel_message(committed(), 1000);
    c.channel_message(transcript("Remind me at six"), 1300);
    assert_eq!(ended(&c.drain()), None);
    assert!(c.view(1300).spoken);

    // The same release with a blank transcript ends the call.
    let mut f = facts();
    f.mode = VoiceMode::PushToTalk;
    let mut c = call_with(f, CallStart::default(), None);
    c.transport(TransportState::Connected, "connected", 0);
    c.channel_open(0);
    c.channel_message(created(7, "push_to_talk"), 0);
    c.press(100);
    c.dismissed(400);
    c.release(900);
    c.drain();
    c.channel_message(committed(), 1000);
    c.channel_message(transcript(""), 1300);
    assert_eq!(hung_up(&mut c, 1310).reason, EndReason::Dismissed);
}

#[test]
fn two_commits_in_flight_are_two_turns() {
    // The first is blank, the second has words: the call waits for both and goes on.
    let mut c = bound();
    c.channel_message(speech_started(), 100);
    c.channel_message(committed(), 300);
    c.channel_message(speech_started(), 400);
    c.channel_message(committed(), 600);
    c.dismissed(650);
    c.drain();
    c.channel_message(transcript(""), 800);
    assert_eq!(ended(&c.drain()), None, "one turn is still open");
    c.channel_message(transcript("Call the plumber"), 1000);
    assert_eq!(ended(&c.drain()), None);
    assert!(c.view(1000).spoken);

    // Both blank: it ends at the second.
    let mut c = bound();
    c.channel_message(committed(), 300);
    c.channel_message(committed(), 600);
    c.dismissed(650);
    c.drain();
    c.channel_message(transcript(""), 800);
    assert_eq!(ended(&c.drain()), None);
    c.channel_message(transcript(" "), 1000);
    assert_eq!(hung_up(&mut c, 1010).reason, EndReason::Dismissed);
}

#[test]
fn a_dismissal_after_the_end_changes_nothing() {
    let mut c = bound();
    c.end(100);
    c.drain();
    c.dismissed(200);
    let effects = c.drain();
    assert_eq!(ended(&effects), None);
    assert!(sent(&effects).is_empty());
}
