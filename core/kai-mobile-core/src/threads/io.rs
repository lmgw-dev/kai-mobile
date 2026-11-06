//! The browser's requests (design §6.4), each one over the paired link with lmgw-client's builders
//! and readers: the lists, a search page, a create in a folder and the actions on a thread.

use std::time::Duration;

use lmgw_client::requests::{self, SearchPage, SearchScope, Thread, ThreadRow};

use super::{
    browser::{Lists, folder_model},
    view::SearchIn,
};
use crate::{
    conversation::load::{Fail, get},
    link::http::Handle,
};

/// The active list (with the folders and the archived count) and, when `archived`, the archived
/// threads.
pub async fn lists(h: &Handle, archived: bool, timeout: Duration) -> Result<Lists, Fail> {
    let active = get(h, requests::threads(), timeout, requests::read_threads).await?;
    let archived = if archived {
        Some(
            get(
                h,
                requests::archived_threads(),
                timeout,
                requests::read_threads,
            )
            .await?
            .threads,
        )
    } else {
        None
    };
    Ok(Lists { active, archived })
}

/// One search page.
pub async fn search(
    h: &Handle,
    q: &str,
    scope: SearchIn,
    folder: Option<i64>,
    offset: Option<i64>,
    timeout: Duration,
) -> Result<SearchPage, Fail> {
    let scope = match scope {
        SearchIn::Active => SearchScope::Active,
        SearchIn::Archived => SearchScope::Archived,
        SearchIn::All => SearchScope::All,
    };
    get(
        h,
        requests::search(q.trim(), scope, folder, offset),
        timeout,
        requests::read_search,
    )
    .await
}

/// Why a create did not happen.
#[derive(Debug, Clone)]
pub enum CreateFail {
    /// The folder names no model: the dashboard's steps.
    NoModel(String),
    /// The folder list does not show it (gone, or hidden from the device).
    NoFolder,
    Fail(Fail),
}

/// A thread in `folder_id`, with the folder's model as lmgw lists it now (the phone has no model
/// picker: the model is the owner's choice).
pub async fn create(h: &Handle, folder_id: i64, timeout: Duration) -> Result<Thread, CreateFail> {
    let list = get(h, requests::folders(), timeout, requests::read_folders)
        .await
        .map_err(CreateFail::Fail)?;
    let folder = list
        .folders
        .iter()
        .find(|f| f.id == folder_id)
        .ok_or(CreateFail::NoFolder)?;
    let model = folder_model(folder).map_err(CreateFail::NoModel)?;
    get(
        h,
        requests::create_thread(&model, Some(folder_id)),
        timeout,
        requests::read_created,
    )
    .await
    .map_err(CreateFail::Fail)
}

/// An action on a stored thread (design §6.4).
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Pin(bool),
    Archive(bool),
    /// Into the folder, `None` for out of any.
    Move(Option<i64>),
    /// A new title (lmgw trims it; empty after that is its `400 bad_request`).
    Rename(String),
    Delete,
}

impl Action {
    /// What the action is, for the log: never the new title (message contents and titles are not
    /// logged, design §3.5).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Pin(true) => "pin",
            Self::Pin(false) => "unpin",
            Self::Archive(true) => "archive",
            Self::Archive(false) => "restore",
            Self::Move(_) => "move",
            Self::Rename(_) => "rename",
            Self::Delete => "delete",
        }
    }
}

/// Runs `action` on `thread`: the thread as lmgw answers it now, `None` for a delete.
pub async fn act(
    h: &Handle,
    thread: i64,
    action: &Action,
    timeout: Duration,
) -> Result<Option<ThreadRow>, Fail> {
    let changed = |req| get(h, req, timeout, requests::read_changed);
    let row = match action {
        Action::Pin(on) => changed(requests::pin(thread, *on)).await?,
        Action::Archive(on) => changed(requests::archive(thread, *on)).await?,
        Action::Move(folder) => changed(requests::move_to_folder(thread, *folder)).await?,
        Action::Rename(title) => changed(requests::threads::rename(thread, title)).await?,
        Action::Delete => {
            get(h, requests::delete(thread), timeout, requests::read_deleted).await?;
            return Ok(None);
        }
    };
    Ok(Some(row.row))
}
