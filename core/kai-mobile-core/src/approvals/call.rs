//! Approvals in a voice call (design §11, api §6.4): the calls the running call's session showed
//! (`mcp_approval_request`), lmgw's word that another client decided one (`lmgw.approval.decided`),
//! the verdicts the session took or refused, and what the call waits for.
//!
//! - **A call the session showed** is a card at once, merged with the feed's by id (the feed's facts
//!   win: its message id places the card in the conversation). Its item names no message, so within
//!   its thread it is of one reply with the other calls the same session showed and with the feed's
//!   calls of their message (lmgw keeps at most one reply waiting per thread). It is the session's
//!   to answer while that call runs: verdicts for a reply whose every call it showed go through it,
//!   so lmgw speaks the resumed reply in the call (`Decision::via_call`; the card's `in_call` is the
//!   reply's).
//! - **The call's end** leaves the cards the feed has; a call only the session showed is dropped
//!   with its notification (the feed, the authority once no session answers, brings it back as
//!   `Added` while it still waits).
//! - **What the call waits for** ([`Approvals::waiting`]): every call waiting with no verdict sent,
//!   by thread; the voice task shows the ones of its thread and keeps the idle clock still for them.

use lmgw_client::feed::ApprovalRequested;

use super::{Approvals, Out};

/// A call the voice call `call`'s session showed, with the call's thread (its `message_id` 0: the
/// item names no message; the feed's copy has it).
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub call: u64,
    pub request: ApprovalRequested,
}

/// A call waiting for the owner's verdict: its thread and its tool (`<label>__<name>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub thread_id: i64,
    pub tool: String,
}

/// What became of verdicts given to the call (the voice task's word to the link).
#[derive(Debug, Clone, PartialEq)]
pub enum CallVerdicts {
    /// The response after them ran and said something: most likely the resumed turn, but only the
    /// phone's inference (lmgw holds a create while the owner speaks, and a turn with words
    /// declines what waits), so the ids stay sent until the feed's `approval.decided` closes them.
    Taken(Vec<String>),
    /// lmgw refused them before deciding: an `error` about them (echoing their `response.create`,
    /// or an `approval_*` code) whose code says so ([`super::refused_before_deciding`]). Its code
    /// and words.
    NotTaken {
        ids: Vec<String>,
        code: Option<String>,
        words: String,
    },
    /// They never went (the call had ended or was not bound): the route sends them.
    Unsent {
        thread_id: i64,
        verdicts: Vec<(String, bool)>,
    },
    /// They went, and lmgw's answer does not settle them: the call ended before it said anything,
    /// the response after them was cancelled (a stop) or said nothing, it failed, or an error about
    /// them had another code (lmgw may have decided first). The route sends them again, where
    /// `approval_decided` means lmgw took them and its success that it decides them now.
    Unsettled {
        thread_id: i64,
        verdicts: Vec<(String, bool)>,
    },
}

impl CallVerdicts {
    pub fn ids(&self) -> Vec<String> {
        match self {
            CallVerdicts::Taken(ids) | CallVerdicts::NotTaken { ids, .. } => ids.clone(),
            CallVerdicts::Unsent { verdicts, .. } | CallVerdicts::Unsettled { verdicts, .. } => {
                verdicts.iter().map(|(id, _)| id.clone()).collect()
            }
        }
    }
}

impl Approvals {
    /// The running call whose session showed `id`, if one runs.
    pub(super) fn via_call(&self, id: &str) -> Option<u64> {
        let live = self.live_call?;
        self.shown
            .iter()
            .any(|s| s.call == live && s.request.approval_request_id == id)
            .then_some(live)
    }

    pub(super) fn shown_by_live_call(&self, id: &str) -> bool {
        self.via_call(id).is_some()
    }

    /// A voice call started (`Some`, its number) or ended (`None`). At the end, the calls only its
    /// session showed go, notification and held verdicts with them (module doc).
    pub fn call_running(&mut self, call: Option<u64>) -> Out {
        let ended = self.live_call.filter(|n| Some(*n) != call);
        self.live_call = call;
        let Some(n) = ended else {
            return Out {
                cards: true,
                ..Default::default()
            };
        };
        let in_feed = |id: &str| self.open.iter().any(|a| a.approval_request_id == id);
        let only_call: Vec<String> = self
            .shown
            .iter()
            .filter(|s| s.call == n && !in_feed(&s.request.approval_request_id))
            .map(|s| s.request.approval_request_id.clone())
            .collect();
        self.shown.retain(|s| s.call != n);
        let mut out = self.withdrawn(only_call);
        out.cards = true;
        out
    }

    /// The running call's session showed a call (`mcp_approval_request`): a card at once, and a
    /// notification while no card is seen, unless the feed brought it first.
    pub fn call_shown(&mut self, shown: Shown, seen: bool) -> Out {
        let id = shown.request.approval_request_id.clone();
        if self.gone.contains(&id) {
            return Out::default();
        }
        let request = shown.request.clone();
        match self
            .shown
            .iter_mut()
            .find(|s| s.request.approval_request_id == id)
        {
            Some(s) => *s = shown,
            None => self.shown.push(shown),
        }
        let mut out = Out {
            cards: true,
            ..Default::default()
        };
        if !self.open.iter().any(|a| a.approval_request_id == id) {
            out.merge(self.added(request, seen));
        }
        out
    }

    /// lmgw said on the call that another client decided `id` (or that it went with its reply):
    /// its card and notification go, and a set read before the decision does not bring it back.
    pub fn call_decided(&mut self, id: &str) -> Out {
        self.shown.retain(|s| s.request.approval_request_id != id);
        self.gone.insert(id.to_owned());
        let mut out = self.withdrawn(vec![id.to_owned()]);
        out.cards = true;
        out
    }

    /// lmgw said the verdicts for `ids` were decided (the route's 409 `approval_decided`): as
    /// [`Self::call_decided`].
    pub fn verdicts_taken(&mut self, ids: &[String]) -> Out {
        let mut out = Out::default();
        for id in ids {
            out.merge(self.call_decided(id));
        }
        out
    }

    /// Every call waiting for the owner's verdict (none sent yet), oldest first.
    pub fn waiting(&self) -> Vec<Waiting> {
        self.all()
            .into_iter()
            .filter(|a| !self.sent.contains(&a.approval_request_id))
            .map(|a| Waiting {
                thread_id: a.thread_id,
                tool: format!("{}__{}", a.server_label, a.name),
            })
            .collect()
    }

    /// The notification ids of the calls only `call`'s session showed (for the tests).
    #[cfg(test)]
    pub(super) fn shown_ids(&self) -> Vec<String> {
        self.shown
            .iter()
            .map(|s| s.request.approval_request_id.clone())
            .collect()
    }
}
