//! The Kai conversation by text (design §6, §15.3): the folder rule and `current`, the thread's
//! messages kept current by the feed, a text send streaming in, New conversation, a send while not
//! connected answered at once.

use kai_mobile_core::{
    CoreError,
    views::{ConnectionState, ConversationStatus, Role, Update},
};
use serde_json::json;

use super::{
    mock::{Mock, detail, message},
    pki::{Clients, PKI},
    rig::{Rig, caught_up, hello, reply, stored},
};

async fn ready(rig: &mut Rig) -> kai_mobile_core::views::ConversationView {
    match rig
        .next("ready", |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::Ready))
        .await
    {
        Update::Conversation { view } => view,
        _ => unreachable!(),
    }
}

/// The folder found by the rule, `current` asked, the thread's messages shown with their Markdown;
/// a `message.added` of the thread joins them; the remembered folder is kept with the pairing.
#[tokio::test]
async fn the_conversation_loads_and_follows_the_feed() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(
        5,
        7,
        vec![
            message(1, 7, "user", "hi"),
            message(2, 7, "assistant", "**Hello**"),
        ],
    );
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    let reset = rig.next("messages", |u| matches!(u, Update::Messages { delta } if delta.reset && delta.thread_id == Some(7))).await;
    let view = ready(&mut rig).await;
    assert_eq!((view.folder_id, view.thread_id), (Some(5), Some(7)));
    let Update::Messages { delta } = reset else {
        unreachable!()
    };
    assert_eq!(delta.upsert.len(), 2);
    assert_eq!(delta.upsert[1].role, Role::Assistant);
    assert!(delta.upsert[1].blocks[0].spans[0].strong);
    assert_eq!(rig.link_file().unwrap().folder_id, Some(5));
    feed.send(&stored(
        "e1:5:bb",
        "message.added",
        reply(7, 3, "It is late."),
    ));
    let up = rig.next("added", |u| matches!(u, Update::Messages { delta } if !delta.reset && delta.upsert.iter().any(|m| m.id == 3))).await;
    let Update::Messages { delta } = up else {
        unreachable!()
    };
    assert_eq!(delta.upsert[0].text, "It is late.");
    // The folder's current thread moves: the conversation reads the new one.
    mock.state.lock().unwrap().details.insert(
        8,
        detail(8, "Kai 2", vec![message(9, 8, "assistant", "fresh")]),
    );
    feed.send(&stored("e1:6:cc", "folder.current", json!({"folder_id": 5, "thread_id": 8, "previous_thread_id": 7, "reason": "idle", "by": null})));
    rig.next(
        "moved",
        |u| matches!(u, Update::Messages { delta } if delta.reset && delta.thread_id == Some(8)),
    )
    .await;
}

/// A folder of the name that does not exist is created plain, and the owner's steps are shown.
#[tokio::test]
async fn a_missing_folder_is_created_and_waits_for_the_owner() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    let u = rig
        .next("not ready", |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::NotReady))
        .await;
    let Update::Conversation { view } = u else {
        unreachable!()
    };
    assert!(view.steps.unwrap().contains("has no model yet"));
    assert_eq!(
        mock.requests("/chat/api/folders")
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    let err = rig.core.send_text("hi".into()).await.unwrap_err();
    assert!(matches!(err, CoreError::NotReady { .. }), "{err:?}");
}

/// A text send streams into the conversation and its `done` names the saved reply.
#[tokio::test]
async fn a_text_send_streams_in() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![]);
    mock.state.lock().unwrap().send_sse = "event: turn\ndata: {\"user_message_id\":10}\n\n\
        event: delta\ndata: {\"text\":\"Hel\"}\n\n\
        event: delta\ndata: {\"text\":\"lo\"}\n\n\
        event: done\ndata: {\"aborted\":false,\"message_id\":11,\"saved\":true,\"model\":\"gemma\",\"answered_by\":\"cloud\",\"prompt_tokens\":null,\"completion_tokens\":null,\"ttfb_ms\":null,\"total_ms\":5,\"timings\":null,\"reasoning_ignored\":[],\"reasoning_note\":null}\n\n"
        .into();
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    ready(&mut rig).await;
    rig.core.send_text("hi there".into()).await.unwrap();
    let streamed = rig.next("streaming", |u| matches!(u, Update::Messages { delta } if delta.upsert.iter().any(|m| m.streaming && m.text == "Hello"))).await;
    drop(streamed);
    let done = rig
        .next(
            "saved",
            |u| matches!(u, Update::Messages { delta } if delta.upsert.iter().any(|m| m.id == 11)),
        )
        .await;
    let Update::Messages { delta } = done else {
        unreachable!()
    };
    assert_eq!(delta.removed, [-1]);
    assert_eq!(delta.upsert[0].answered_by.as_deref(), Some("cloud"));
    let body: serde_json::Value =
        serde_json::from_slice(&mock.requests("/chat/api/threads/7/send")[0].body).unwrap();
    assert_eq!(body["content"], "hi there");
}

/// A label's `error` frame is a notice: the turn answers past it, and the line stays on the saved
/// reply after `done` (it is not an error: the reply has none).
#[tokio::test]
async fn a_label_notice_stays_on_the_saved_reply() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![]);
    mock.state.lock().unwrap().send_sse = "event: turn\ndata: {\"user_message_id\":10}\n\n\
        event: error\ndata: {\"message\":\"MCP server 'kai': device 'Kai desktop' is offline (never seen)\",\"code\":\"tool_offline\",\"server_label\":\"kai\"}\n\n\
        event: delta\ndata: {\"text\":\"Plain answer\"}\n\n\
        event: done\ndata: {\"aborted\":false,\"message_id\":11,\"saved\":true,\"model\":\"gemma\",\"answered_by\":null,\"prompt_tokens\":null,\"completion_tokens\":null,\"ttfb_ms\":null,\"total_ms\":5,\"timings\":null,\"reasoning_ignored\":[],\"reasoning_note\":null}\n\n"
        .into();
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    ready(&mut rig).await;
    rig.core.send_text("hi".into()).await.unwrap();
    let done = rig
        .next(
            "saved",
            |u| matches!(u, Update::Messages { delta } if delta.upsert.iter().any(|m| m.id == 11)),
        )
        .await;
    let Update::Messages { delta } = done else {
        unreachable!()
    };
    let reply = &delta.upsert[0];
    assert_eq!(reply.text, "Plain answer");
    assert_eq!(reply.error, None);
    assert_eq!(
        reply.notices,
        ["Kai desktop is offline. Answered without its tools."]
    );
}

/// New conversation asks `current` with `{new: true}` and shows the new thread.
#[tokio::test]
async fn new_conversation_moves_to_a_new_thread() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![message(1, 7, "user", "old")]);
    {
        let mut s = mock.state.lock().unwrap();
        s.details.insert(8, detail(8, "Kai", vec![]));
        s.new_thread = Some(8);
    }
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    ready(&mut rig).await;
    rig.core.new_conversation().await.unwrap();
    rig.next(
        "new thread",
        |u| matches!(u, Update::Messages { delta } if delta.reset && delta.thread_id == Some(8)),
    )
    .await;
    let current = mock.requests("/chat/api/folders/5/current");
    assert!(String::from_utf8_lossy(&current.last().unwrap().body).contains("\"new\":true"));
}

/// The screen empties the moment New conversation is chosen, before the new thread is known; a
/// late `message.added` of the old thread stays out of the new view.
#[tokio::test]
async fn new_conversation_empties_the_view_first_and_drops_the_old_threads_events() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![message(1, 7, "user", "old")]);
    {
        let mut s = mock.state.lock().unwrap();
        s.details.insert(8, detail(8, "Kai", vec![]));
        s.new_thread = Some(8);
    }
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    ready(&mut rig).await;
    rig.core.new_conversation().await.unwrap();
    let first = rig
        .next("the empty view", |u| matches!(u, Update::Messages { .. }))
        .await;
    let Update::Messages { delta } = first else {
        unreachable!()
    };
    assert!(delta.reset && delta.thread_id.is_none() && delta.upsert.is_empty());
    rig.next(
        "new thread",
        |u| matches!(u, Update::Messages { delta } if delta.reset && delta.thread_id == Some(8)),
    )
    .await;
    feed.send(&stored(
        "e1:5:bb",
        "message.added",
        reply(7, 20, "late old"),
    ));
    feed.send(&stored("e1:6:cc", "message.added", reply(8, 21, "new")));
    let up = rig
        .next(
            "added",
            |u| matches!(u, Update::Messages { delta } if !delta.upsert.is_empty()),
        )
        .await;
    let Update::Messages { delta } = up else {
        unreachable!()
    };
    assert_eq!(delta.upsert.iter().map(|m| m.id).collect::<Vec<_>>(), [21]);
}

/// A send while not connected answers at once with the state's words (§5.3).
#[tokio::test]
async fn a_send_while_not_connected_answers_at_once() {
    let rig = Rig::unpaired().await;
    let err = rig.core.send_text("hi".into()).await.unwrap_err();
    let CoreError::NotConnected { reason } = err else {
        panic!("{err:?}")
    };
    assert_eq!(reason, "Not paired with lmgw");
    let mock = Mock::start(&PKI, Clients::Required).await;
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    rig.connection("connecting", |v| v.state == ConnectionState::Connecting)
        .await;
    assert!(matches!(
        rig.core.send_text("hi".into()).await,
        Err(CoreError::NotConnected { .. })
    ));
}
