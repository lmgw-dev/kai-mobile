//! Approvals in the call (design §11, api §6.4) and the late task result's continuation (§7.6's
//! seam; lmgw's MCP Tasks design §3.4).

use super::*;
use crate::approvals::Waiting;

fn item(id: &str) -> String {
    ev(
        json!({"type": "conversation.item.done", "item": {"id": id, "type": "mcp_approval_request",
        "server_label": "desktop", "name": "open_file", "arguments": "{\"path\":\"/tmp/x\"}"}}),
    )
}

fn waiting(thread: i64) -> Vec<Waiting> {
    vec![Waiting {
        thread_id: thread,
        tool: "desktop__open_file".into(),
    }]
}

fn task_done() -> String {
    ev(
        json!({"type": "lmgw.task.done", "thread_id": 7, "message_id": 40, "id": 3,
        "task_id": "t3", "server_label": "builder", "tool": "builder__build",
        "status": "completed", "by": null}),
    )
}

fn outcome(effects: &[Effect]) -> Option<CallVerdicts> {
    effects.iter().find_map(|e| match e {
        Effect::Verdicts(v) => Some(v.clone()),
        _ => None,
    })
}

/// The verdicts' `response.create` with its `event_id`, after one item per verdict.
fn verdicts_sent(c: &mut Call, now: u64) -> String {
    assert!(c.verdicts(
        7,
        vec![("mcpr_a".into(), true), ("mcpr_b".into(), false)],
        now
    ));
    let s = sent(&c.drain());
    assert_eq!(s.len(), 3, "{s:?}");
    assert_eq!(s[0]["type"], "conversation.item.create");
    assert_eq!(s[0]["item"]["type"], "mcp_approval_response");
    assert_eq!(s[0]["item"]["approval_request_id"], "mcpr_a");
    assert_eq!(s[0]["item"]["approve"], true);
    assert_eq!(s[1]["item"]["approve"], false);
    assert_eq!(s[2]["type"], "response.create");
    s[2]["event_id"].as_str().expect("tagged").to_owned()
}

#[test]
fn a_call_the_session_shows_is_a_card_and_asks_for_attention_once() {
    let mut c = bound();
    c.channel_message(item("mcpr_a"), 100);
    let effects = c.drain();
    let Some(Effect::ApprovalShown(a)) = effects.first() else {
        panic!("{effects:?}")
    };
    assert_eq!(
        (
            a.approval_request_id.as_str(),
            a.thread_id,
            a.thread_title.as_str(),
            a.message_id
        ),
        ("mcpr_a", 7, "Kai", 0)
    );
    assert!(effects.contains(&Effect::Asked));
    // The same item again (a reconnecting reader, a repeat): a card again, no second ask.
    c.channel_message(item("mcpr_a"), 200);
    assert!(!c.drain().contains(&Effect::Asked));
    // lmgw's word that another client decided it.
    c.channel_message(
        ev(
            json!({"type": "lmgw.approval.decided", "approval_request_id": "mcpr_a",
            "approve": true, "by": "the dashboard"}),
        ),
        300,
    );
    assert_eq!(c.drain(), [Effect::ApprovalDecided("mcpr_a".into())]);
}

#[test]
fn a_call_waiting_in_the_thread_is_said_and_keeps_the_idle_clock_still() {
    let mut c = bound();
    c.approvals(waiting(7), 1000);
    let v = c.view(1000);
    assert_eq!(v.words, "Kai waits for your decision on desktop__open_file");
    assert_eq!(v.awaiting, ["desktop__open_file"]);
    assert_eq!(
        v.ring,
        RingState::Listening,
        "the microphone is not held back"
    );
    c.tick(60_000);
    assert!(
        ended(&c.drain()).is_none(),
        "a pending approval is no quiet"
    );
    // Another thread's call is not this call's.
    c.approvals(waiting(8), 60_000);
    assert_eq!(c.view(60_000).words, "Listening");
    assert!(c.view(60_000).awaiting.is_empty());
    c.tick(89_999);
    assert!(ended(&c.drain()).is_none());
    c.tick(90_000);
    assert_eq!(ended(&c.drain()).unwrap().reason, EndReason::Idle);
}

fn unsettled() -> Option<CallVerdicts> {
    Some(CallVerdicts::Unsettled {
        thread_id: 7,
        verdicts: vec![("mcpr_a".into(), true), ("mcpr_b".into(), false)],
    })
}

fn done(id: &str, status: &str) -> String {
    ev(json!({"type": "response.done", "response": {"id": id, "status": status}}))
}

#[test]
fn verdicts_go_as_items_and_a_tagged_create_and_a_resumed_turn_that_speaks_probably_took_them() {
    let mut c = bound();
    let tag = verdicts_sent(&mut c, 100);
    assert!(tag.starts_with("kai_approvals_1_"), "{tag}");
    assert!(
        !c.verdicts(7, vec![("mcpr_c".into(), true)], 110),
        "one batch at a time"
    );
    speaking(&mut c, "r9", 200);
    c.channel_message(done("r9", "completed"), 900);
    // Only the phone's inference: the book keeps them sent until the feed says (approvals.rs).
    assert_eq!(
        outcome(&c.drain()),
        Some(CallVerdicts::Taken(vec!["mcpr_a".into(), "mcpr_b".into()]))
    );
}

/// Approve while the owner still talks: lmgw holds the create for their turn, and its words
/// decline what waits (the verdicts go unused). The response the call sees speaks as the resumed
/// one would, so the call can only say "probably": the feed's approval.decided tells the truth.
#[test]
fn approve_while_talking_looks_taken_to_the_call_and_the_feed_settles_it() {
    let mut c = bound();
    c.channel_message(ev(json!({"type": "input_audio_buffer.speech_started"})), 50);
    verdicts_sent(&mut c, 100);
    c.channel_message(ev(json!({"type": "input_audio_buffer.committed"})), 300);
    c.channel_message(
        ev(
            json!({"type": "conversation.item.input_audio_transcription.completed",
            "item_id": "i1", "transcript": "never mind, what time is it"}),
        ),
        400,
    );
    speaking(&mut c, "r5", 500);
    c.channel_message(done("r5", "completed"), 1500);
    assert!(matches!(outcome(&c.drain()), Some(CallVerdicts::Taken(_))));
}

#[test]
fn a_cancelled_or_silent_resume_goes_back_to_the_route() {
    // A stop cancels it once it ran (lmgw may not have decided yet).
    let mut c = bound();
    verdicts_sent(&mut c, 100);
    speaking(&mut c, "r9", 200);
    c.drain();
    c.tap(300);
    let r = stats_request(&c.drain()).unwrap();
    c.stats(r, PlayoutStats::default(), 310);
    c.drain();
    c.channel_message(done("r9", "cancelled"), 400);
    assert_eq!(outcome(&c.drain()), unsettled());
    // Completed without a word, a sound or a frame.
    let mut c = bound();
    verdicts_sent(&mut c, 100);
    c.channel_message(
        ev(json!({"type": "response.created", "response": {"id": "r9"}})),
        200,
    );
    c.channel_message(done("r9", "completed"), 300);
    assert_eq!(outcome(&c.drain()), unsettled());
    // A chat frame of the resumed turn is output: lmgw decided before its turn framed anything.
    let mut c = bound();
    verdicts_sent(&mut c, 100);
    c.channel_message(
        ev(json!({"type": "response.created", "response": {"id": "r9"}})),
        200,
    );
    c.channel_message(
        ev(
            json!({"type": "lmgw.chat.frame", "response_id": "r9", "event": "tool",
            "data": {"event": "start", "name": "desktop__open_file"}}),
        ),
        250,
    );
    c.channel_message(done("r9", "completed"), 300);
    assert!(matches!(outcome(&c.drain()), Some(CallVerdicts::Taken(_))));
}

#[test]
fn only_an_approval_code_but_approval_decided_or_key_rate_is_not_taken() {
    let error = |code: &str, tag: Option<&str>| {
        let mut e = json!({"type": "invalid_request_error", "code": code,
            "message": "lmgw's words about desktop__open_file"});
        if let Some(t) = tag {
            e["event_id"] = t.into();
        }
        ev(json!({"type": "error", "error": e}))
    };
    for code in ["approval_out_of_scope", "approval_missing", "key_rate"] {
        let mut c = bound();
        let tag = verdicts_sent(&mut c, 100);
        c.channel_message(error(code, Some(&tag)), 150);
        let effects = c.drain();
        assert_eq!(
            outcome(&effects),
            Some(CallVerdicts::NotTaken {
                ids: vec!["mcpr_a".into(), "mcpr_b".into()],
                code: Some(code.into()),
                words: "lmgw's words about desktop__open_file".into(),
            }),
            "{code}"
        );
        assert!(c.view(150).error.is_some(), "the call's error too");
    }
    // An approval code without the echo is about them too.
    let mut c = bound();
    verdicts_sent(&mut c, 100);
    c.channel_message(error("approval_out_of_scope", None), 150);
    assert!(matches!(
        outcome(&c.drain()),
        Some(CallVerdicts::NotTaken { .. })
    ));
    // Decided already, the resumed turn's failure (the model, a timeout, the hold), anything
    // else: lmgw may have decided first, so the route settles it.
    for code in [
        "approval_decided",
        "gpu_hold",
        "upstream_timeout",
        "conversation_already_has_active_response",
    ] {
        let mut c = bound();
        let tag = verdicts_sent(&mut c, 100);
        c.channel_message(
            ev(json!({"type": "response.created", "response": {"id": "r9"}})),
            120,
        );
        c.channel_message(error(code, Some(&tag)), 150);
        assert_eq!(outcome(&c.drain()), unsettled(), "{code}");
        // The response's failed end after it changes nothing.
        c.channel_message(done("r9", "failed"), 160);
        assert_eq!(outcome(&c.drain()), None, "{code}");
    }
    // A failed resume with no error about it.
    let mut c = bound();
    verdicts_sent(&mut c, 100);
    c.channel_message(
        ev(json!({"type": "response.created", "response": {"id": "r9"}})),
        200,
    );
    c.channel_message(
        ev(
            json!({"type": "response.done", "response": {"id": "r9", "status": "failed",
            "status_details": {"type": "failed", "error": {"code": "gpu_hold",
                "message": "local models are paused"}}}}),
        ),
        300,
    );
    assert_eq!(outcome(&c.drain()), unsettled());
}

#[test]
fn an_end_before_lmgw_answered_hands_the_verdicts_back_and_an_unbound_call_takes_none() {
    let mut c = bound();
    verdicts_sent(&mut c, 100);
    c.end(200);
    let effects = c.drain();
    assert_eq!(
        outcome(&effects),
        Some(CallVerdicts::Unsettled {
            thread_id: 7,
            verdicts: vec![("mcpr_a".into(), true), ("mcpr_b".into(), false)],
        })
    );
    assert!(
        effects
            .iter()
            .position(|e| matches!(e, Effect::Verdicts(_)))
            < effects.iter().position(|e| matches!(e, Effect::Ended(_))),
        "handed back before the end: {effects:?}"
    );
    // Before the bind nothing takes them (the route does).
    let mut c = call();
    assert!(!c.verdicts(7, vec![("mcpr_a".into(), true)], 10));
    assert!(sent(&c.drain()).is_empty());
}

#[test]
fn a_late_task_result_is_answered_by_a_continuation_when_the_call_is_quiet() {
    let mut c = bound();
    c.channel_message(task_done(), 1000);
    let s = sent(&c.drain());
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0]["type"], "response.create");
    let tag = s[0]["event_id"].as_str().unwrap();
    assert!(tag.starts_with("kai_continue_1_"), "{tag}");
    // Nothing to answer (another window answered): logged, not shown.
    c.channel_message(
        ev(
            json!({"type": "error", "error": {"type": "invalid_request_error",
            "code": "empty_turn", "message": "nothing to answer", "event_id": tag}}),
        ),
        1100,
    );
    c.drain();
    assert_eq!(c.view(1100).error, None);
    assert!(c.take_captions().iter().all(|l| l.speaker != Speaker::Note));
}

/// A result enters only while no turn of the thread runs: one that ended while Kai spoke entered
/// after that turn's request, so the continuation is owed, and goes once the call is quiet.
#[test]
fn a_task_that_ends_while_kai_speaks_is_continued_once_the_call_is_quiet() {
    let mut c = bound();
    speaking(&mut c, "r1", 100);
    c.drain();
    c.channel_message(task_done(), 200);
    assert!(sent(&c.drain()).is_empty(), "never over Kai's speech");
    c.channel_message(done("r1", "completed"), 300);
    assert!(
        sent(&c.drain()).is_empty(),
        "Kai's audio still plays: {:?}",
        c.view(300).ring
    );
    c.channel_message(
        ev(json!({"type": "output_audio_buffer.stopped", "response_id": "r1"})),
        400,
    );
    let s = sent(&c.drain());
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0]["type"], "response.create");
    assert!(
        s[0]["event_id"]
            .as_str()
            .unwrap()
            .starts_with("kai_continue_1_")
    );
}

#[test]
fn the_owner_s_turn_under_way_answers_the_result_and_no_create_goes() {
    let mut c = bound();
    c.channel_message(
        ev(json!({"type": "input_audio_buffer.speech_started"})),
        100,
    );
    c.channel_message(task_done(), 200);
    c.channel_message(ev(json!({"type": "input_audio_buffer.committed"})), 300);
    assert!(sent(&c.drain()).is_empty());
    // Owed while Kai speaks, then the owner speaks over it: their turn answers it.
    let mut c = bound();
    speaking(&mut c, "r1", 100);
    c.channel_message(task_done(), 200);
    c.channel_message(
        ev(json!({"type": "input_audio_buffer.speech_started"})),
        300,
    );
    c.channel_message(done("r1", "cancelled"), 310);
    c.channel_message(ev(json!({"type": "input_audio_buffer.committed"})), 900);
    c.channel_message(
        ev(
            json!({"type": "conversation.item.input_audio_transcription.completed",
            "item_id": "i1", "transcript": "and the build?"}),
        ),
        1000,
    );
    assert!(types(&c.drain()).iter().all(|t| t != "response.create"));
}

#[test]
fn a_continuation_waits_for_the_audio_and_for_a_pending_verdict() {
    // Another app holds the audio: owed until the focus is back.
    let mut c = bound();
    c.focus(crate::voice::AudioFocus::LostTransient, 100);
    let r = stats_request(&c.drain()).unwrap();
    c.stats(r, PlayoutStats::default(), 110);
    c.drain();
    c.channel_message(task_done(), 200);
    assert!(types(&c.drain()).iter().all(|t| t != "response.create"));
    c.focus(crate::voice::AudioFocus::Gained, 5000);
    assert_eq!(types(&c.drain()), ["response.create"]);
    // A call of the thread waits for the owner's verdict: a continuation's reply would decline it.
    let mut c = bound();
    c.approvals(waiting(7), 100);
    c.channel_message(task_done(), 200);
    assert!(sent(&c.drain()).is_empty());
    c.approvals(Vec::new(), 300);
    assert_eq!(types(&c.drain()), ["response.create"]);
}
