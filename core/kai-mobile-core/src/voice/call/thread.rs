//! A thread's call and the move to another thread (design §7.12): a call that binds by
//! `chat_thread` (Talk in an opened thread) follows its own thread, not the Kai conversation, takes
//! K16's facts for that thread from the link, and names no conversation's thread when it is gone;
//! and any call moves to another binding at its next quiet point when Talk is chosen elsewhere
//! (rule 3), by §7.7's rebind with the target as its restart.

use super::Call;
use crate::voice::{
    CallStart, ends,
    view::{CallEnd, CallPhase, EndReason},
};

impl Call {
    /// The call binds by `chat_thread` (Talk in an opened thread), not by the Kai folder.
    pub(super) fn by_thread(&self) -> bool {
        self.start.thread.is_some()
    }

    /// Who else answers in thread `thread` (K16), for a call bound to it by `chat_thread`; the Kai
    /// call takes them from [`Call::conversation`]. Kept in every phase: the link sends a thread's
    /// facts only when they change, so a turn already running when the call starts (or binds) is
    /// heard here once, before `session.created`; the microphone and the view read them only while
    /// the call is live.
    pub fn live(&mut self, thread: i64, answering: Vec<String>, now: u64) {
        if self.start.thread != Some(thread) {
            return;
        }
        self.others = answering;
        self.settle(now);
    }

    /// Stay (rule 3): the move asked by a Talk elsewhere is called off. Refused with words when
    /// there is none to call off any more: the move went already (the next call is another
    /// call, which a Stay never touches) or the call ended.
    pub fn stay(&mut self, now: u64) -> Result<(), String> {
        if self.over() {
            return Err(match self.end.as_ref().and_then(|e| e.restart) {
                Some(_) => "The call moved already; Stay came too late.".to_owned(),
                None => "The call has ended.".to_owned(),
            });
        }
        if self.switch.take().is_some() {
            tracing::info!(call = self.n, "voice: Stay; the move is called off");
        }
        self.settle(now);
        Ok(())
    }

    /// Thread `thread` was deleted or left this device's reach (the feed): a move waiting to go
    /// there is called off, since its call would only be refused.
    pub fn thread_gone(&mut self, thread: i64, now: u64) {
        if self.over() || self.switch.and_then(|s| s.thread) != Some(thread) {
            return;
        }
        tracing::info!(
            call = self.n,
            thread,
            "voice: the thread to move to is gone; the call stays"
        );
        self.switch = None;
        self.captions
            .note("The thread to move to is gone; voice stays here.".to_owned());
        self.settle(now);
    }

    /// The restart this ended call owes (a rebind's or a move's, §7.7) was refused before it
    /// reached the voice task (the link's state): the call no longer says a call follows, and
    /// says why, with Reconnect to the restart's binding. No effect: Kotlin heard the refusal
    /// itself.
    pub fn restart_refused(&mut self, words: String) {
        let Some(restart) = self.end.as_ref().and_then(|e| e.restart) else {
            return;
        };
        if !self.over() {
            return;
        }
        tracing::info!(call = self.n, %words, "voice: the rebind's next call was refused");
        self.end = Some(CallEnd {
            thread: restart.thread,
            ..ends::refused(words, true)
        });
    }

    /// Talk elsewhere during the call (rule 3): the call moves to `target` at its next quiet point
    /// (at once while a bind is pending; a move asked before the bind waits for it). A target that
    /// binds as this call does calls a move off; a later target replaces an earlier one.
    pub fn switch_to(&mut self, target: CallStart, now: u64) {
        if self.over() {
            return;
        }
        if target.thread == self.start.thread {
            if self.switch.take().is_some() {
                tracing::info!(
                    call = self.n,
                    "voice: the move to another thread is called off"
                );
            }
        } else {
            tracing::info!(call = self.n, to = ?target.thread, "voice: the call moves at its next quiet point");
            self.switch = Some(CallStart {
                new: false,
                ..target
            });
        }
        self.settle(now);
    }

    /// A pending bind holds nothing: a move goes at once, declining it (§7.9).
    pub(super) fn switch_if_pending(&mut self, now: u64) {
        if self.phase != CallPhase::Pending {
            return;
        }
        if let Some(target) = self.switch.take() {
            tracing::info!(call = self.n, to = ?target.thread, "voice: the pending bind is left for another thread");
            self.stale = true;
            self.hang_up(ends::rebind(target), now);
        }
    }

    /// lmgw's end for a thread's call: `out_of_reach` is the thread itself gone.
    pub(super) fn thread_end(&self, end: CallEnd) -> CallEnd {
        if self.by_thread() && end.reason == EndReason::OutOfReach {
            ends::thread_gone(end)
        } else {
            end
        }
    }
}
