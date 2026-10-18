//! The book's rules (design §11, §12.3).

use super::*;

fn call(id: &str, thread: i64, message: i64) -> ApprovalRequested {
    ApprovalRequested {
        thread_id: thread,
        thread_title: "Kai".into(),
        message_id: message,
        approval_request_id: id.into(),
        server_label: "desktop".into(),
        name: "open_file".into(),
        arguments: r#"{"path":"/tmp/x","line":3}"#.into(),
        call_id: None,
    }
}

#[test]
fn a_call_is_notified_once_while_away_and_withdrawn_when_it_goes() {
    let mut b = Approvals::new(vec![]);
    b.set_open(vec![call("a", 7, 3)]);
    let out = b.change(ApprovalsChange::Added(call("a", 7, 3)), false);
    assert_eq!(out.updates.len(), 1);
    let again = b.change(ApprovalsChange::Added(call("a", 7, 3)), false);
    assert!(again.updates.is_empty(), "notified already");
    assert_eq!(b.known(), ["a"]);
    let out = b.change(ApprovalsChange::Withdrawn(vec!["a".into()]), false);
    assert_eq!(
        out.updates,
        [Update::Withdraw {
            id: notification_id("a")
        }]
    );
}

#[test]
fn a_dismissed_call_stays_dismissed_and_is_cut_at_caught_up() {
    let mut b = Approvals::new(vec!["a".into(), "gone".into()]);
    b.set_open(vec![call("a", 7, 3)]);
    assert!(
        b.change(ApprovalsChange::Added(call("a", 7, 3)), false)
            .updates
            .is_empty()
    );
    let out = b.caught_up();
    assert!(out.save);
    assert_eq!(b.dismissed(), ["a"]);
    assert!(b.backgrounded().updates.is_empty());
}

#[test]
fn verdicts_wait_for_every_call_of_the_reply() {
    let mut b = Approvals::new(vec![]);
    b.set_open(vec![call("a", 7, 3), call("b", 7, 3), call("c", 8, 1)]);
    let d = b.decide("a", true, true).unwrap();
    assert_eq!((d.send.is_none(), d.remaining), (true, 1));
    assert!(d.updates.is_empty(), "visible: the card shows it");
    let d = b.decide("b", false, true).unwrap();
    assert_eq!(
        d.send.unwrap(),
        [("a".to_owned(), true), ("b".to_owned(), false)]
    );
    assert!(b.decide("zz", true, true).is_err());
}

#[test]
fn a_held_verdict_is_never_asked_again_and_names_itself_on_the_calls_it_waits_for() {
    let mut b = Approvals::new(vec!["b".into()]);
    b.set_open(vec![call("a", 7, 3), call("b", 7, 3)]);
    let out = b.change(ApprovalsChange::Added(call("a", 7, 3)), false);
    assert_eq!(out.updates.len(), 1);
    // From a notification, the app away: a withdrawn, b (swiped before) raised again.
    let d = b.decide("a", true, false).unwrap();
    assert_eq!(d.remaining, 1);
    assert!(d.save, "b is no longer dismissed");
    assert_eq!(
        d.updates[0],
        Update::Withdraw {
            id: notification_id("a")
        }
    );
    let Update::Notify { notification: n } = &d.updates[1] else {
        panic!("{:?}", d.updates)
    };
    assert_eq!(n.id, notification_id("b"));
    assert!(
        n.text
            .starts_with("Your verdict on desktop__open_file waits for this one.")
    );
    // Neither leaving the screen nor a reconnect's Added asks for a again.
    assert!(b.backgrounded().updates.is_empty());
    assert!(
        b.change(ApprovalsChange::Added(call("a", 7, 3)), false)
            .updates
            .is_empty()
    );
    assert_eq!(b.views()[0].verdict, Some(true));
}

#[test]
fn arguments_read_in_compact_lines() {
    assert_eq!(
        compact_arguments(r#"{"path":"/tmp/x","line":3}"#),
        "line: 3\npath: /tmp/x"
    );
    assert_eq!(compact_arguments("[1]"), "[1]");
    let n = notification(&call("a", 7, 3), &[]);
    assert_eq!(n.title, "Kai wants to run desktop__open_file");
    assert!(n.text.ends_with("in 'Kai'"));
}

// ---------------------------------------------------------------- approvals in a voice call

/// What the call's session shows: the call's thread, no message id.
fn shown(id: &str, call_n: u64) -> Shown {
    Shown {
        call: call_n,
        request: call(id, 7, 0),
    }
}

fn withdraw(id: &str) -> Update {
    Update::Withdraw {
        id: notification_id(id),
    }
}

fn notified(out: &Out) -> Vec<String> {
    out.updates
        .iter()
        .filter_map(|u| match u {
            Update::Notify { notification } => Some(notification.id.key.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_call_the_session_shows_is_one_card_and_asked_once_whichever_source_comes_first() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(1));
    // The session first, the app away: a card and a notification at once.
    let out = b.call_shown(shown("a", 1), false);
    assert!(out.cards);
    assert_eq!(notified(&out), ["a"]);
    assert_eq!(b.views().len(), 1);
    assert!(b.views()[0].in_call, "the call's session answers it");
    // The feed's copy: its facts win (the message id), nothing asks again.
    let mut fed = call("a", 7, 3);
    fed.thread_title = "Kai thread".into();
    b.set_open(vec![fed]);
    let out = b.change(ApprovalsChange::Added(call("a", 7, 3)), false);
    assert!(notified(&out).is_empty());
    let v = b.views();
    assert_eq!((v.len(), v[0].message_id, v[0].in_call), (1, 3, true));
    // The feed first, the session after: no second ask either.
    b.set_open(vec![call("a", 7, 3), call("b", 7, 3)]);
    assert_eq!(
        notified(&b.change(ApprovalsChange::Added(call("b", 7, 3)), false)),
        ["b"]
    );
    assert!(notified(&b.call_shown(shown("b", 1), false)).is_empty());
    // While a card is seen (the app, or the overlay unlocked) nothing notifies.
    assert!(notified(&b.call_shown(shown("c", 1), true)).is_empty());
}

#[test]
fn verdicts_go_through_the_call_only_when_its_session_showed_every_call_of_the_reply() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(4));
    b.call_shown(shown("a", 4), true);
    b.call_shown(shown("b", 4), true);
    let d = b.decide("a", true, true).unwrap();
    assert_eq!((d.send.is_none(), d.remaining), (true, 1));
    assert_eq!(b.views()[1].waiting, 0);
    assert_eq!(b.views()[0].waiting, 1, "a waits for b");
    let d = b.decide("b", false, true).unwrap();
    assert_eq!(d.via_call, Some(4));
    assert_eq!(
        d.send.unwrap(),
        [("a".to_owned(), true), ("b".to_owned(), false)]
    );
    assert!(b.views().iter().all(|v| v.sent));
    // A verdict on its way is not given again.
    assert!(
        b.decide("a", false, true)
            .unwrap_err()
            .contains("went to lmgw")
    );
    // The session took them: the cards go, and a feed set read before stays without them.
    b.verdicts_taken(&["a".into(), "b".into()]);
    b.set_open(vec![call("a", 7, 3), call("b", 7, 3)]);
    assert!(b.views().is_empty());
    assert!(b.waiting().is_empty());
    // The feed's withdrawal (or its next caught_up) ends the filter.
    b.change(
        ApprovalsChange::Withdrawn(vec!["a".into(), "b".into()]),
        true,
    );
    b.set_open(vec![]);
    assert!(b.views().is_empty());

    // A reply the session did not show (it waited before the bind) goes by the route.
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(4));
    b.set_open(vec![call("x", 7, 9)]);
    let d = b.decide("x", true, true).unwrap();
    assert_eq!((d.via_call, d.send.is_some()), (None, true));
    assert!(!b.views()[0].in_call);
}

#[test]
fn lmgw_s_word_on_the_call_closes_the_card_and_a_stale_set_does_not_bring_it_back() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(1));
    b.set_open(vec![call("a", 7, 3)]);
    assert_eq!(
        notified(&b.change(ApprovalsChange::Added(call("a", 7, 3)), false)),
        ["a"]
    );
    b.call_shown(shown("a", 1), false);
    let out = b.call_decided("a");
    assert_eq!(out.updates, [withdraw("a")]);
    assert!(b.views().is_empty());
    // The feed still holds it for a moment: no card, no notification.
    b.set_open(vec![call("a", 7, 3)]);
    assert!(b.views().is_empty());
    assert!(b.backgrounded().updates.is_empty());
    // caught_up hands it back to the feed.
    b.caught_up();
    assert_eq!(
        b.views().len(),
        1,
        "a set at caught_up that still has it shows it"
    );
}

#[test]
fn the_call_s_end_keeps_the_feed_s_cards_and_drops_those_only_its_session_showed() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(2));
    b.set_open(vec![call("a", 7, 3)]);
    b.call_shown(shown("a", 2), true);
    assert_eq!(notified(&b.call_shown(shown("z", 2), false)), ["z"]);
    let out = b.call_running(None);
    assert_eq!(out.updates, [withdraw("z")]);
    let v = b.views();
    assert_eq!(v.len(), 1);
    assert_eq!(
        (v[0].approval_request_id.as_str(), v[0].in_call),
        ("a", false)
    );
    assert!(b.shown_ids().is_empty());
    // The next call's verdicts for a go by the route: its session never showed it.
    b.call_running(Some(3));
    assert_eq!(b.decide("a", true, true).unwrap().via_call, None);
}

#[test]
fn a_verdict_lmgw_did_not_take_is_asked_again_with_its_words() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(1));
    b.call_shown(shown("a", 1), true);
    let d = b.decide("a", true, true).unwrap();
    assert_eq!(d.via_call, Some(1));
    let note = refusal_note(
        Some("approval_out_of_scope"),
        "phone may not run desktop__open_file",
    );
    let out = b.not_taken(&["a".into()], Some(note.clone()), true, false);
    assert_eq!(notified(&out), ["a"], "away: asked again");
    let v = &b.views()[0];
    assert_eq!((v.verdict, v.sent), (None, false));
    assert_eq!(v.note.as_deref(), Some(note.as_str()));
    assert!(note.contains("Decline it here"), "{note}");
    // A new verdict clears the note.
    b.decide("a", false, true).unwrap();
    assert_eq!(b.views()[0].note, None);
}

#[test]
fn what_waits_is_every_call_with_no_verdict_sent_by_thread() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(1));
    b.set_open(vec![call("a", 7, 3), call("c", 8, 1)]);
    b.call_shown(shown("a", 1), true);
    let tools = |b: &Approvals| {
        b.waiting()
            .into_iter()
            .map(|w| (w.thread_id, w.tool))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tools(&b),
        [
            (7, "desktop__open_file".to_owned()),
            (8, "desktop__open_file".to_owned())
        ]
    );
    // A verdict sent waits no more.
    b.decide("a", true, true).unwrap();
    assert_eq!(tools(&b), [(8, "desktop__open_file".to_owned())]);
}

/// The session's items name no message: within its thread, a call the session showed is of one
/// reply with the feed's calls the same session showed (lmgw keeps one reply waiting per thread),
/// and the card's "Kai speaks the answer" holds only when the session showed the whole reply.
#[test]
fn a_reply_the_feed_and_the_session_both_show_is_one_reply() {
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(1));
    b.call_shown(shown("a", 1), true);
    b.call_shown(shown("b", 1), true);
    // The feed's copy of a came (message 30); b's did not yet.
    b.set_open(vec![call("a", 7, 30)]);
    let d = b.decide("a", true, true).unwrap();
    assert_eq!((d.send.is_none(), d.remaining), (true, 1), "a waits for b");
    let views = b.views();
    assert!(views.iter().all(|v| v.in_call), "{views:?}");
    let d = b.decide("b", false, true).unwrap();
    assert_eq!(d.via_call, Some(1));
    assert_eq!(
        d.send.unwrap(),
        [("a".to_owned(), true), ("b".to_owned(), false)]
    );

    // A call of the reply the session did not show (the feed's c, of a's message): the reply goes
    // by the route, and no card of it promises the call's answer.
    let mut b = Approvals::new(vec![]);
    b.call_running(Some(1));
    b.call_shown(shown("a", 1), true);
    b.set_open(vec![call("a", 7, 30), call("c", 7, 30)]);
    assert!(b.views().iter().all(|v| !v.in_call));
    assert_eq!(b.decide("a", true, true).unwrap().remaining, 1);
    let d = b.decide("c", true, true).unwrap();
    assert_eq!((d.via_call, d.send.map(|s| s.len())), (None, Some(2)));
}

fn decided_as(id: &str, approve: bool, by: Option<&str>, not_run: bool) -> ApprovalDecided {
    ApprovalDecided {
        thread_id: 7,
        message_id: 30,
        approval_request_id: id.into(),
        approve,
        by: by.map(str::to_owned),
        not_run,
        call_id: None,
    }
}

/// The phone never closes a card on its own inference; the feed's approval.decided does, and when
/// lmgw decided otherwise than the verdict sent the owner is told why, once.
#[test]
fn lmgw_s_decision_other_than_the_verdict_sent_is_told_once() {
    const ME: &str = "device 'phone'";
    let sent = |id: &str, approve: bool| {
        let mut b = Approvals::new(vec![]);
        b.call_running(Some(1));
        b.call_shown(shown(id, 1), true);
        b.set_open(vec![call(id, 7, 30)]);
        b.decide(id, approve, true).unwrap();
        b
    };
    // As sent: nothing to say.
    let mut b = sent("a", true);
    assert_eq!(
        b.feed_decided(&decided_as("a", true, Some(ME), false), Some(ME)),
        None
    );
    // The owner's own words came first (approve while still talking): lmgw's decline by this phone.
    let mut b = sent("a", true);
    let words = b
        .feed_decided(&decided_as("a", false, Some(ME), false), Some(ME))
        .unwrap();
    assert!(
        words.starts_with("desktop__open_file did not run"),
        "{words}"
    );
    assert!(
        words.contains("what you said or typed reached lmgw before your approval"),
        "{words}"
    );
    assert_eq!(
        b.feed_decided(&decided_as("a", false, Some(ME), false), Some(ME)),
        None,
        "once"
    );
    // Another client first.
    let mut b = sent("a", true);
    let words = b
        .feed_decided(
            &decided_as("a", false, Some("the dashboard"), false),
            Some(ME),
        )
        .unwrap();
    assert!(words.contains("the dashboard declined it"), "{words}");
    let mut b = sent("a", false);
    let words = b
        .feed_decided(
            &decided_as("a", true, Some("the dashboard"), false),
            Some(ME),
        )
        .unwrap();
    assert!(
        words.contains("approved by the dashboard before your decline"),
        "{words}"
    );
    // Approved, and its turn could not start: said as lmgw's second approval.decided for it.
    let mut b = sent("a", true);
    assert_eq!(
        b.feed_decided(&decided_as("a", true, Some(ME), false), Some(ME)),
        None
    );
    let words = b
        .feed_decided(&decided_as("a", false, Some(ME), true), Some(ME))
        .unwrap();
    assert!(words.contains("never ran"), "{words}");
    // A route's "Already decided" said it: nothing more.
    let mut b = sent("a", true);
    b.told(&["a".into()]);
    assert_eq!(
        b.feed_decided(
            &decided_as("a", false, Some("the dashboard"), false),
            Some(ME)
        ),
        None
    );
    // Refused before deciding: no verdict of the phone's stands.
    let mut b = sent("a", true);
    b.not_taken(&["a".into()], None, true, true);
    assert_eq!(
        b.feed_decided(
            &decided_as("a", false, Some("the dashboard"), false),
            Some(ME)
        ),
        None
    );
    // A call that waits no more at a caught_up had its word, or never gets one.
    let mut b = sent("a", true);
    b.set_open(vec![]);
    b.caught_up();
    assert_eq!(
        b.feed_decided(&decided_as("a", false, Some(ME), false), Some(ME)),
        None
    );
    // Calls a never-taken verdict was not given for say nothing.
    let mut b = Approvals::new(vec![]);
    assert_eq!(
        b.feed_decided(&decided_as("z", false, Some(ME), false), Some(ME)),
        None
    );
}

#[test]
fn a_refusal_code_that_does_not_say_lmgw_refused_before_deciding_says_it_may_have_decided() {
    for code in [
        "approval_out_of_scope",
        "approval_missing",
        "approval_moved_on",
        "key_rate",
    ] {
        assert!(refused_before_deciding(Some(code)), "{code}");
    }
    for code in [
        "approval_decided",
        "gpu_hold",
        "internal",
        "upstream_timeout",
    ] {
        assert!(!refused_before_deciding(Some(code)), "{code}");
    }
    assert!(!refused_before_deciding(None));
    assert!(
        refusal_note(Some("gpu_hold"), "paused")
            .starts_with("The decision may not have reached lmgw")
    );
    assert!(
        refusal_note(Some("approval_missing"), "b too")
            .starts_with("lmgw did not take the verdict")
    );
}
