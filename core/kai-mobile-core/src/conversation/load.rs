//! The Kai conversation's lookups (design §6.1, §6.2): the folder by lmgw-client's rule
//! (`ongoing::pick`, kai §4), the threads in it, `current`, and the thread with its messages. Each
//! is one request over the paired link ([`crate::link::http`]).

use std::time::Duration;

use lmgw_client::{
    ongoing::{self, Pick, Readiness},
    requests::{self, ApiError, Folder, FolderCreate, ThreadDetail},
};

use crate::link::http::{self, Handle, NoAnswer};

/// What a lookup found (design §6.1).
#[derive(Debug, Clone)]
pub enum Loaded {
    /// The folder is ready: `current` named the thread, and its messages came.
    Ready {
        folder: Box<Folder>,
        remember: Option<i64>,
        detail: Box<ThreadDetail>,
        rolled_over: bool,
        /// The threads in the folder (`GET /chat/api/threads`), for the reply notifications;
        /// `None` when the list could not be read.
        threads: Option<Vec<i64>>,
    },
    /// The folder needs the owner's steps.
    NotReady {
        folder: Box<Folder>,
        remember: Option<i64>,
        steps: String,
        threads: Option<Vec<i64>>,
    },
    /// The lookup failed.
    Failed(Fail),
}

/// Why a request failed: lmgw's refusal (with its status), or no answer.
#[derive(Debug, Clone)]
pub enum Fail {
    Refused { status: u16, error: ApiError },
    NoAnswer(NoAnswer),
}

impl Fail {
    pub fn words(&self, timeout: Duration) -> String {
        match self {
            Self::Refused { error, .. } => format!("{} ({})", error.message, error.code),
            Self::NoAnswer(n) => n.words(timeout),
        }
    }
}

async fn get<T>(
    h: &Handle,
    req: requests::Request,
    timeout: Duration,
    read: fn(u16, &str) -> Result<T, ApiError>,
) -> Result<T, Fail> {
    let (status, body) = http::call(h, &req, timeout).await.map_err(Fail::NoAnswer)?;
    read(status, &body).map_err(|error| Fail::Refused { status, error })
}

/// The folder lookup, `current` (a new thread when `new`) and the thread (module doc).
pub async fn load(
    h: &Handle,
    name: &str,
    remembered: Option<i64>,
    new: bool,
    timeout: Duration,
) -> Loaded {
    match load_inner(h, name, remembered, new, timeout).await {
        Ok(l) => l,
        Err(f) => Loaded::Failed(f),
    }
}

async fn load_inner(
    h: &Handle,
    name: &str,
    remembered: Option<i64>,
    new: bool,
    timeout: Duration,
) -> Result<Loaded, Fail> {
    let list = get(h, requests::folders(), timeout, requests::read_folders).await?;
    let mut pick = ongoing::pick(&list.folders, name, remembered);
    if pick == Pick::Create {
        // A plain folder of the name (the owner gives it a model and marks it ongoing), then the
        // list again: racing creates converge on the lowest id (kai K2-22).
        tracing::info!(%name, "conversation: creating the folder");
        let body = FolderCreate {
            name: name.to_owned(),
            ..Default::default()
        };
        get(
            h,
            requests::create_folder(&body),
            timeout,
            requests::read_folder,
        )
        .await?;
        let list = get(h, requests::folders(), timeout, requests::read_folders).await?;
        pick = ongoing::pick(&list.folders, name, remembered);
    }
    let Pick::Use {
        folder,
        namesakes,
        remember,
    } = pick
    else {
        return Err(Fail::NoAnswer(NoAnswer::Failed(
            crate::link::failure::Failure::Other(format!(
                "lmgw lists no folder '{name}' after creating it"
            )),
        )));
    };
    if !namesakes.is_empty() {
        tracing::info!(
            folder = folder.id,
            ?namesakes,
            "conversation: other folders of the name"
        );
    }
    let threads = match get(h, requests::threads(), timeout, requests::read_threads).await {
        Ok(list) => Some(
            list.threads
                .iter()
                .filter(|t| t.folder_id == Some(folder.id))
                .map(|t| t.id)
                .collect(),
        ),
        Err(f) => {
            tracing::warn!(why = %f.words(timeout), "conversation: the thread list could not be read");
            None
        }
    };
    if ongoing::readiness(&folder) != Readiness::Ready {
        let steps = steps(name, &folder);
        return Ok(Loaded::NotReady {
            folder,
            remember,
            steps,
            threads,
        });
    }
    let current = match get(
        h,
        requests::current(folder.id, new),
        timeout,
        requests::read_current,
    )
    .await
    {
        Ok(c) => c,
        Err(Fail::Refused { error, .. })
            if error.code == "folder_no_model" || error.code == "not_ongoing" =>
        {
            let mut f = (*folder).clone();
            if error.code == "folder_no_model" {
                f.defaults.model_alias = None;
            } else {
                f.ongoing = None;
            }
            return Ok(Loaded::NotReady {
                steps: steps(name, &f),
                folder: Box::new(f),
                remember,
                threads,
            });
        }
        Err(e) => return Err(e),
    };
    let detail = load_thread(h, current.thread.row.id, timeout).await?;
    Ok(Loaded::Ready {
        folder,
        remember,
        detail: Box::new(detail),
        rolled_over: current.rolled_over,
        threads,
    })
}

/// `GET /chat/api/threads/{id}`.
pub async fn load_thread(h: &Handle, id: i64, timeout: Duration) -> Result<ThreadDetail, Fail> {
    get(h, requests::thread(id), timeout, requests::read_thread).await
}

/// The owner's steps for a folder that is not ready, in the dashboard's words (lmgw's folder form:
/// the folder's menu → Folder settings: Model, Ongoing conversation), to be done on the computer.
pub fn steps(name: &str, f: &Folder) -> String {
    match ongoing::readiness(f) {
        Readiness::NotOngoing => format!(
            "The folder '{name}' is not an ongoing conversation yet. On lmgw: Chat, the folder's \
             menu → Folder settings, tick Ongoing conversation."
        ),
        _ => format!(
            "The folder '{name}' has no model yet. On lmgw: Chat, the folder's menu → Folder \
             settings, pick its Model and tick Ongoing conversation."
        ),
    }
}
