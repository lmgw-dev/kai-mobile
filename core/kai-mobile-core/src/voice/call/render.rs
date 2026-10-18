//! The call's snapshot for Kotlin (design §7.1): the ring's state, the state line's words, the
//! facts beside them.

use lmgw_client::{realtime::TurnMode, voice::VoiceState};

use super::Call;
use crate::voice::view::{CallPhase, RingState, VoiceView};

/// The state line while another app holds the audio (§8.5).
const PAUSED: &str = "Paused: another app took the audio";

/// The state line while the bound thread's calls wait for the owner's verdict (§11).
fn waiting_words(tools: &[String]) -> String {
    match tools {
        [one] => format!("Kai waits for your decision on {one}"),
        many => format!("Kai waits for your decision on {} tool calls", many.len()),
    }
}

impl Call {
    pub fn view(&self, now: u64) -> VoiceView {
        let ptt = self.effective_mode() == Some(TurnMode::PushToTalk);
        let state = self.machine.state(now as f64);
        let waiting_for = (self.phase == CallPhase::Live && !ptt)
            .then(|| self.others.first().cloned())
            .flatten();
        let ring = match self.phase {
            CallPhase::Off | CallPhase::Ending | CallPhase::Ended => RingState::Rest,
            CallPhase::Connecting if !self.cue => RingState::Connecting,
            CallPhase::Connecting if self.muted => RingState::Muted,
            CallPhase::Connecting if ptt && !self.pressed => RingState::Idle,
            CallPhase::Connecting => RingState::Listening,
            CallPhase::Pending => RingState::Pending,
            CallPhase::Live if self.muted && !self.pressed => RingState::Muted,
            CallPhase::Live => match state {
                VoiceState::Interrupted => RingState::Interrupted,
                VoiceState::Listening => RingState::Listening,
                VoiceState::Thinking => RingState::Thinking,
                VoiceState::Speaking => RingState::Speaking,
                VoiceState::Idle if ptt || waiting_for.is_some() => RingState::Idle,
                VoiceState::Idle if self.cue_on_capture => RingState::Connecting,
                VoiceState::Idle => RingState::Listening,
            },
        };
        let holder = self.held_by.clone().unwrap_or_default();
        let awaiting = if self.phase == CallPhase::Live {
            self.awaiting()
        } else {
            Vec::new()
        };
        let words = match self.phase {
            CallPhase::Off => String::new(),
            CallPhase::Connecting if self.expect_pending => {
                format!("Connecting to lmgw… (voice is on {holder})")
            }
            CallPhase::Connecting if self.cue && self.paused => PAUSED.to_owned(),
            CallPhase::Connecting if self.cue && self.muted => "Muted".to_owned(),
            CallPhase::Connecting if self.cue && self.pressed => "Talking".to_owned(),
            CallPhase::Connecting if self.cue && ptt => "Hold to talk".to_owned(),
            CallPhase::Connecting if self.cue => "Listening".to_owned(),
            CallPhase::Connecting => "Connecting to lmgw…".to_owned(),
            CallPhase::Pending if self.taking_over => format!("Taking voice over from {holder}…"),
            CallPhase::Pending => format!("Voice is on {holder}"),
            CallPhase::Live => match ring {
                RingState::Muted if self.paused => PAUSED.to_owned(),
                // Kai waits for a verdict (§11): said over listening, idle and muted alike, since
                // the owner's words would decline it.
                RingState::Listening | RingState::Idle | RingState::Muted
                    if !awaiting.is_empty() && !self.pressed =>
                {
                    waiting_words(&awaiting)
                }
                RingState::Muted => "Muted".to_owned(),
                RingState::Idle if waiting_for.is_some() => {
                    format!("Waiting for {}", waiting_for.clone().unwrap_or_default())
                }
                RingState::Idle => "Hold to talk".to_owned(),
                RingState::Connecting => "Starting the microphone…".to_owned(),
                RingState::Listening if self.pressed => "Talking".to_owned(),
                RingState::Listening => "Listening".to_owned(),
                RingState::Thinking => match &self.tool {
                    Some(t) => format!("Running {t}"),
                    None => "Thinking".to_owned(),
                },
                RingState::Speaking => "Speaking".to_owned(),
                RingState::Interrupted => "Interrupted".to_owned(),
                _ => String::new(),
            },
            CallPhase::Ending => "Ending…".to_owned(),
            CallPhase::Ended => self
                .end
                .as_ref()
                .and_then(|e| e.words.clone())
                .unwrap_or_else(|| "Voice ended".to_owned()),
        };
        let note = match (&self.transport_note, ptt, self.facts.ptt_tail_ms) {
            (Some(t), _, _) => Some(t.clone()),
            (None, true, None) => Some(
                "push-to-talk keeps no tail after the release: voice.ptt_tail_ms is not set (WP0 \
                 (g) measures it), so the last syllable may be cut"
                    .to_owned(),
            ),
            _ => None,
        };
        VoiceView {
            call: self.n,
            phase: self.phase,
            ring,
            words,
            cue: self.cue,
            spoken: self.spoken,
            push_to_talk: ptt,
            muted: self.muted,
            paused: self.paused,
            half_duplex: self.facts.half_duplex,
            pressed: self.pressed,
            kai_muted: self.phase == CallPhase::Live && self.machine.playback_muted(),
            stoppable: self.phase == CallPhase::Live && self.response_open(now),
            held_by: (self.phase == CallPhase::Pending)
                .then(|| self.held_by.clone())
                .flatten(),
            taking_over: self.taking_over,
            pending_note: self.pending_note.clone(),
            waiting_for,
            thread_id: self.thread.as_ref().map(|(id, _)| *id),
            thread_title: self.thread.as_ref().map(|(_, t)| t.clone()),
            tool: self.tool.clone(),
            awaiting,
            hold: self.hold,
            hold_fallback: self.hold_fallback.clone(),
            held_stage: self.held_stage.clone(),
            error: self.error.clone(),
            note,
            end: self.end.clone(),
        }
    }
}
