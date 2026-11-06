//! The browser's lists and the opened thread, sans-IO (design §6.4).

use lmgw_client::{
    feed::{FeedEvent, ThreadNow},
    requests::{
        Folder, Message, SearchHit, SearchPage, SearchThread, ThreadDetail, ThreadList, ThreadRow,
    },
    types::{
        chat::Thread,
        chat_feed::{MessagesChanged, ThreadChanged, ThreadGone, TurnStarted},
    },
};

use super::{
    Opened,
    browser::{Browser, Lists, no_model_steps, search_view, searchable, snippet},
    view::{BrowserStatus, HitKind, OpenedStatus, SnippetPart},
};
use crate::conversation::{Convo, send::Frame};

fn folder(id: i64, name: &str, model: Option<&str>) -> Folder {
    let mut f = Folder {
        id,
        name: name.into(),
        ..Default::default()
    };
    f.defaults.model_alias = model.map(Into::into);
    f
}

fn row(id: i64, title: &str, folder: Option<i64>, pinned: bool) -> ThreadRow {
    ThreadRow {
        id,
        title: title.into(),
        folder_id: folder,
        pinned,
        updated_at: "2026-10-19 08:00:00".into(),
        ..Default::default()
    }
}

fn lists() -> Lists {
    Lists {
        active: ThreadList {
            // lmgw's order: pinned first, then the most recently active.
            threads: vec![
                row(4, "Pinned note", Some(2), true),
                row(9, "Kai now", Some(1), false),
                row(3, "Loose", None, false),
                row(5, "Recipes", Some(2), false),
                row(6, "In a hidden folder", None, false),
            ],
            archived_count: 2,
            temporary: vec![ThreadRow {
                id: -1,
                title: "Scratch".into(),
                temporary: true,
                ..Default::default()
            }],
            folders: vec![
                folder(1, "Kai", Some("gemma")),
                folder(2, "Notes", None),
                folder(7, "Empty", Some("cloud/model")),
            ],
        },
        archived: Some(vec![ThreadRow {
            archived_at: Some("2026-10-18 10:00:00".into()),
            ..row(8, "Old", Some(2), false)
        }]),
    }
}

#[test]
fn the_browser_groups_by_folder_in_lmgws_order_with_the_loose_threads_last() {
    let mut b = Browser::default();
    assert!(b.show(true));
    let g = b.want().unwrap();
    assert_eq!(b.read(g, Ok(lists())), (true, false));
    let v = b.view(Some(1), Some(9));
    assert_eq!(v.status, BrowserStatus::Ready);
    let names: Vec<_> = v
        .sections
        .iter()
        .map(|s| (s.folder_id, s.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            (Some(1), "Kai"),
            (Some(2), "Notes"),
            (Some(7), "Empty"),
            (None, "")
        ]
    );
    let notes = &v.sections[1];
    assert_eq!(
        notes.threads.iter().map(|t| t.id).collect::<Vec<_>>(),
        [4, 5],
        "pinned first, as lmgw listed them"
    );
    assert!(notes.threads[0].pinned);
    assert!(v.sections[0].kai && v.sections[0].threads[0].kai);
    assert_eq!(v.sections[0].threads[0].day, "2026-10-19");
    // A folder without a model offers no New thread, and says what to do on the dashboard.
    assert!(!notes.can_create);
    assert_eq!(
        notes.steps.as_deref(),
        Some(no_model_steps("Notes").as_str())
    );
    assert!(v.sections[2].can_create && v.sections[2].threads.is_empty());
    assert!(!v.sections[3].can_create);
    assert_eq!(
        v.sections[3]
            .threads
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        [3, 6]
    );
    assert!(v.temporary[0].temporary);
    assert_eq!(v.archived_count, 2);
    // The archived list is kept only while its section is open.
    assert!(v.archived.is_none());
    assert_eq!(v.folders.len(), 3);
}

#[test]
fn the_archived_section_reads_its_list_only_while_open() {
    let mut b = Browser::default();
    assert!(!b.show_archived(true), "not shown: nothing to read");
    b.show_archived(false);
    b.show(true);
    let g = b.want().unwrap();
    b.read(g, Ok(lists()));
    assert!(b.show_archived(true));
    let g = b.want().unwrap();
    b.read(g, Ok(lists()));
    let v = b.view(None, None);
    assert!(v.archived.as_ref().unwrap()[0].archived);
    b.show_archived(false);
    assert!(b.view(None, None).archived.is_none());
}

#[test]
fn reads_are_coalesced_and_a_closed_browser_drops_its_lists() {
    let mut b = Browser::default();
    assert!(b.want().is_none(), "not shown: no read");
    b.show(true);
    let g1 = b.want().unwrap();
    // Events while the read runs: one more read after it, not one each.
    assert!(b.want().is_none());
    assert!(b.want().is_none());
    assert_eq!(b.read(g1, Ok(lists())), (true, true));
    let g2 = b.want().unwrap();
    assert_eq!(b.read(g2, Err("lmgw did not answer".into())), (true, false));
    let v = b.view(None, None);
    assert_eq!(v.status, BrowserStatus::Failed);
    assert_eq!(v.sections.len(), 4, "the last good lists stay");
    b.show(false);
    // A read of the closed browser's is dropped.
    assert_eq!(b.read(g2, Ok(lists())), (false, false));
    b.show(true);
    assert!(b.view(None, None).sections.is_empty());
}

#[test]
fn an_abandoned_read_frees_the_next_and_a_deferred_one_waits_for_the_next_want() {
    let mut b = Browser::default();
    b.show(true);
    let g1 = b.want().unwrap();
    assert!(b.want().is_none());
    // lmgw refused the read and no answer is shown: the next trigger reads again.
    b.abandon(g1);
    assert!(b.want().is_some(), "not stuck in flight");
    // An older generation's abandon changes nothing.
    b.abandon(g1);
    assert!(b.want().is_none());
    b.show(false);
    b.show(true);
    // Deferred (the app is off screen): the want that follows reads.
    b.defer();
    assert!(b.want().is_some());
}

#[test]
fn searches_need_two_characters_and_snippets_mark_their_matches() {
    assert!(!searchable(" a "));
    assert!(searchable("ab"));
    assert!(searchable("öl"));
    let part = |text: &str, hit| SnippetPart {
        text: text.into(),
        hit,
    };
    assert_eq!(
        snippet("before \u{E000}tea\u{E001} after \u{E000}open"),
        [
            part("before ", false),
            part("tea", true),
            part(" after ", false),
            part("open", true),
        ]
    );
    // Markup in a snippet is text.
    assert_eq!(snippet("<b>x</b>")[0].text, "<b>x</b>");
    let page = SearchPage {
        total_threads: 3,
        next_offset: Some(20),
        threads: vec![SearchThread {
            thread_id: 9,
            title: "Kai now".into(),
            match_count: 2,
            hits: vec![
                SearchHit {
                    kind: "m".into(),
                    message_id: Some(41),
                    snippet: "\u{E000}tea\u{E001}".into(),
                    ..Default::default()
                },
                SearchHit {
                    kind: "z".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    };
    let v = search_view(page, Some(9));
    assert_eq!((v.total, v.next_offset), (3, Some(20)));
    assert!(v.threads[0].kai);
    assert_eq!(v.threads[0].hits[0].kind, HitKind::Message);
    assert_eq!(v.threads[0].hits[0].message_id, Some(41));
    assert_eq!(v.threads[0].hits[1].kind, HitKind::Other);
}

fn message(id: i64, thread: i64, role: &str, text: &str) -> Message {
    Message {
        id,
        thread_id: thread,
        role: role.into(),
        content: text.into(),
        ..Default::default()
    }
}

fn detail(thread: i64, title: &str, messages: Vec<Message>) -> ThreadDetail {
    ThreadDetail {
        thread: Thread {
            row: row(thread, title, Some(2), false),
            ..Default::default()
        },
        messages,
        ..Default::default()
    }
}

fn opened() -> Opened {
    let mut o = Opened::new(1, 5, &Convo::default());
    o.loaded(&detail(5, "Recipes", vec![message(1, 5, "user", "hi")]));
    o
}

fn changed(r: ThreadRow) -> FeedEvent {
    FeedEvent::ThreadUpdated(ThreadNow::Row(ThreadChanged {
        thread: r,
        by: None,
    }))
}

#[test]
fn an_opened_thread_follows_its_own_events_only() {
    let mut o = Opened::new(1, 5, &Convo::default());
    assert_eq!(o.view(None).status, OpenedStatus::Loading);
    let d = o.loaded(&detail(5, "Recipes", vec![message(1, 5, "user", "hi")]));
    assert!(d.reset && d.thread_id == Some(5) && d.upsert.len() == 1);
    let v = o.view(Some(9));
    assert_eq!(
        (v.status, v.title.as_str(), v.kai),
        (OpenedStatus::Ready, "Recipes", false)
    );
    let added = |thread| {
        FeedEvent::MessageAdded(MessagesChanged {
            thread_id: thread,
            messages: vec![message(2, thread, "assistant", "**soup**")],
            ..Default::default()
        })
    };
    assert!(
        o.event(&added(9)).delta.is_none(),
        "the Kai thread's message stays out"
    );
    let asks = o.event(&added(5));
    assert_eq!(asks.delta.unwrap().upsert[0].id, 2);
    // Another principal's turn in it shows.
    let asks = o.event(&FeedEvent::TurnStarted(TurnStarted {
        thread_id: 5,
        by: "device 'desktop'".into(),
        ..Default::default()
    }));
    assert!(asks.view);
    assert_eq!(o.view(None).answering, ["device 'desktop'"]);
}

#[test]
fn a_thread_update_changes_the_view_and_an_archive_or_a_delete_reads_again() {
    let mut o = opened();
    let asks = o.event(&changed(row(5, "Soups", Some(7), true)));
    assert!(asks.view && !asks.read);
    let v = o.view(None);
    assert_eq!(
        (v.title.as_str(), v.folder_id, v.pinned),
        ("Soups", Some(7), true)
    );
    let mut archived = row(5, "Soups", Some(7), true);
    archived.archived_at = Some("2026-10-19 09:00:00".into());
    let asks = o.event(&changed(archived));
    assert!(asks.read && o.view(None).archived);
    let asks = o.event(&FeedEvent::ThreadDeleted(ThreadGone {
        thread_id: 5,
        deleted: true,
        by: None,
    }));
    assert!(asks.read);
    // Another thread's update leaves it alone.
    assert_eq!(
        o.event(&changed(row(6, "Other", None, false))),
        Default::default()
    );
    o.failed(true, "no thread (not_found)".into());
    let v = o.view(None);
    assert_eq!(v.status, OpenedStatus::Gone);
    assert!(
        v.note
            .unwrap()
            .contains("no longer within this device's reach")
    );
}

#[test]
fn a_failed_reread_keeps_what_the_thread_showed() {
    let mut o = opened();
    o.failed(
        false,
        "lmgw did not answer within 10 s (Settings → Advanced connection → Wait for lmgw)".into(),
    );
    let v = o.view(None);
    assert_eq!(v.status, OpenedStatus::Ready);
    assert!(v.note.unwrap().contains("Wait for lmgw"));
    let mut fresh = Opened::new(2, 5, &Convo::default());
    fresh.failed(false, "x".into());
    assert_eq!(fresh.view(None).status, OpenedStatus::Failed);
}

#[test]
fn a_send_streams_into_the_opened_thread_and_an_end_without_done_keeps_why() {
    let mut o = opened();
    o.convo.streaming = Some(crate::conversation::Streaming {
        thread_id: 5,
        ..Default::default()
    });
    assert!(o.view(None).sending);
    let (d, done) = o.convo.frame(Frame::Delta("Hel".into())).unwrap();
    assert!(!done && d.thread_id == Some(5) && d.upsert[0].streaming);
    let d = o
        .convo
        .stream_ended("the reply broke off: reset".into())
        .unwrap();
    assert_eq!(
        d.upsert[0].error.as_deref(),
        Some("the reply broke off: reset")
    );
    assert!(!d.upsert[0].streaming && !o.view(None).sending);
    assert!(o.convo.stream_ended("again".into()).is_none());
}

#[test]
fn an_action_answer_and_a_delete_by_the_phone_change_the_view() {
    let mut o = opened();
    o.changed(row(5, "Renamed", Some(2), true));
    o.changed(row(6, "Other", None, false));
    let v = o.view(Some(5));
    assert_eq!((v.title.as_str(), v.pinned, v.kai), ("Renamed", true, true));
    o.deleted();
    assert_eq!(o.view(None).status, OpenedStatus::Gone);
}

#[test]
fn opened_reads_are_coalesced_and_a_delta_during_a_read_owes_another() {
    let mut o = Opened::new(1, 5, &Convo::default());
    assert!(o.want());
    assert!(!o.want(), "one read in flight");
    assert!(o.read_ended(), "the trigger that came meanwhile is owed");
    assert!(o.want());
    assert!(!o.read_ended());
    // A delta applied while a read runs: that read may predate it.
    assert!(o.want());
    o.delta_applied();
    assert!(o.read_ended());
}

#[test]
fn the_kai_thread_opens_as_marked_and_reads_nothing() {
    let mut o = Opened::new(1, 7, &Convo::default());
    assert!(!o.is_kai(None) && !o.is_kai(Some(9)));
    assert!(!o.view(None).kai);
    assert!(o.is_kai(Some(7)));
    assert!(!o.is_kai(Some(7)), "marked once");
    assert!(o.view(None).kai);
    assert!(!o.want(), "the conversation shows it");
    // Marked, it stays marked when the conversation moves on.
    assert!(o.view(Some(8)).kai);
}

#[test]
fn an_own_archive_or_restore_reads_the_thread_again() {
    let mut o = opened();
    assert!(!o.changed(row(5, "Recipes", None, true)), "a pin does not");
    let mut archived = row(5, "Recipes", None, true);
    archived.archived_at = Some("2026-10-19 09:00:00".into());
    assert!(o.changed(archived));
    assert!(
        o.changed(row(5, "Recipes", None, true)),
        "nor does a restore"
    );
    assert!(!o.changed(row(6, "Other", None, false)));
}
