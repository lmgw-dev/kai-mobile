//! The phone's audio around a call (design §8.4, §8.5): the audio focus and the headset's buttons,
//! as Kotlin reads them from Android (the call's MediaSession, its focus request). The rules are
//! the call's: what a loss stops and mutes, what a press does in each mode.

use lmgw_client::realtime::TurnMode;

use super::Call;
use crate::voice::{AudioFocus, HeadsetPress, ends, view::CallPhase};

impl Call {
    /// The call's audio focus (§8.5):
    /// - **a transient loss** (a ringing phone call, another app's short focus) mutes, releasing a
    ///   held push-to-talk press first (its turn is committed, as a mute does), then stops Kai
    ///   (§7.6's stop, which also cancels that turn's owed response): nothing of Kai plays into the
    ///   other app's audio. The ring says the call is paused, and the idle clock stands still
    ///   until the focus is back (the call is not idle while a phone call runs, however long);
    /// - **the focus back** lifts the pause, starts the idle clock again from now and leaves the
    ///   mute on until the owner unmutes (no surprise listening);
    /// - **a permanent loss** ends the call ("voice ended: another app took the audio"), by the
    ///   stop and the channel's close as the owner's End does.
    ///
    /// lmgw's word and a failure end a paused call as any other.
    pub fn focus(&mut self, focus: AudioFocus, now: u64) {
        if self.over() {
            self.settle(now);
            return;
        }
        match focus {
            AudioFocus::LostTransient => {
                tracing::info!(
                    call = self.n,
                    "voice: another app took the audio for a while; Kai stops and the microphone is muted"
                );
                self.set_muted(true, now);
                if self.phase == CallPhase::Live {
                    self.stop(now);
                }
                self.paused = true;
                self.focus_away = true;
            }
            AudioFocus::Gained => {
                if self.paused {
                    tracing::info!(
                        call = self.n,
                        "voice: the audio focus is back; the mute stays until the owner unmutes"
                    );
                    self.paused = false;
                }
                if self.focus_away {
                    // The idle clock starts again from the focus's return.
                    self.focus_away = false;
                    self.idle_since = None;
                }
            }
            AudioFocus::Lost => {
                tracing::info!(
                    call = self.n,
                    "voice: another app took the audio; the call ends"
                );
                if self.phase == CallPhase::Connecting {
                    // Given up before lmgw's first word: its id is hung up before the next bind.
                    self.stale = true;
                }
                self.hang_up(ends::audio_taken(), now);
            }
        }
        self.settle(now);
    }

    /// A headset button (§8.4): in push-to-talk a press starts a turn and the next one commits it
    /// (a toggle: a headset delivers no reliable hold); in automatic mode it is the ring's tap
    /// (stops Kai while a response is open, else toggles the mute, kai K6); a long press ends the
    /// call.
    pub fn headset(&mut self, press: HeadsetPress, now: u64) {
        match press {
            HeadsetPress::Long => {
                tracing::info!(
                    call = self.n,
                    "voice: the headset's long press ends the call"
                );
                self.end(now);
            }
            HeadsetPress::Click => {
                let ptt = self.effective_mode() == Some(TurnMode::PushToTalk);
                if ptt && matches!(self.phase, CallPhase::Connecting | CallPhase::Live) {
                    if self.pressed {
                        self.release(now);
                    } else {
                        self.press(now);
                    }
                } else {
                    self.tap(now);
                }
            }
        }
    }
}
