//! One connection of the held feed (design §5.4, §12): `GET /chat/api/feed?keepalive_s=` raced on
//! the pairing's URLs (or, while the core polls for the owner's Confirm with the app visible, on
//! the kept-alive connection to the URL that answered), its bytes through lmgw-client's
//! [`FeedReader`], each chunk's items handed to the link's actor with the cursor and the reader's
//! open set. The actor owns every deadline (on the boot clock) and aborts the task when one passes;
//! the race itself is also bounded by `lmgw.timeout_s` here.

use std::time::Duration;

use lmgw_client::{
    feed::{ApprovalRequested, FeedItem, FeedReader},
    pairing::Urls,
    requests::{self, ApiError, CertDates},
};
use tokio::sync::mpsc;

use super::{
    actor::Cmd,
    failure::Failure,
    race::race,
    tls::{Tls, describe},
};

/// What a connection tells the actor.
#[derive(Debug)]
pub enum FeedMsg {
    /// lmgw answered at `url` (the head came, any status).
    Answered { url: String },
    /// lmgw refused the feed: its status and refusal.
    Refused { status: u16, error: ApiError },
    /// No URL answered: each one's failure, in the list's order; `timed_out` when the race's bound
    /// passed first.
    Failed {
        failures: Vec<(String, Failure)>,
        timed_out: bool,
    },
    /// A chunk's items, and what the reader holds after them.
    Chunk(Box<Chunk>),
    /// The stream ended (lmgw closed it, or it broke).
    Ended { why: String },
}

/// A chunk's items (module doc).
#[derive(Debug)]
pub struct Chunk {
    pub items: Vec<FeedItem>,
    /// The bytes were not empty (a sign of life).
    pub bytes: usize,
    pub cursor: Option<String>,
    pub open: Vec<ApprovalRequested>,
    pub keepalive_s: Option<u32>,
    pub urls: Option<Urls>,
    pub cert: Option<CertDates>,
    pub dashboard_url: Option<String>,
}

/// How a connection goes.
pub struct Plan {
    pub generation: u64,
    pub tls: Tls,
    /// The URLs in the order to try them (the last good first).
    pub urls: Vec<String>,
    /// Use the shared client's kept-alive connection to this URL instead of racing (§4.2: the
    /// confirmation's poll, no new handshake).
    pub shared: Option<String>,
    pub stagger: Duration,
    pub timeout: Duration,
    pub reader: FeedReader,
}

/// Runs one connection (module doc) until it ends or is aborted.
pub async fn run(plan: Plan, tx: mpsc::UnboundedSender<Cmd>) {
    let generation = plan.generation;
    let send = |msg: FeedMsg| tx.send(Cmd::Feed { generation, msg }).is_ok();
    let req = plan.reader.resume();
    let get = |http: reqwest::Client, base: &str| {
        let mut rb = http.get(req.url(base));
        for h in &req.headers {
            rb = rb.header(h.name.as_str(), h.value.as_str());
        }
        rb.send()
    };
    let answered = match plan.shared.as_deref() {
        Some(url) => match tokio::time::timeout(plan.timeout, get(plan.tls.http(), url)).await {
            Ok(Ok(r)) => Ok((url.to_owned(), r)),
            Ok(Err(e)) => Err((vec![(url.to_owned(), Failure::of(&e))], false)),
            Err(_) => Err((vec![(url.to_owned(), Failure::NoAnswer)], true)),
        },
        None => {
            let attempt = |base: &str, answered| {
                let http = plan.tls.racing(answered);
                let base = base.to_owned();
                let get = &get;
                async move {
                    let http = http.map_err(|e| Failure::Other(describe(&e)))?;
                    get(http, &base).await.map_err(|e| Failure::of(&e))
                }
            };
            let raced = race(plan.urls.clone(), plan.stagger, attempt);
            match tokio::time::timeout(plan.timeout, raced).await {
                Ok(Ok(won)) => Ok(won),
                Ok(Err(failed)) => Err((failed, false)),
                Err(_) => Err((
                    plan.urls
                        .iter()
                        .map(|u| (u.clone(), Failure::NoAnswer))
                        .collect(),
                    true,
                )),
            }
        }
    };
    let (url, mut resp) = match answered {
        Ok(won) => won,
        Err((failures, timed_out)) => {
            send(FeedMsg::Failed {
                failures,
                timed_out,
            });
            return;
        }
    };
    if !send(FeedMsg::Answered { url }) {
        return;
    }
    let status = resp.status();
    if !status.is_success() {
        let body = tokio::time::timeout(plan.timeout, resp.text())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        send(FeedMsg::Refused {
            status: status.as_u16(),
            error: requests::read_refusal(status.as_u16(), &body),
        });
        return;
    }
    let mut reader = plan.reader;
    loop {
        let bytes = match resp.chunk().await {
            Ok(Some(b)) => b,
            Ok(None) => {
                send(FeedMsg::Ended {
                    why: "lmgw closed the feed".to_owned(),
                });
                return;
            }
            Err(e) => {
                send(FeedMsg::Ended {
                    why: format!("the feed broke: {}", describe(&e)),
                });
                return;
            }
        };
        let items = reader.push(&bytes);
        let chunk = Chunk {
            items,
            bytes: bytes.len(),
            cursor: reader.cursor().map(str::to_owned),
            open: reader.open_approvals().to_vec(),
            keepalive_s: reader.keepalive_s(),
            urls: reader.urls().cloned(),
            cert: reader.certificate(),
            dashboard_url: reader.dashboard_url().map(str::to_owned),
        };
        if !send(FeedMsg::Chunk(Box::new(chunk))) {
            return;
        }
    }
}
