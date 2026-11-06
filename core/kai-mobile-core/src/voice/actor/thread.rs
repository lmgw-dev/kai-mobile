//! The voice task's side of a call's move to another thread (design §7.12, rule 3): Talk elsewhere
//! asks the running call to move, and Stay calls that call's move off. Stay names its call, so one
//! that comes after the move went never touches the next call; it needs nothing of the link, so it
//! is never refused for the link's state as a start is.

use super::VoiceActor;
use crate::{CoreError, voice::CallStart};

impl VoiceActor {
    /// Talk in an opened thread during a call (rule 3): whether a call runs to move.
    pub(super) fn switch(&mut self, bind: Option<i64>, start: CallStart, now: u64) -> bool {
        let Some(s) = self.slot.as_mut().filter(|s| !s.call.over()) else {
            return false;
        };
        let target = CallStart {
            thread: bind,
            ..start
        };
        tracing::info!(call = s.call.number(), thread = ?target.thread, "voice: Talk elsewhere during the call");
        s.call.switch_to(target, now);
        true
    }

    /// Stay for call `call`: its move called off, or the words why there is none to call off.
    pub(super) fn stay(&mut self, call: u64, now: u64) -> Result<(), CoreError> {
        let refused = |reason: &str| CoreError::NotReady {
            reason: reason.to_owned(),
        };
        let Some(s) = self.slot.as_mut() else {
            return Err(refused("No voice call runs."));
        };
        if s.call.number() != call {
            tracing::info!(
                call,
                current = s.call.number(),
                "voice: Stay for another call than the current one; refused"
            );
            return Err(refused("The call moved already; Stay came too late."));
        }
        s.call.stay(now).map_err(|w| refused(&w))
    }
}
