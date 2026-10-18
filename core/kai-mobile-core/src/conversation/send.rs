//! A text send (design §6.3) and an approval's verdicts (§11): each POSTs and reads the resumed
//! turn's event stream to its end (a request that stopped reading would stop the turn), decoded by
//! lmgw-client's `SseDecoder` and typed by `TurnEvent::parse`. Only the head is bounded
//! (`lmgw.timeout_s`): a turn takes as long as its model does.

use std::time::Duration;

use lmgw_client::{
    feed::SseDecoder,
    requests::{
        self, ApiError,
        approvals::{ApprovalDecision, ApprovalsRequest},
    },
    types::{
        chat_frames::{ErrorFrame, ToolFrame, TurnEvent},
        chat_turn::SendRequest,
    },
};

use crate::link::{
    failure::Failure,
    http::{self, Handle, NoAnswer},
    tls::describe,
};

/// What a turn's stream says, as the conversation shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// `turn`: the user's message was written with this id.
    Started { user_message_id: i64 },
    /// `delta`: reply text to append.
    Delta(String),
    /// `tool` start: the tool's name.
    Tool(String),
    /// `error`: the frame whole (a label's notice or a fatal error; `notice` tells which).
    Error(ErrorFrame),
    /// `done`: the reply's id when it was saved, the alias that answered in its place.
    Done {
        message_id: Option<i64>,
        answered_by: Option<String>,
        aborted: bool,
    },
}

/// How a stream ended without its `done`.
#[derive(Debug, Clone)]
pub enum StreamFail {
    Refused {
        status: u16,
        error: ApiError,
    },
    NoAnswer(NoAnswer),
    /// The stream broke off after its head.
    Broke(String),
}

/// `POST /chat/api/threads/{id}/send` with `{content}` (lmgw-client's builder).
pub fn send_request(thread_id: i64, text: &str) -> requests::Request {
    requests::send(
        thread_id,
        &SendRequest {
            content: text.to_owned(),
            ..Default::default()
        },
    )
}

/// The verdicts for every call of a reply (`POST /chat/api/threads/{id}/approvals`).
pub fn approvals_request(thread_id: i64, verdicts: &[(String, bool)]) -> requests::Request {
    requests::approvals(
        thread_id,
        &ApprovalsRequest {
            decisions: verdicts
                .iter()
                .map(|(id, approve)| ApprovalDecision {
                    approval_request_id: id.clone(),
                    approve: *approve,
                    reason: None,
                })
                .collect(),
            speak: false,
        },
    )
}

/// Sends `req` and reads its turn stream to the end, handing each frame to `frame`. `Ok` once
/// the stream ended (with or without `done`).
pub async fn stream(
    h: &Handle,
    req: &requests::Request,
    timeout: Duration,
    mut frame: impl FnMut(Frame),
) -> Result<(), StreamFail> {
    let mut resp = http::open(h, req, timeout)
        .await
        .map_err(StreamFail::NoAnswer)?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = tokio::time::timeout(timeout, resp.text())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        return Err(StreamFail::Refused {
            status,
            error: requests::read_refusal(status, &body),
        });
    }
    let mut sse = SseDecoder::new();
    loop {
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => return Ok(()),
            Err(e) => {
                return Err(match Failure::of(&e) {
                    Failure::Other(_) => StreamFail::Broke(describe(&e)),
                    f => StreamFail::Broke(f.words()),
                });
            }
        };
        for item in sse.push(&chunk) {
            let lmgw_client::feed::SseItem::Frame(f) = item else {
                continue;
            };
            match TurnEvent::parse(&f.event, &f.data) {
                Ok(TurnEvent::Turn(t)) => frame(Frame::Started {
                    user_message_id: t.user_message_id,
                }),
                Ok(TurnEvent::Delta(t)) => frame(Frame::Delta(t.text)),
                Ok(TurnEvent::Tool(ToolFrame::Start(s))) => frame(Frame::Tool(s.name)),
                Ok(TurnEvent::Error(e)) => frame(Frame::Error(e)),
                Ok(TurnEvent::Done(d)) => frame(Frame::Done {
                    message_id: d.saved.as_ref().filter(|s| s.saved).map(|s| s.message_id),
                    answered_by: d.saved.and_then(|s| s.answered_by),
                    aborted: d.aborted,
                }),
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(event = %e.event, "conversation: a turn frame that does not read, skipped")
                }
            }
        }
    }
}
