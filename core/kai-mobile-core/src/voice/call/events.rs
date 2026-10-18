//! lmgw's events on `oai-events` (design §7.3 step 5, §7.6, §7.8, §7.10, §11): every message
//! through lmgw-client's [`Joiner`](lmgw_client::realtime::Joiner) first (a broken part set is
//! shown with its id and the call goes on), then parsed and applied to the call and its Machine. An
//! `error`'s `event_id` (the client event it is about) is read beside the parse.

use lmgw_client::realtime::{
    self, BindPending, CloseKind, ErrorFacts, ServerEvent, SessionClosed, SessionFacts, TurnMode,
};
use lmgw_client::voice::VoiceState;

use super::{Call, Effect, approvals::error_echo};
use crate::voice::{ends, view::CallPhase};

impl Call {
    /// One message as it arrived (module doc).
    pub(super) fn message(&mut self, text: String, now: u64) {
        let joined = self.joiner.push(text);
        for broken in joined.broken {
            let words = broken.to_string();
            tracing::warn!(call = self.n, %words, "voice: a part set lmgw sent cannot be joined");
            self.error = Some(words.clone());
            self.captions.note(words);
        }
        let Some(text) = joined.event else {
            return;
        };
        let echo = error_echo(&text);
        match realtime::parse(&text) {
            Some(ev) => self.event(ev, echo.as_deref(), now),
            None => tracing::warn!(
                call = self.n,
                bytes = text.len(),
                "voice: a message on oai-events that is no event"
            ),
        }
    }

    fn event(&mut self, ev: ServerEvent, echo: Option<&str>, now: u64) {
        if self.over() {
            // The phone ended it, or lmgw said why: nothing after that changes the call. lmgw's
            // word after the phone's End still counts: lmgw let the call go, so its id is not hung
            // up and nothing more goes to it.
            if matches!(ev, ServerEvent::SessionClosed(_)) && self.phase == CallPhase::Ending {
                tracing::info!(
                    call = self.n,
                    "voice: lmgw ended the call while it was ending"
                );
                self.stale = false;
                self.lmgw_ended = true;
                return;
            }
            tracing::debug!(
                call = self.n,
                "voice: an event after the call's end; ignored"
            );
            return;
        }
        match ev {
            ServerEvent::SessionCreated(f) => self.session_created(f, now),
            ServerEvent::SessionUpdated(f) => {
                // The echo is lmgw's word on the mode only once no update of the phone's is in
                // flight (an older echo would undo a newer switch).
                if self.updates.echo()
                    && self.phase == CallPhase::Live
                    && let Some(m) = f.turn_mode
                {
                    self.mode = Some(m);
                }
            }
            ServerEvent::BindPending(b) => self.bind_pending(b, now),
            ServerEvent::SessionClosed(c) => self.session_closed(c, now),
            ServerEvent::SpeechStarted => {
                self.machine.speech_started();
                self.turn_started();
            }
            ServerEvent::SpeechStopped => self.machine.speech_stopped(),
            ServerEvent::Committed => {
                self.machine.committed();
                self.turn_committed();
            }
            ServerEvent::Cleared => {
                self.machine.cleared();
                self.turn_cleared(now);
            }
            ServerEvent::TranscriptDelta(d) => self.captions.owner_delta(&d),
            ServerEvent::Transcript(t) => {
                self.captions.owner_final(&t);
                self.turn_words(!t.trim().is_empty(), now);
            }
            ServerEvent::TranscriptFailed(e) => {
                self.machine.turn_failed();
                self.on_error(e, now);
                self.turn_words(false, now);
            }
            ServerEvent::ResponseCreated { response_id } => {
                self.machine.response_created(&response_id);
                self.verdicts_response(&response_id);
            }
            ServerEvent::SpokenDelta { response_id, delta } => {
                self.verdicts_output(&response_id);
                self.captions.kai_delta(&response_id, &delta)
            }
            ServerEvent::ResponseDone {
                response_id,
                status,
                reason,
            } => {
                self.verdicts_done(&response_id, &status);
                if status == "cancelled" {
                    self.machine
                        .response_cancelled(&response_id, reason.as_deref());
                } else {
                    self.machine.response_done(&response_id);
                    if status == "failed" {
                        let words = format!(
                            "the reply failed: {}",
                            reason.unwrap_or_else(|| "lmgw gave no reason".to_owned())
                        );
                        self.error = Some(words.clone());
                        self.captions.note(words);
                        self.turn_words(false, now);
                    } else {
                        // Kai answered: the conversation is under way.
                        self.reply_completed(now);
                    }
                }
                self.set_tool(None);
            }
            ServerEvent::ChatFrame {
                response_id,
                event,
                data,
            } => {
                // The chat turn runs: what it frames is output (a resumed turn's frames come only
                // after lmgw decided the calls).
                self.verdicts_output(&response_id);
                match event.as_str() {
                    // The running tool shows on the state line; once its preamble played the ring
                    // thinks (chat-voice §8.5).
                    "tool" => match data["event"].as_str() {
                        Some("start" | "ready") => {
                            let name = data["name"].as_str().unwrap_or("a tool").to_owned();
                            self.set_tool(Some(name));
                        }
                        Some("result") => self.set_tool(None),
                        _ => {}
                    },
                    "done" => self.set_tool(None),
                    _ => {}
                }
            }
            ServerEvent::ChatReply(r) => self.captions.kai_reply(
                r.message_id,
                r.content.as_deref(),
                r.unheard.as_deref(),
                r.removed,
            ),
            ServerEvent::ModelState(m) => {
                if m.state == "held" {
                    let cause = m.cause.map(|c| format!(" ({c})")).unwrap_or_default();
                    self.held_stage = Some(format!("{} {}{cause}", m.stage, m.alias));
                } else if self
                    .held_stage
                    .as_deref()
                    .is_some_and(|h| h.starts_with(&format!("{} ", m.stage)))
                {
                    self.held_stage = None;
                }
            }
            ServerEvent::Timing(t) => {
                let fallback = t
                    .models
                    .chat
                    .as_ref()
                    .and_then(|c| c.answered_by.clone().filter(|a| *a != c.alias));
                self.captions
                    .timing(t.response_id.as_deref(), t.message_id, fallback.as_deref());
            }
            ServerEvent::Thread(t) => {
                if let Some((id, title)) = self.thread.as_mut()
                    && *id == t.id
                {
                    *title = t.title;
                }
            }
            ServerEvent::Error(e) => {
                // Verdicts lmgw refused go back to their cards (or to the route) and are the call's
                // error too; a continuation with nothing to answer is no error.
                self.verdicts_refused(&e, echo);
                if !self.continuation_refused(&e, echo) {
                    self.on_error(e, now);
                }
            }
            ServerEvent::OutputAudioBufferStarted { response_id } => {
                self.verdicts_output(&response_id);
                self.machine.output_started(&response_id)
            }
            ServerEvent::OutputAudioBufferStopped { response_id } => {
                self.machine.output_stopped(&response_id)
            }
            ServerEvent::OutputAudioBufferCleared { response_id } => {
                // lmgw stopped sending it (a barge-in, a stop): the phone silences what its jitter
                // buffer still holds, until the next `started` (§7.6).
                self.machine.output_cleared(&response_id, now as f64)
            }
            ServerEvent::Starved(s) => self.machine.starved(&s.response_id, s.starved),
            ServerEvent::ApprovalRequest(a) => self.approval_shown(a),
            ServerEvent::ApprovalDecided(d) => self.approval_decided(d),
            ServerEvent::TaskDone(d) => self.task_done(d),
            ServerEvent::Unknown { kind, .. } => {
                tracing::debug!(call = self.n, %kind, "voice: an event this build does not act on")
            }
            _ => {}
        }
    }

    fn set_tool(&mut self, tool: Option<String>) {
        self.machine.set_tool(tool.is_some());
        self.tool = tool;
    }

    /// `session.created`: bound (§7.3 step 5).
    fn session_created(&mut self, f: SessionFacts, now: u64) {
        if !matches!(self.phase, CallPhase::Connecting | CallPhase::Pending) {
            tracing::debug!(
                call = self.n,
                "voice: session.created outside the bind; ignored"
            );
            return;
        }
        let mic_was_off = self.mic == Some(crate::voice::MicState::Off);
        self.phase = CallPhase::Live;
        self.machine.session_started();
        self.connect_by = None;
        self.taking_over = false;
        self.pending_note = None;
        self.expect_pending = false;
        self.refusal = None;
        if let Some(t) = f.thread {
            tracing::info!(call = self.n, thread = t.id, "voice: bound");
            self.thread_at_bind = self.conversation_thread;
            if self.conversation_thread != Some(t.id) {
                // The folder rolled over at the bind: the conversation follows (§6.2).
                self.out.push(Effect::FollowThread(t.id));
            }
            self.thread = Some((t.id, t.title));
        }
        self.mode = self
            .override_mode
            .clone()
            .or(f.turn_mode)
            .or(Some(TurnMode::Automatic));
        if let Some(m) = self.switch_to.take()
            && self.mode.as_ref() != Some(&m)
        {
            self.updates.sent();
            self.mode = Some(m.clone());
            self.send(realtime::ClientEvent::BoundSessionUpdate(
                realtime::BoundSession::turn_mode(m),
            ));
        }
        if self.pressed {
            self.machine.ptt_down();
        }
        for ev in std::mem::take(&mut self.early) {
            self.send(ev);
        }
        if mic_was_off {
            // The microphone comes back: the cue waits for its first samples (§7.9).
            self.cue_on_capture = true;
        }
        let _ = now;
    }

    /// `lmgw.session.bind_pending {held_by}` (§7.9): the microphone off, the prompt; again with a
    /// new holder.
    fn bind_pending(&mut self, b: BindPending, now: u64) {
        if !matches!(self.phase, CallPhase::Connecting | CallPhase::Pending) {
            return;
        }
        let first = self.phase == CallPhase::Connecting;
        tracing::info!(call = self.n, held_by = %b.held_by, first, "voice: the bind is pending");
        self.phase = CallPhase::Pending;
        self.machine.bind_pending(&b.held_by);
        let new_holder = self.held_by.as_deref() != Some(b.held_by.as_str());
        self.held_by = Some(b.held_by);
        self.connect_by = None;
        if first {
            if self.cue {
                self.pending_note =
                    Some("what you said since the listening cue was not heard".to_owned());
            }
            self.pressed = false;
            self.tail_until = None;
            if !self.early.is_empty() {
                tracing::debug!(
                    call = self.n,
                    n = self.early.len(),
                    "voice: what waited for the bind is dropped"
                );
                self.early.clear();
            }
            if self.start.take_over {
                // The owner chose already (an ended call's Take over).
                self.take_over(now);
            }
        } else if new_holder {
            // A third client took the thread meanwhile: the prompt asks about the new holder.
            self.taking_over = false;
        }
    }

    /// `lmgw.session.closed` (§7.8): why lmgw ends the call.
    fn session_closed(&mut self, c: SessionClosed, now: u64) {
        let close = realtime::call_close(&c);
        let end = ends::closed(&close, c.kind.as_str(), self.refusal.as_deref());
        if let CloseKind::Revoked { kind } = &close.kind {
            self.out
                .push(Effect::Revoked(kind.clone(), close.message.clone()));
        }
        // lmgw ended it itself: no stale id, nothing more goes to it (a stop still waiting for its
        // stats included), and it closes the channel; the PeerConnection waits for that at most
        // `lmgw.timeout_s`.
        self.stale = false;
        self.lmgw_ended = true;
        self.finish(end);
        self.close_by = Some(now + self.facts.timeout_ms);
    }

    /// An `error` (§7.10): shown verbatim with its code; `bind_pending` is a bug in Kai. An
    /// approval's is logged by its code alone: lmgw's words name the tools.
    fn on_error(&mut self, e: ErrorFacts, now: u64) {
        let code = e.code.clone().unwrap_or_else(|| e.kind.clone());
        if code == "bind_pending" {
            tracing::warn!(call = self.n, message = %e.message, "voice: lmgw answered an event with bind_pending: a bug in Kai (only take_over goes while the bind is pending)");
            return;
        }
        let words = format!("{} ({code})", e.message);
        let n = self.errors.entry(code.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            tracing::info!(call = self.n, %code, times = *n, "voice: an error again");
        } else if code.starts_with("approval_") {
            tracing::info!(call = self.n, %code, "voice: lmgw sent an error");
        } else {
            tracing::info!(call = self.n, %words, "voice: lmgw sent an error");
        }
        if self.phase != CallPhase::Live {
            // Before the bind: a refusal at the open, which lmgw.session.closed follows (C21).
            self.refusal = Some(words.clone());
        } else if self.machine.current().is_none()
            && self.machine.state(now as f64) == VoiceState::Thinking
        {
            // A committed turn that will get no response (`empty_turn`, a hold without fallback).
            self.machine.turn_failed();
            self.turn_words(false, now);
        }
        self.error = Some(words.clone());
        self.captions.note(words);
    }
}
