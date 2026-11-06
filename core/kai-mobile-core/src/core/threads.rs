//! The threads browser's entry points (design §6.4): the browser's lists and the opened thread go
//! through the link's actor like the conversation's; a search, a create and an action run on the
//! core's runtime over the link the actor hands out, and tell the actor what they changed.

use std::time::Duration;

use super::{Core, stopped, stream_result};
use crate::{
    CoreError,
    conversation::{load::Fail, send},
    link::{actor::Cmd, http::Handle},
    threads::{
        browser::{SEARCH_MIN_CHARS, search_view, searchable},
        io::{self, Action, CreateFail},
        view::{SearchIn, SearchPageView},
    },
};

/// A request's failure as Kotlin sees it: lmgw's refusal verbatim with its code, or no answer with
/// the bound's setting.
fn fail_error(f: &Fail, timeout: Duration) -> CoreError {
    match f {
        Fail::Refused { error, .. } => CoreError::Refused {
            code: error.code.clone(),
            reason: error.message.clone(),
            hint: None,
        },
        Fail::NoAnswer(n) => CoreError::Unreachable {
            reason: n.words(timeout),
            setting: Some("lmgw.timeout_s".to_owned()),
        },
    }
}

impl Core {
    /// The link for a browser request, and the Kai conversation's thread.
    async fn linked(&self) -> Result<(Handle, Option<i64>), CoreError> {
        self.ask(|reply| Cmd::Linked { reply })
            .await
            .unwrap_or_else(|| Err(stopped()))
    }

    /// Tells the actor of a refusal, so a key refusal is acted on (§5.5).
    fn refused(&self, h: &Handle, f: &Fail) {
        if let Fail::Refused { status, error } = f {
            self.send(Cmd::Refused {
                identity: h.identity(),
                status: *status,
                error: error.clone(),
            });
        }
    }

    async fn act(&self, thread_id: i64, action: Action) -> Result<(), CoreError> {
        let (h, _) = self.linked().await?;
        let timeout = self.timeout();
        let task = {
            let h = h.clone();
            let action = action.clone();
            async move { io::act(&h, thread_id, &action, timeout).await }
        };
        match self.on_rt(task).await {
            Ok(row) => {
                tracing::info!(thread = thread_id, action = action.kind(), "threads: done");
                self.send(Cmd::Acted {
                    identity: h.identity(),
                    thread: thread_id,
                    row: row.map(Box::new),
                });
                Ok(())
            }
            Err(f) => {
                self.refused(&h, &f);
                Err(fail_error(&f, timeout))
            }
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Core {
    /// The threads browser shows (its lists are read and kept current) or closed (they are
    /// dropped), design §6.4.
    pub fn browser(&self, showing: bool) {
        self.send(Cmd::Browser(showing));
    }

    /// The browser's archived section opened (its list is read and kept current) or closed.
    pub fn browser_archived(&self, showing: bool) {
        self.send(Cmd::BrowserArchived(showing));
    }

    /// One page of lmgw's full-text search: `query` of at least two characters (refused without a
    /// request below it), `scope`, optionally one folder, `offset` the previous page's
    /// `next_offset`.
    pub async fn search(
        &self,
        query: String,
        scope: SearchIn,
        folder_id: Option<i64>,
        offset: Option<i64>,
    ) -> Result<SearchPageView, CoreError> {
        if !searchable(&query) {
            return Err(CoreError::NotReady {
                reason: format!("A search needs at least {SEARCH_MIN_CHARS} characters."),
            });
        }
        let (h, kai_thread) = self.linked().await?;
        let timeout = self.timeout();
        let task = {
            let h = h.clone();
            async move { io::search(&h, &query, scope, folder_id, offset, timeout).await }
        };
        match self.on_rt(task).await {
            Ok(page) => Ok(search_view(page, kai_thread)),
            Err(f) => {
                self.refused(&h, &f);
                Err(fail_error(&f, timeout))
            }
        }
    }

    /// Opens thread `thread_id` beside the Kai conversation (`Update::Opened`,
    /// `Update::OpenedMessages`); opening the open one reads it again.
    pub fn open_thread(&self, thread_id: i64) {
        self.send(Cmd::Open(Some(thread_id)));
    }

    /// Closes the opened thread.
    pub fn close_thread(&self) {
        self.send(Cmd::Open(None));
    }

    /// Sends `text` to the opened thread `thread_id` (§6.3's send, stream and notices); the reply
    /// streams in through `Update::OpenedMessages`.
    pub async fn send_in_thread(&self, thread_id: i64, text: String) -> Result<(), CoreError> {
        let (h, seq) = self
            .ask(|reply| Cmd::ThreadSendStart {
                thread: thread_id,
                reply,
            })
            .await
            .unwrap_or_else(|| Err(stopped()))?;
        let timeout = self.timeout();
        let cmds = self.cmds.clone();
        self.on_rt(async move {
            let req = send::send_request(thread_id, &text);
            let frames = cmds.clone();
            let result = send::stream(&h, &req, timeout, |frame| {
                let _ = frames.send(Cmd::ThreadFrame { seq, frame });
            })
            .await;
            let out = stream_result(&result, timeout);
            let _ = cmds.send(Cmd::ThreadSendEnded {
                seq,
                identity: h.identity(),
                result,
            });
            out
        })
        .await
    }

    /// A new thread in folder `folder_id`, from the folder's defaults with the folder's model (the
    /// phone has no model picker); it does not open it (the app opens what it selects, with
    /// `open_thread`). Answers the new thread's id, or the dashboard's steps
    /// when the folder names no model.
    pub async fn new_thread(&self, folder_id: i64) -> Result<i64, CoreError> {
        let (h, _) = self.linked().await?;
        let timeout = self.timeout();
        let task = {
            let h = h.clone();
            async move { io::create(&h, folder_id, timeout).await }
        };
        match self.on_rt(task).await {
            Ok(t) => {
                let id = t.row.id;
                tracing::info!(folder = folder_id, thread = id, "threads: created");
                self.send(Cmd::Acted {
                    identity: h.identity(),
                    thread: id,
                    row: Some(Box::new(t.row)),
                });
                Ok(id)
            }
            Err(CreateFail::NoModel(steps)) => Err(CoreError::NotReady { reason: steps }),
            Err(CreateFail::NoFolder) => Err(CoreError::NotReady {
                reason: "lmgw does not list this folder for this device any more.".to_owned(),
            }),
            Err(CreateFail::Fail(f)) => {
                self.refused(&h, &f);
                Err(fail_error(&f, timeout))
            }
        }
    }

    /// Pins (`true`) or unpins a thread; pinning an archived one restores it (lmgw's rule).
    pub async fn pin_thread(&self, thread_id: i64, pinned: bool) -> Result<(), CoreError> {
        self.act(thread_id, Action::Pin(pinned)).await
    }

    /// Archives (`true`) or restores a thread.
    pub async fn archive_thread(&self, thread_id: i64, archived: bool) -> Result<(), CoreError> {
        self.act(thread_id, Action::Archive(archived)).await
    }

    /// Moves a thread into `folder_id`, `None` for out of any folder.
    pub async fn move_thread(
        &self,
        thread_id: i64,
        folder_id: Option<i64>,
    ) -> Result<(), CoreError> {
        self.act(thread_id, Action::Move(folder_id)).await
    }

    /// Renames a thread (`POST /chat/api/threads/{id}/title`); lmgw trims the title and refuses an
    /// empty one with its code.
    pub async fn rename_thread(&self, thread_id: i64, title: String) -> Result<(), CoreError> {
        self.act(thread_id, Action::Rename(title)).await
    }

    /// Deletes a thread and its messages on lmgw (the app asked the owner first).
    pub async fn delete_thread(&self, thread_id: i64) -> Result<(), CoreError> {
        self.act(thread_id, Action::Delete).await
    }
}
