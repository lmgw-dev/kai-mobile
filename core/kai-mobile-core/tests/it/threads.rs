//! The threads browser against the scripted lmgw (design §6.4): the lists while the browser shows,
//! another thread opened beside the Kai conversation and kept current by the feed, a text send
//! into it, the actions (a refusal verbatim with its code), a create in a folder, the search.

use kai_mobile_core::{
    CoreError,
    lmgw::requests::ThreadRow,
    threads::view::{BrowserStatus, OpenedStatus, SearchIn},
    views::{ConnectionState, ConversationStatus, Update},
};
use serde_json::json;

use super::{
    mock::{Mock, detail_of, message, ready_folder},
    pki::{Clients, PKI},
    rig::{Rig, caught_up, hello, reply, stored},
};

const SSE: &str = "event: turn\ndata: {\"user_message_id\":30}\n\n\
    event: delta\ndata: {\"text\":\"Soup\"}\n\n\
    event: done\ndata: {\"aborted\":false,\"message_id\":31,\"saved\":true,\"model\":\"gemma\",\"answered_by\":null,\"prompt_tokens\":null,\"completion_tokens\":null,\"ttfb_ms\":null,\"total_ms\":5,\"timings\":null,\"reasoning_ignored\":[],\"reasoning_note\":null}\n\n";

/// The Kai folder (5) with its thread 7, a folder "Notes" (2) with a model and its thread 20, a
/// folder "Drafts" (3) without a model, a temporary thread.
fn mock_state(mock: &Mock) {
    mock.with_conversation(5, 7, vec![message(1, 7, "user", "hi")]);
    let mut s = mock.state.lock().unwrap();
    let mut notes = ready_folder(2, "Notes", 0);
    notes.ongoing = None;
    let mut drafts = ready_folder(3, "Drafts", 0);
    drafts.ongoing = None;
    drafts.defaults.model_alias = None;
    s.folders.extend([notes, drafts]);
    let recipes = ThreadRow {
        id: 20,
        title: "Recipes".into(),
        folder_id: Some(2),
        ..Default::default()
    };
    s.threads.push(recipes.clone());
    s.details.insert(
        20,
        detail_of(recipes, vec![message(21, 20, "assistant", "**Tea**")]),
    );
    s.threads.push(ThreadRow {
        id: -3,
        title: "Scratch".into(),
        temporary: true,
        ..Default::default()
    });
    s.send_sse = SSE.into();
}

async fn connected(mock: &Mock) -> (Rig, super::mock::Feed) {
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    rig.next(
        "ready",
        |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::Ready),
    )
    .await;
    // The app is on screen: the lists are read only then.
    rig.core.set_foreground(true);
    (rig, feed)
}

#[tokio::test]
async fn the_browser_lists_by_folder_and_an_opened_thread_follows_the_feed() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock_state(&mock);
    let (mut rig, feed) = connected(&mock).await;
    rig.core.browser(true);
    let u = rig
        .next(
            "browser",
            |u| matches!(u, Update::Browser { view } if view.status == BrowserStatus::Ready),
        )
        .await;
    let Update::Browser { view } = u else {
        unreachable!()
    };
    let kai = view
        .sections
        .iter()
        .find(|s| s.folder_id == Some(5))
        .unwrap();
    assert!(kai.kai && kai.threads[0].kai);
    let drafts = view
        .sections
        .iter()
        .find(|s| s.folder_id == Some(3))
        .unwrap();
    assert!(!drafts.can_create && drafts.steps.is_some());
    // The temporary thread is in the list without a folder (the mock lists it there).
    assert!(
        view.sections
            .iter()
            .any(|s| s.threads.iter().any(|t| t.id == -3 && t.temporary))
    );

    rig.core.open_thread(20);
    let u = rig
        .next("opened", |u| matches!(u, Update::OpenedMessages { delta } if delta.reset && !delta.upsert.is_empty()))
        .await;
    let Update::OpenedMessages { delta } = u else {
        unreachable!()
    };
    assert_eq!((delta.thread_id, delta.upsert[0].id), (Some(20), 21));
    rig.next("opened ready", |u| matches!(u, Update::Opened { view: Some(v) } if v.status == OpenedStatus::Ready && v.title == "Recipes" && !v.kai)).await;

    // A message in it joins it; the Kai conversation does not get it, and the browser reads again.
    let reads = mock
        .requests("/chat/api/threads")
        .iter()
        .filter(|r| r.path == "/chat/api/threads")
        .count();
    feed.send(&stored(
        "e1:5:bb",
        "message.added",
        reply(20, 22, "More tea."),
    ));
    rig.next("added", |u| matches!(u, Update::OpenedMessages { delta } if delta.upsert.iter().any(|m| m.id == 22))).await;
    assert!(!rig.seen.iter().any(
        |u| matches!(u, Update::Messages { delta } if delta.upsert.iter().any(|m| m.id == 22))
    ));
    rig.next("browser again", |u| matches!(u, Update::Browser { .. }))
        .await;
    let again = mock
        .requests("/chat/api/threads")
        .iter()
        .filter(|r| r.path == "/chat/api/threads")
        .count();
    assert!(again > reads, "the browser read its lists again");

    // A send streams into the opened thread.
    rig.core
        .send_in_thread(20, "and soup?".into())
        .await
        .unwrap();
    assert_eq!(mock.requests("/chat/api/threads/20/send").len(), 1);
    rig.next("saved", |u| matches!(u, Update::OpenedMessages { delta } if delta.upsert.iter().any(|m| m.id == 31 && m.text == "Soup"))).await;
    // A send to a thread that is not the opened one is refused without a request.
    let e = rig.core.send_in_thread(7, "x".into()).await.unwrap_err();
    assert!(matches!(e, CoreError::NotReady { .. }), "{e:?}");

    // The thread is deleted elsewhere: a read again shows it gone.
    mock.state.lock().unwrap().details.remove(&20);
    feed.send(&stored(
        "e1:6:cc",
        "thread.deleted",
        json!({"thread_id": 20, "deleted": true, "by": null}),
    ));
    rig.next(
        "gone",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.status == OpenedStatus::Gone),
    )
    .await;
    rig.core.close_thread();
    rig.next("closed", |u| matches!(u, Update::Opened { view: None }))
        .await;
}

#[tokio::test]
async fn actions_create_and_search_go_to_lmgw_and_a_refusal_keeps_its_code() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock_state(&mock);
    let (mut rig, _feed) = connected(&mock).await;
    rig.core.open_thread(20);
    rig.next(
        "opened",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.status == OpenedStatus::Ready),
    )
    .await;

    rig.core.pin_thread(20, true).await.unwrap();
    let pin = &mock.requests("/chat/api/threads/20/pin")[0];
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&pin.body).unwrap(),
        json!({"pinned": true})
    );
    rig.next(
        "pinned",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.pinned),
    )
    .await;
    rig.core.move_thread(20, None).await.unwrap();
    rig.next(
        "moved",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.folder_id.is_none()),
    )
    .await;
    rig.core.rename_thread(20, " Soups ".into()).await.unwrap();
    rig.next(
        "renamed",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.title == "Soups"),
    )
    .await;
    let e = rig.core.rename_thread(20, "  ".into()).await.unwrap_err();
    assert!(
        matches!(&e, CoreError::Refused { code, .. } if code == "bad_request"),
        "{e:?}"
    );
    rig.core.archive_thread(20, true).await.unwrap();
    rig.next(
        "archived",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.archived),
    )
    .await;

    let e = rig.core.pin_thread(-3, true).await.unwrap_err();
    match e {
        CoreError::Refused { code, .. } => assert_eq!(code, "temporary_thread"),
        e => panic!("{e:?}"),
    }

    // A folder without a model: the dashboard's steps, no create.
    let e = rig.core.new_thread(3).await.unwrap_err();
    assert!(
        matches!(&e, CoreError::NotReady { reason } if reason.contains("Folder settings")),
        "{e:?}"
    );
    assert!(
        mock.requests("/chat/api/threads")
            .iter()
            .all(|r| r.method != "POST" || r.path != "/chat/api/threads")
    );
    // A folder with one: created with the folder's model, and opened.
    let id = rig.core.new_thread(2).await.unwrap();
    // Creating does not open it (the app selects, then opens, once).
    assert!(mock.requests(&format!("/chat/api/threads/{id}")).is_empty());
    rig.core.open_thread(id);
    let create = mock
        .requests("/chat/api/threads")
        .into_iter()
        .find(|r| r.method == "POST" && r.path == "/chat/api/threads")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&create.body).unwrap();
    assert_eq!(
        (body["model_alias"].as_str(), body["folder_id"].as_i64()),
        (Some("gemma"), Some(2))
    );
    rig.next("new opened", |u| matches!(u, Update::Opened { view: Some(v) } if v.thread_id == id && v.status == OpenedStatus::Ready)).await;
    rig.core.delete_thread(id).await.unwrap();
    rig.next("deleted", |u| matches!(u, Update::Opened { view: Some(v) } if v.thread_id == id && v.status == OpenedStatus::Gone)).await;

    // The search: under two characters no request; a page with its marks split.
    let e = rig
        .core
        .search("a".into(), SearchIn::All, None, None)
        .await
        .unwrap_err();
    assert!(matches!(e, CoreError::NotReady { .. }));
    assert!(mock.requests("/chat/api/search").is_empty());
    mock.state.lock().unwrap().search = Some(
        r#"{"total_threads":1,"threads":[{"thread_id":7,"title":"Kai","folder_id":5,"archived":false,"updated_at":"x","match_count":1,"hits":[{"kind":"m","message_id":1,"role":"user","snippet":"say \ue000hi\ue001"}]}],"next_offset":20}"#
            .into(),
    );
    let page = rig
        .core
        .search("hi there".into(), SearchIn::All, Some(5), Some(0))
        .await
        .unwrap();
    assert_eq!(
        mock.requests("/chat/api/search")[0].path,
        "/chat/api/search?q=hi%20there&archived=all&folder=5&offset=0"
    );
    assert_eq!(page.next_offset, Some(20));
    assert!(page.threads[0].kai);
    assert!(page.threads[0].hits[0].parts[1].hit);
}

/// A key refusal of a list read ends that read: after Try again the lists are read again (the read
/// was once left in flight for good).
#[tokio::test]
async fn a_key_refusal_of_a_list_read_does_not_stop_the_lists_for_good() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock_state(&mock);
    let (mut rig, _feed) = connected(&mock).await;
    mock.state.lock().unwrap().threads_answer = Some((
        401,
        r#"{"code":"device_disabled","message":"device 'phone' is disabled"}"#.into(),
    ));
    rig.core.browser(true);
    rig.connection("refused", |v| v.state == ConnectionState::KeyRefused)
        .await;
    mock.state.lock().unwrap().threads_answer = None;
    rig.core.retry();
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    rig.next(
        "lists again",
        |u| matches!(u, Update::Browser { view } if view.status == BrowserStatus::Ready),
    )
    .await;
}

/// The back stack still shows the browser while the app is off screen: nothing is read then, and
/// the app becoming visible reads.
#[tokio::test]
async fn the_lists_are_not_read_while_the_app_is_off_screen() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock_state(&mock);
    let (mut rig, _feed) = connected(&mock).await;
    rig.core.set_foreground(false);
    rig.core.browser(true);
    rig.core.browser_archived(true);
    rig.next("loading", |u| matches!(u, Update::Browser { .. }))
        .await;
    rig.next("loading again", |u| matches!(u, Update::Browser { .. }))
        .await;
    let archived = |m: &Mock| {
        m.requests("/chat/api/threads")
            .iter()
            .filter(|r| r.path.contains("archived"))
            .count()
    };
    assert_eq!(archived(&mock), 0, "no read in the background");
    rig.core.set_foreground(true);
    rig.next(
        "read when visible",
        |u| matches!(u, Update::Browser { view } if view.status == BrowserStatus::Ready),
    )
    .await;
    assert!(archived(&mock) > 0);
}

/// The Kai thread is the conversation's: opened after the conversation knows it, or before, it is
/// marked `kai`, reads nothing and takes no send.
#[tokio::test]
async fn the_kai_thread_never_opens_as_a_second_copy() {
    // After: the conversation is ready.
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock_state(&mock);
    let (mut rig, _feed) = connected(&mock).await;
    let reads = mock.requests("/chat/api/threads/7").len();
    rig.core.open_thread(7);
    rig.next(
        "marked",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.thread_id == 7 && v.kai),
    )
    .await;
    let e = rig.core.send_in_thread(7, "x".into()).await.unwrap_err();
    assert!(matches!(e, CoreError::NotReady { .. }), "{e:?}");
    assert_eq!(
        mock.requests("/chat/api/threads/7").len(),
        reads,
        "not read"
    );
    assert!(mock.requests("/chat/api/threads/7/send").is_empty());

    // Before: opened while the conversation is still loading.
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock_state(&mock);
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    rig.core.open_thread(7);
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    rig.next(
        "marked once known",
        |u| matches!(u, Update::Opened { view: Some(v) } if v.thread_id == 7 && v.kai),
    )
    .await;
    let e = rig.core.send_in_thread(7, "x".into()).await.unwrap_err();
    assert!(matches!(e, CoreError::NotReady { .. }), "{e:?}");
    assert!(mock.requests("/chat/api/threads/7/send").is_empty());
}
