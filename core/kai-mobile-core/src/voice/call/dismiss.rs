//! The assistant overlay's dismissal (design §7.7's "dismissed early", §9.2): before the owner's
//! first turn it ends the call the overlay started, so an accidental long press leaves no open
//! microphone; after it the call goes on in the background, so a conversation the owner started
//! survives a glance at another app.
//!
//! **The first turn** is the owner's first turn that reached the thread with words: a final
//! transcript that is not blank, or a reply Kai completed. A commit of noise or silence (lmgw's
//! `empty_turn`, a failed transcript) is no turn. Turns under way when the overlay goes are counted:
//! one opens when lmgw hears speech start (or a push-to-talk release asks for the commit), becomes a
//! commit in flight at `committed`, and is resolved by its transcript (or `cleared`, when the speech
//! never became a commit). With the first words the call goes on; the call ends as dismissed once
//! no turn is open and none had words.
//!
//! The rule is the core's, not the overlay's: the overlay's view of the call can be a moment behind
//! a commit, and the core sees every event in order.

use super::Call;
use crate::voice::{ends, view::CallPhase};

impl Call {
    /// The overlay that started this call was dismissed (back, a swipe down, a tap outside, Home,
    /// the screen going off): ends the call before the owner's first turn, or once a turn under way
    /// turns out to have no words; nothing after the first turn. A pending bind is declined.
    pub fn dismissed(&mut self, now: u64) {
        match self.phase {
            CallPhase::Connecting => {
                tracing::info!(
                    call = self.n,
                    "voice: the overlay was dismissed before the bind; the call ends"
                );
                // As the owner's End before the bind: lmgw may hold the call already.
                self.stale = true;
                self.hang_up(ends::dismissed(), now);
            }
            CallPhase::Pending => {
                tracing::info!(
                    call = self.n,
                    "voice: the overlay was dismissed while the bind was pending; declined"
                );
                self.hang_up(ends::declined(), now);
            }
            CallPhase::Live if self.spoken => tracing::info!(
                call = self.n,
                "voice: the overlay was dismissed after the owner's first turn; the call goes on"
            ),
            CallPhase::Live if self.turn_under_way() => {
                tracing::info!(
                    call = self.n,
                    "voice: the overlay was dismissed while a turn was under way; its words decide"
                );
                self.dismissed = true;
            }
            CallPhase::Live => {
                tracing::info!(
                    call = self.n,
                    "voice: the overlay was dismissed before the owner's first turn; the call ends"
                );
                self.hang_up(ends::dismissed(), now);
            }
            CallPhase::Off | CallPhase::Ending | CallPhase::Ended => {}
        }
        self.settle(now);
    }

    /// Whether a turn of the owner's is under way: speech lmgw heard that is not committed or
    /// cleared yet (or a push-to-talk release waiting for lmgw's `committed`), a commit whose
    /// transcript has not come, or the ring held.
    pub(super) fn turn_under_way(&self) -> bool {
        self.speech || self.committing > 0 || self.pressed
    }

    /// lmgw heard the owner start to speak, or a push-to-talk release asked for the commit: a turn
    /// opens (until its `committed`, or its `cleared` when nothing is committed).
    pub(super) fn turn_started(&mut self) {
        self.speech = true;
    }

    /// lmgw committed the turn: its transcript is what decides now. One commit is one turn, so two
    /// in flight count as two.
    pub(super) fn turn_committed(&mut self) {
        self.speech = false;
        self.committing += 1;
    }

    /// lmgw cleared the input buffer: speech that never became a commit is no turn, and has no words.
    pub(super) fn turn_cleared(&mut self, now: u64) {
        if self.speech {
            self.speech = false;
            self.turn_resolved(now);
        }
    }

    /// A commit's transcript came (`words`: not blank), or its transcript or reply failed: one open
    /// turn has its outcome.
    pub(super) fn turn_words(&mut self, words: bool, now: u64) {
        self.committing = self.committing.saturating_sub(1);
        if words {
            self.first_turn();
        }
        self.turn_resolved(now);
    }

    /// Kai completed a reply: the conversation is under way, whatever turns are still open.
    pub(super) fn reply_completed(&mut self, now: u64) {
        self.first_turn();
        self.turn_resolved(now);
    }

    fn first_turn(&mut self) {
        if !self.spoken {
            tracing::debug!(call = self.n, "voice: the owner's first turn");
            self.spoken = true;
        }
    }

    /// A dismissal that waited acts once no turn is open: with the first turn's words the call goes
    /// on, without any it ends as dismissed. (Relies on lmgw resolving every committed turn with a
    /// transcript, a failed transcript or a failed reply.)
    fn turn_resolved(&mut self, now: u64) {
        if !self.dismissed {
            return;
        }
        if self.spoken {
            self.dismissed = false;
            tracing::info!(
                call = self.n,
                "voice: the turn under way at the overlay's dismissal had words; the call goes on"
            );
        } else if self.phase == CallPhase::Live && !self.turn_under_way() {
            self.dismissed = false;
            tracing::info!(
                call = self.n,
                "voice: the turns under way at the overlay's dismissal had no words; the call ends"
            );
            self.hang_up(ends::dismissed(), now);
        }
    }
}
