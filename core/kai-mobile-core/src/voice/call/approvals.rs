//! Approvals in the call (design §11, api §6.4): a call of the thread's turn waits for the owner's
//! verdict.
//!
//! - **Shown:** lmgw's `mcp_approval_request` item goes to the link's book as a card
//!   ([`Effect::ApprovalShown`]); the first time an id comes in this call the phone asks for
//!   attention ([`Effect::Asked`]: the ask earcon and its haptic), since the owner may be listening
//!   with the screen off.
//! - **Waiting:** while a call of the bound thread waits with no verdict sent, the state line says
//!   so and the idle clock stands still (§7.7: "no pending approval"). The microphone is not held
//!   back: the owner's words are a new message, which declines what waits (api §6.3).
//! - **Deciding:** the verdicts of a reply whose every call this session showed come from the book
//!   ([`Call::verdicts`]): one `mcp_approval_response` item each, then `response.create` with an
//!   `event_id`. What lmgw answers ([`CallVerdicts`]):
//!   - **not taken:** an `error` about them (echoing that create, or an `approval_*` code) whose
//!     code says lmgw refused before deciding (an `approval_*` code but `approval_decided`, or
//!     `key_rate`): lmgw's words go on the card;
//!   - **unsettled:** such an `error` with any other code, the response after them failing (the
//!     model, an upstream timeout, `gpu_hold`: lmgw may have written the decision first), cancelled
//!     (a stop) or ending without output, or the call's end before any of that: handed back to be
//!     sent again by the route, whose answer settles them;
//!   - **probably taken:** that response ran and said something. Only probably: lmgw holds a create
//!     while the owner speaks, and a turn with words declines what waits (the verdicts go unused),
//!     so the owner's turn looks like the resumed one. The book keeps them sent until the feed's
//!     `approval.decided` says how lmgw decided.
//! - **Decided elsewhere:** `lmgw.approval.decided` closes the card ([`Effect::ApprovalDecided`]).

use lmgw_client::{
    feed::ApprovalRequested,
    realtime::{ApprovalDecidedEvent, ApprovalRequest, ClientEvent, ErrorFacts},
};

use super::{Call, Effect};
use crate::{
    approvals::{CallVerdicts, refused_before_deciding},
    voice::view::CallPhase,
};

/// Verdicts on their way to lmgw through the call.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Approving {
    /// The `event_id` their `response.create` carries.
    tag: String,
    thread_id: i64,
    verdicts: Vec<(String, bool)>,
    /// The response lmgw created after them (the resumed turn's, or the owner's turn's).
    response: Option<String>,
    /// That response said something (a spoken delta, its audio, a chat frame).
    output: bool,
}

impl Approving {
    fn ids(&self) -> Vec<String> {
        self.verdicts.iter().map(|(id, _)| id.clone()).collect()
    }

    fn unsettled(self) -> CallVerdicts {
        CallVerdicts::Unsettled {
            thread_id: self.thread_id,
            verdicts: self.verdicts,
        }
    }
}

impl Call {
    /// The calls waiting for the owner's verdict, by thread, as the link's book has them.
    pub fn approvals(&mut self, waiting: Vec<crate::approvals::Waiting>, now: u64) {
        self.waiting = waiting;
        self.settle(now);
    }

    /// The tools of the bound thread's calls that wait for the owner's verdict.
    pub(super) fn awaiting(&self) -> Vec<String> {
        let Some((thread, _)) = &self.thread else {
            return Vec::new();
        };
        self.waiting
            .iter()
            .filter(|w| w.thread_id == *thread)
            .map(|w| w.tool.clone())
            .collect()
    }

    /// The verdicts of a reply whose every call this session showed (module doc). False when the
    /// call cannot take them now (not bound, ended, a batch still on its way): the route sends them.
    pub fn verdicts(&mut self, thread_id: i64, verdicts: Vec<(String, bool)>, now: u64) -> bool {
        if self.phase != CallPhase::Live || self.lmgw_ended || self.approving.is_some() {
            return false;
        }
        for (id, approve) in &verdicts {
            self.send(ClientEvent::ApprovalResponse {
                approval_request_id: id.clone(),
                approve: *approve,
                reason: None,
            });
        }
        let tag = self.tag("kai_approvals");
        tracing::info!(call = self.n, n = verdicts.len(), %tag, "voice: the verdicts go through the call");
        self.send_tagged(ClientEvent::ResponseCreate, tag.clone());
        self.approving = Some(Approving {
            tag,
            thread_id,
            verdicts,
            response: None,
            output: false,
        });
        self.settle(now);
        true
    }

    /// `conversation.item.done` of an `mcp_approval_request` item.
    pub(super) fn approval_shown(&mut self, a: ApprovalRequest) {
        let (thread_id, thread_title) = self.thread.clone().unwrap_or_default();
        let id = a.approval_request_id.clone();
        tracing::info!(call = self.n, %id, "voice: a call waits for an approval");
        self.out
            .push(Effect::ApprovalShown(Box::new(ApprovalRequested {
                thread_id,
                thread_title,
                message_id: 0,
                approval_request_id: a.approval_request_id,
                server_label: a.server_label,
                name: a.name,
                arguments: a.arguments,
                call_id: a.call_id,
            })));
        if self.asked.insert(id) {
            self.out.push(Effect::Asked);
        }
    }

    /// `lmgw.approval.decided`: another client decided a call this session showed.
    pub(super) fn approval_decided(&mut self, d: ApprovalDecidedEvent) {
        tracing::info!(call = self.n, id = %d.approval_request_id, approve = d.approve, by = ?d.by, "voice: decided elsewhere");
        self.out
            .push(Effect::ApprovalDecided(d.approval_request_id));
    }

    /// An `error` about the verdicts on their way (echoing their `response.create`, or an
    /// `approval_*` code): not taken when its code says lmgw refused before deciding, else
    /// unsettled (module doc). Only the code is logged: lmgw's words name the tools. True when it
    /// was about them.
    pub(super) fn verdicts_refused(&mut self, e: &ErrorFacts, echo: Option<&str>) -> bool {
        let Some(a) = &self.approving else {
            return false;
        };
        let approval_code = e
            .code
            .as_deref()
            .is_some_and(|c| c.starts_with("approval_"));
        if echo != Some(a.tag.as_str()) && !approval_code {
            return false;
        }
        let Some(a) = self.approving.take() else {
            return false;
        };
        let outcome = if refused_before_deciding(e.code.as_deref()) {
            tracing::info!(call = self.n, code = ?e.code, "voice: lmgw refused the verdicts before deciding");
            CallVerdicts::NotTaken {
                ids: a.ids(),
                code: e.code.clone(),
                words: e.message.clone(),
            }
        } else {
            tracing::info!(call = self.n, code = ?e.code, "voice: an error about the verdicts that does not settle them; the route sends them again");
            a.unsettled()
        };
        self.out.push(Effect::Verdicts(outcome));
        true
    }

    /// `response.created`: the first after the verdicts' `response.create` is the one they went
    /// with (the resumed turn's, or the owner's when their words came first).
    pub(super) fn verdicts_response(&mut self, response_id: &str) {
        if let Some(a) = self.approving.as_mut()
            && a.response.is_none()
        {
            a.response = Some(response_id.to_owned());
        }
    }

    /// That response said something.
    pub(super) fn verdicts_output(&mut self, response_id: &str) {
        if let Some(a) = self.approving.as_mut()
            && a.response.as_deref() == Some(response_id)
        {
            a.output = true;
        }
    }

    /// `response.done` of that response: completed with output, probably taken (the book keeps
    /// them sent); cancelled, failed or silent, unsettled (module doc).
    pub(super) fn verdicts_done(&mut self, response_id: &str, status: &str) {
        let Some(a) = &self.approving else {
            return;
        };
        if a.response.as_deref() != Some(response_id) {
            return;
        }
        let Some(a) = self.approving.take() else {
            return;
        };
        let outcome = if a.output && matches!(status, "completed" | "incomplete") {
            tracing::info!(
                call = self.n,
                "voice: the response after the verdicts ran; the feed says how lmgw decided"
            );
            CallVerdicts::Taken(a.ids())
        } else {
            tracing::info!(call = self.n, %status, output = a.output, "voice: the response after the verdicts does not settle them; the route sends them again");
            a.unsettled()
        };
        self.out.push(Effect::Verdicts(outcome));
    }

    /// The call ended with verdicts on their way and no word from lmgw: handed back.
    pub(super) fn verdicts_unsettled(&mut self) {
        if let Some(a) = self.approving.take() {
            tracing::info!(
                call = self.n,
                "voice: the call ended before lmgw answered the verdicts"
            );
            self.out.push(Effect::Verdicts(a.unsettled()));
        }
    }

    /// A fresh `event_id` of the phone's (`<prefix>_<call>_<n>`).
    pub(super) fn tag(&mut self, prefix: &str) -> String {
        self.tags += 1;
        format!("{prefix}_{}_{}", self.n, self.tags)
    }
}

/// The `event_id` an `error` echoes (`error.event_id`, OpenAI's), if it is one.
pub(super) fn error_echo(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    (v["type"] == "error")
        .then(|| v["error"]["event_id"].as_str().map(str::to_owned))
        .flatten()
        .filter(|id| !id.is_empty())
}
