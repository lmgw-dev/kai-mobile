//! The owner's controls on a call (design §7.5, §7.6, §7.9): Take over, push-to-talk, the tap and
//! the stop, mute, the mode switch, the delayed cue's first samples, the stats' answer, End.

use lmgw_client::realtime::{BoundSession, ClientEvent, TurnMode};

use super::{Call, Effect, StatsFor};
use crate::voice::{PlayoutStats, ends, view::CallPhase};

impl Call {
    /// Take over (§7.9): the one event a pending bind takes.
    pub fn take_over(&mut self, now: u64) {
        if self.phase == CallPhase::Pending && !self.taking_over {
            self.taking_over = true;
            self.emit(&ClientEvent::TakeOver);
        }
        self.settle(now);
    }

    /// Push-to-talk pressed (§7.5): a stop first when a response is open, then the input
    /// buffer's clear and the track on. Ignored when muted, in automatic mode, or not connected.
    pub fn press(&mut self, now: u64) {
        if !matches!(self.phase, CallPhase::Connecting | CallPhase::Live) || self.pressed {
            return;
        }
        if self.muted {
            tracing::debug!(
                call = self.n,
                "voice: push-to-talk pressed while muted; ignored"
            );
            return;
        }
        if self.effective_mode() != Some(TurnMode::PushToTalk) {
            tracing::debug!(
                call = self.n,
                "voice: push-to-talk pressed in automatic mode; ignored"
            );
            return;
        }
        if self.response_open(now) {
            self.stop(now);
        }
        self.pressed = true;
        self.tail_until = None;
        if self.phase == CallPhase::Live {
            self.machine.ptt_down();
            self.send(ClientEvent::Clear);
        }
        // Before `session.created` no clear goes: lmgw's buffer holds the audio since the first
        // frame, and the press's own start is in it.
        self.settle(now);
    }

    /// Push-to-talk released: the commit and `response.create` at once (lmgw's commit window takes
    /// the turn's last frames), the track on for the tail.
    pub fn release(&mut self, now: u64) {
        if !self.pressed {
            return;
        }
        self.pressed = false;
        if self.phase == CallPhase::Live {
            self.machine.ptt_up(true);
        }
        self.send(ClientEvent::Commit);
        self.send(ClientEvent::ResponseCreate);
        // The owner's own turn: a dismissal of the overlay waits for its words (`dismiss`).
        self.turn_started();
        if let Some(t) = self.facts.ptt_tail_ms.filter(|t| *t > 0) {
            self.tail_until = Some(now + u64::from(t));
        }
        self.settle(now);
    }

    /// A tap on the ring (§7.6, kai K6): stops Kai while a response is open, else toggles the mute.
    pub fn tap(&mut self, now: u64) {
        match self.phase {
            CallPhase::Live if self.response_open(now) => self.stop(now),
            CallPhase::Live | CallPhase::Connecting => {
                let m = !self.muted;
                self.set_muted(m, now);
            }
            _ => {}
        }
        self.settle(now);
    }

    /// The stop alone (the screen's stop button): idempotent, as lmgw's event is.
    pub fn stop_kai(&mut self, now: u64) {
        if self.phase == CallPhase::Live {
            self.stop(now);
        }
        self.settle(now);
    }

    /// Mute (§7.5): the track silent, so an open turn ends in lmgw's detector.
    pub fn set_muted(&mut self, muted: bool, now: u64) {
        if self.over() {
            return;
        }
        if muted && self.pressed {
            self.release(now);
        }
        if !muted {
            // The owner's unmute lifts a focus pause too: it is the owner's choice (§8.5).
            self.paused = false;
        }
        self.muted = muted;
        self.settle(now);
    }

    /// The mode switch (§7.4): `session.update` naming `session.lmgw.turn_mode`; held back until
    /// `session.created`.
    pub fn set_push_to_talk(&mut self, on: bool, now: u64) {
        let want = if on {
            TurnMode::PushToTalk
        } else {
            TurnMode::Automatic
        };
        match self.phase {
            CallPhase::Live => {
                if self.mode.as_ref() != Some(&want) {
                    if !on && self.pressed {
                        self.release(now);
                    }
                    self.send(ClientEvent::BoundSessionUpdate(BoundSession::turn_mode(
                        want.clone(),
                    )));
                    self.updates.sent();
                    self.mode = Some(want);
                }
            }
            CallPhase::Connecting | CallPhase::Pending => self.switch_to = Some(want),
            _ => {}
        }
        self.settle(now);
    }

    /// The device module's first captured samples after the microphone came back from `Off`:
    /// the delayed listening cue (§7.9).
    pub fn capturing(&mut self, now: u64) {
        if self.cue_on_capture && self.phase == CallPhase::Live {
            self.cue_on_capture = false;
            self.cue = true;
            self.out.push(Effect::Cue);
            tracing::info!(
                call = self.n,
                "voice: the first samples after the bind; the listening cue"
            );
        }
        self.settle(now);
    }

    /// Kotlin's answer to [`Effect::ReadStats`].
    pub fn stats(&mut self, request: u64, stats: PlayoutStats, now: u64) {
        match self.stats_for.remove(&request) {
            Some(StatsFor::Stop) if self.stop_request == Some(request) => {
                self.send_stop(stats.estimated_playout_timestamp, now);
            }
            Some(StatsFor::Report)
                if self.phase == CallPhase::Live
                    && self.stop_request.is_none()
                    && !self.machine.playback_muted() =>
            {
                let jb = self
                    .jitter
                    .report(stats.jitter_buffer_delay, stats.jitter_buffer_emitted_count);
                if let Some(ev) = ClientEvent::playout(stats.estimated_playout_timestamp, jb) {
                    self.send(ev);
                }
            }
            _ => {}
        }
        self.settle(now);
    }

    /// The owner's End (§7.6): the stop and the channel's close on a bound call, the close alone
    /// while a bind is pending (a decline), the PeerConnection closed before the channel opened.
    pub fn end(&mut self, now: u64) {
        match self.phase {
            CallPhase::Live => self.hang_up(ends::owner(), now),
            CallPhase::Pending => self.hang_up(ends::declined(), now),
            CallPhase::Connecting => {
                self.stale = true;
                self.hang_up(ends::owner(), now)
            }
            _ => {}
        }
        self.settle(now);
    }
}
