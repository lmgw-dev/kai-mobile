//! The late task result's continuation (design §7.6's seam from WP3; lmgw's MCP Tasks design §3.4,
//! `lmgw_client::realtime`'s module doc): a tool that runs as a task answers its turn at once, and
//! its result enters the thread later, while no turn runs; lmgw then says `lmgw.task.done` and
//! starts no turn itself. The client picks the moment and sends `response.create` with no new words,
//! which answers every result no reply answered yet.
//!
//! - **A turn of the owner's or another's answers it:** the owner's speech, a press or a commit in
//!   flight (their message goes after the result), or another principal's turn in the thread (K16):
//!   no `response.create` then.
//! - **A response merely open does not:** a result enters only while no turn of the thread runs, so
//!   one that ended while Kai spoke entered after that turn's request (lmgw says
//!   `lmgw.task.done` once the turn ended, while the phone may still play its audio). Owed, and sent
//!   at the next quiet point; where a turn did answer it after all, lmgw's `empty_turn` says so.
//! - **Quiet:** sent at once. **Paused** (another app holds the audio, §8.5), a stop in flight, a
//!   push-to-talk tail, or a call of the thread waiting for the owner's verdict (a continuation's
//!   reply is a new message, which would decline it): owed, and sent once that clears, unless a
//!   turn has answered it meanwhile.
//! - **Refusals** echoing its `event_id`: `empty_turn` (another window answered the results) and
//!   `turn_running` (a turn of the thread runs; the results enter when it ends) are logged, not
//!   shown; any other code is the call's error as usual.

use lmgw_client::realtime::{ClientEvent, ErrorFacts, TaskDone};

use super::Call;
use crate::voice::view::CallPhase;

impl Call {
    /// `lmgw.task.done`.
    pub(super) fn task_done(&mut self, d: TaskDone) {
        tracing::info!(
            call = self.n,
            message = d.message_id,
            status = %d.status,
            "voice: a late task result entered the thread"
        );
        if self.turn_answers_results() {
            tracing::info!(
                call = self.n,
                "voice: the turn under way answers the task's result"
            );
            return;
        }
        self.continuation_owed = true;
    }

    /// A turn is under way whose request holds what entered the thread (module doc): the owner's,
    /// or another principal's.
    fn turn_answers_results(&self) -> bool {
        self.turn_under_way() || !self.others.is_empty()
    }

    /// Sends the owed continuation once the call is quiet (from `settle`).
    pub(super) fn continue_if_due(&mut self, now: u64) {
        if !self.continuation_owed {
            return;
        }
        if self.phase != CallPhase::Live {
            if self.over() {
                self.continuation_owed = false;
            }
            return;
        }
        if self.turn_answers_results() {
            tracing::info!(
                call = self.n,
                "voice: a turn began; it answers the task's result"
            );
            self.continuation_owed = false;
            return;
        }
        if self.response_open(now)
            || self.focus_away
            || self.stop_request.is_some()
            || self.tail_until.is_some()
            || !self.awaiting().is_empty()
        {
            return;
        }
        self.continuation_owed = false;
        let tag = self.tag("kai_continue");
        tracing::info!(call = self.n, %tag, "voice: the continuation answers the task's result");
        self.continuing = Some(tag.clone());
        self.send_tagged(ClientEvent::ResponseCreate, tag);
    }

    /// An `error` echoing the continuation's `response.create` with a code that only says nothing
    /// is owed now: logged. True when it was one.
    pub(super) fn continuation_refused(&mut self, e: &ErrorFacts, echo: Option<&str>) -> bool {
        if echo.is_none() || echo != self.continuing.as_deref() {
            return false;
        }
        self.continuing = None;
        match e.code.as_deref() {
            Some(code @ ("empty_turn" | "turn_running")) => {
                tracing::info!(call = self.n, %code, "voice: the continuation had nothing to answer now");
                true
            }
            _ => false,
        }
    }
}
