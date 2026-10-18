//! One call's rules, sans-IO (design §7): what the phone sends on `oai-events`, what the microphone
//! and Kai's remote track do, when the listening cue plays, and how the call ends. Inputs are the
//! signalling's outcomes, libwebrtc's transport and channel, lmgw's events, the owner's controls and
//! the time (`now`, boot milliseconds); outputs are [`Effect`]s, which the voice actor carries out
//! in order, and the [`VoiceView`] snapshot with the changed [`CaptionLine`]s.
//!
//! - **The microphone** ([`MicState`], §7.5, §7.9): `Live` for the whole bound call in automatic
//!   mode, `Silent` (RTP with silence) for mute, push-to-talk between turns and K16, `Off` while a
//!   bind is pending or expected. Push-to-talk keeps it `Live` for `voice.ptt_tail_ms` after the
//!   release.
//! - **Nothing goes before lmgw's first word**, and while a bind is pending only
//!   `lmgw.session.take_over`; a push-to-talk release before `session.created` is sent right after
//!   it, a mode switch too.
//! - **The stop** (§7.6): Kai's track muted at once, the stats read, `lmgw.response.stop` with the
//!   playout time; every event after it waits behind it, so nothing the owner does next overtakes
//!   it (a `response.create` behind the stop would be dropped by lmgw). The local mute is the
//!   Machine's (`playback_muted`): from a stop or `output_audio_buffer.cleared` to the next
//!   `started`.
//! - **Playout reports** while Kai's audio plays, every `voice.playout_report_ms`.
//! - **The end** (§7.7, §7.8): the owner's End sends the stop and then closes the channel; lmgw's
//!   `lmgw.session.closed` says why lmgw ended it; anything else is "connection lost". The
//!   PeerConnection closes once the channel reports closed, at most `lmgw.timeout_s` after its
//!   close was asked (or after lmgw's word). Closed without the channel's close or lmgw's word, the
//!   call's id is hung up before the next bind: lmgw may still hold it and the thread's binding.
//!
//! Here the call's state, its signalling and transport, the link's facts, the time and the rules
//! inside; the owner's controls in `owner`, the phone's audio focus and headset buttons in
//! `device`, the assistant overlay's dismissal in `dismiss`, lmgw's events in `events`, approvals
//! in the call in `approvals`, the late task result's continuation in `continuation`, the view in
//! `render`, the captions in [`Captions`].

mod approvals;
mod captions;
mod continuation;
mod device;
mod dismiss;
mod events;
mod owner;
mod render;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use lmgw_client::{
    feed::ApprovalRequested,
    realtime::{BoundSession, ClientEvent, Joiner, RevokeKind, Splitter, TurnMode, Updates},
    voice::{Machine, VoiceState},
};

use super::{
    CallStart, MicState, TransportState, ends,
    playout::Jitter,
    view::{CallEnd, CallPhase, CaptionLine},
};
use crate::{
    approvals::{CallVerdicts, Waiting},
    settings::VoiceMode,
};

pub use captions::Captions;

/// The settings a call runs with, read at its start (design §3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub mode: VoiceMode,
    pub connect_timeout_s: u32,
    pub idle_end_s: u32,
    pub playout_report_ms: u32,
    /// `None`: not set (WP0 (g) has not measured it); no tail then, and the view says so.
    pub ptt_tail_ms: Option<u32>,
    /// Half duplex (`voice.echo` `half_duplex`, §8.2): the `session` part carries it with
    /// `echo_tail_ms`.
    pub half_duplex: bool,
    pub echo_tail_ms: Option<u32>,
    /// `lmgw.timeout_s`, in ms: the stop's wait for its stats, the channel's close.
    pub timeout_ms: u64,
}

/// What the voice actor carries out, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// One `oai-events` message (an event, or one of its parts).
    Send(String),
    Mic(MicState),
    /// Kai's remote track audible or muted.
    Playback(bool),
    /// Read the remote audio track's `inbound-rtp` stats; Kotlin answers with `Voice::stats`.
    ReadStats(u64),
    CloseChannel,
    /// Close and dispose of the PeerConnection.
    ClosePeer,
    /// The listening cue: the earcon and the haptic tick.
    Cue,
    /// The call is over for the owner: Kotlin drops the microphone type and the call's audio.
    Ended(CallEnd),
    /// The conversation moves to the thread the call bound (the folder rolled over at the bind).
    FollowThread(i64),
    /// lmgw ended the call because the device was revoked: the link stops as for the feed's
    /// `revoked` (the kind, lmgw's sentence).
    Revoked(RevokeKind, String),
    /// The session showed a call waiting for an approval (the call's thread, no message id): a
    /// card in the link's book (§11).
    ApprovalShown(Box<ApprovalRequested>),
    /// lmgw said another client decided a call this session showed.
    ApprovalDecided(String),
    /// A new call waits for the owner's verdict in this call: the ask earcon and its haptic.
    Asked,
    /// What became of verdicts given to the call.
    Verdicts(CallVerdicts),
}

/// What a stats request is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatsFor {
    Stop,
    Report,
}

/// What waits behind a stop for its stats (an event with the `event_id` it carries, if any).
#[derive(Debug, Clone, PartialEq)]
enum Queued {
    Event(ClientEvent, Option<String>),
    Close,
}

/// One call (module doc).
#[derive(Debug)]
pub struct Call {
    n: u64,
    start: CallStart,
    facts: Facts,
    phase: CallPhase,
    machine: Machine,
    out: Vec<Effect>,
    // The signalling and the channel.
    call_id: Option<String>,
    splitter: Option<Splitter>,
    joiner: Joiner,
    channel_open: bool,
    peer_closed: bool,
    /// The call ended without lmgw ending it (a lost connection, a start given up after its POST,
    /// a rebind): its id is hung up over HTTP (§7.8).
    stale: bool,
    /// lmgw said why it ends the call: nothing more goes to it (calls §8.1).
    lmgw_ended: bool,
    // Deadlines (boot ms).
    connect_by: Option<u64>,
    close_by: Option<u64>,
    tail_until: Option<u64>,
    report_at: Option<u64>,
    stop_by: Option<u64>,
    idle_since: Option<u64>,
    // The cue and the microphone.
    cue: bool,
    expect_pending: bool,
    cue_on_capture: bool,
    mic: Option<MicState>,
    audible: bool,
    muted: bool,
    /// A transient focus loss paused the call (§8.5): muted, until the focus comes back or the
    /// owner unmutes.
    paused: bool,
    /// Another app holds the audio for a while (a transient focus loss, until the focus is back):
    /// the call is not idle then, so the idle clock stands still (§8.5).
    focus_away: bool,
    pressed: bool,
    // The assistant overlay's dismissal (`dismiss`).
    /// The owner's first turn reached the thread with words: a dismissal leaves the call going on.
    spoken: bool,
    /// Speech lmgw heard (or a push-to-talk release) that is not committed or cleared yet.
    speech: bool,
    /// Commits whose transcripts have not come: the turns in flight.
    committing: u32,
    /// The overlay was dismissed while a turn was under way: its words decide.
    dismissed: bool,
    /// Events before lmgw's first word, sent after `session.created`.
    early: Vec<ClientEvent>,
    mode: Option<TurnMode>,
    override_mode: Option<TurnMode>,
    switch_to: Option<TurnMode>,
    updates: Updates,
    others: Vec<String>,
    // The stop and the reports.
    stop_request: Option<u64>,
    held: Vec<Queued>,
    next_request: u64,
    stats_for: HashMap<u64, StatsFor>,
    jitter: Jitter,
    // What the view shows.
    held_by: Option<String>,
    taking_over: bool,
    pending_note: Option<String>,
    thread: Option<(i64, String)>,
    thread_at_bind: Option<i64>,
    conversation_thread: Option<i64>,
    rebind: bool,
    tool: Option<String>,
    hold: bool,
    hold_fallback: Option<String>,
    held_stage: Option<String>,
    error: Option<String>,
    errors: HashMap<String, u32>,
    /// An `error` before the bind: a refusal at the open, which `lmgw.session.closed` follows.
    refusal: Option<String>,
    transport_note: Option<String>,
    end: Option<CallEnd>,
    captions: Captions,
    // Approvals in the call (`approvals`) and the late task result's continuation.
    /// The calls waiting for the owner's verdict, every thread's, as the link's book has them.
    waiting: Vec<Waiting>,
    /// The approval ids this session showed (each asks for attention once).
    asked: HashSet<String>,
    approving: Option<approvals::Approving>,
    /// The phone's `event_id`s so far.
    tags: u64,
    /// A late task result waits for the quiet moment's `response.create`.
    continuation_owed: bool,
    /// The `event_id` of the last continuation's `response.create`.
    continuing: Option<String>,
}

fn ms(s: u32) -> u64 {
    u64::from(s) * 1000
}

impl Call {
    /// Call number `n`, started now by `start` (§7.3 step 1). `expect`: another client holds the
    /// Kai thread's voice as the feed shows it, so the microphone stays `Off` and no cue plays at
    /// DTLS-connected (§7.9). `conversation`: the conversation's thread now.
    pub fn new(
        n: u64,
        start: CallStart,
        facts: Facts,
        expect: Option<String>,
        conversation: Option<(i64, String)>,
        hold: (bool, Option<String>),
        now: u64,
    ) -> Self {
        let override_mode = match facts.mode {
            VoiceMode::Thread => None,
            VoiceMode::Automatic => Some(TurnMode::Automatic),
            VoiceMode::PushToTalk => Some(TurnMode::PushToTalk),
        };
        let mut updates = Updates::default();
        if override_mode.is_some() {
            // The `session` part is the session's first update; `session.updated` answers it.
            updates.sent();
        }
        let mut call = Self {
            n,
            start,
            connect_by: Some(now + ms(facts.connect_timeout_s)),
            facts,
            phase: CallPhase::Connecting,
            machine: Machine::default(),
            out: Vec::new(),
            call_id: None,
            splitter: None,
            joiner: Joiner::new(),
            channel_open: false,
            peer_closed: false,
            stale: false,
            lmgw_ended: false,
            close_by: None,
            tail_until: None,
            report_at: None,
            stop_by: None,
            idle_since: None,
            cue: false,
            expect_pending: expect.is_some(),
            cue_on_capture: false,
            mic: None,
            audible: true,
            muted: false,
            paused: false,
            focus_away: false,
            pressed: false,
            spoken: false,
            speech: false,
            committing: 0,
            dismissed: false,
            early: Vec::new(),
            mode: override_mode.clone(),
            override_mode,
            switch_to: None,
            updates,
            others: Vec::new(),
            stop_request: None,
            held: Vec::new(),
            next_request: 0,
            stats_for: HashMap::new(),
            jitter: Jitter::default(),
            held_by: expect,
            taking_over: false,
            pending_note: None,
            conversation_thread: conversation.as_ref().map(|(id, _)| *id),
            thread: conversation,
            thread_at_bind: None,
            rebind: false,
            tool: None,
            hold: hold.0,
            hold_fallback: hold.1,
            held_stage: None,
            error: None,
            errors: HashMap::new(),
            refusal: None,
            transport_note: None,
            end: None,
            captions: Captions::new(n),
            waiting: Vec::new(),
            asked: HashSet::new(),
            approving: None,
            tags: 0,
            continuation_owed: false,
            continuing: None,
        };
        call.settle(now);
        call
    }

    pub fn number(&self) -> u64 {
        self.n
    }

    pub fn phase(&self) -> CallPhase {
        self.phase
    }

    pub fn start(&self) -> CallStart {
        self.start
    }

    /// The effects to carry out, in order.
    pub fn drain(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.out)
    }

    /// The captions changed since the last call.
    pub fn take_captions(&mut self) -> Vec<CaptionLine> {
        self.captions.take_changed()
    }

    /// The `session` part (§7.4): the owner's turn mode override and half duplex, `None` when it
    /// would carry nothing (the offer then goes as `application/sdp`).
    pub fn session_part(&self) -> Option<BoundSession> {
        let part = BoundSession {
            turn_mode: self.override_mode.clone(),
            half_duplex: self.facts.half_duplex.then_some(true),
            echo_tail_ms: if self.facts.half_duplex {
                self.facts.echo_tail_ms
            } else {
                None
            },
        };
        (part != BoundSession::default()).then_some(part)
    }

    /// The call's id once the POST answered.
    pub fn call_id(&self) -> Option<&str> {
        self.call_id.as_deref()
    }

    /// The id to hang up over HTTP, once the call is over without lmgw having ended it (§7.8).
    pub fn stale_id(&self) -> Option<&str> {
        (self.over() && self.stale)
            .then_some(self.call_id.as_deref())
            .flatten()
    }

    /// Ended for the owner (the PeerConnection may still be closing).
    pub fn over(&self) -> bool {
        matches!(self.phase, CallPhase::Ending | CallPhase::Ended)
    }

    /// Over, and the PeerConnection closed.
    pub fn done(&self) -> bool {
        self.phase == CallPhase::Ended && self.peer_closed
    }

    // ------------------------------------------------------------------ signalling and transport

    /// The POST answered (§7.3 step 3): the id kept, the splitter set to lmgw's message size.
    pub fn answered(&mut self, call_id: String, answer_sdp: &str, now: u64) {
        let size = lmgw_client::realtime::parts::max_message_size(answer_sdp);
        tracing::info!(call = self.n, %call_id, max_message_size = size, "voice: lmgw answered the offer");
        self.call_id = Some(call_id);
        self.splitter = Some(Splitter::new(size));
        if self.over() {
            // Given up while the POST was on its way: lmgw's call is hung up.
            self.stale = true;
        }
        self.settle(now);
    }

    /// The POST failed: the call never started.
    pub fn not_answered(&mut self, end: CallEnd, now: u64) {
        if self.over() {
            return;
        }
        self.lost(end);
        self.settle(now);
    }

    /// libwebrtc's connection state (`detail`: its own name for it).
    pub fn transport(&mut self, state: TransportState, detail: &str, now: u64) {
        match state {
            TransportState::New | TransportState::Connecting => {}
            TransportState::Connected => {
                self.transport_note = None;
                // DTLS connected is the listening cue (§7.2), unless a pending bind is expected.
                if self.phase == CallPhase::Connecting && !self.cue && !self.expect_pending {
                    self.cue = true;
                    self.out.push(Effect::Cue);
                    tracing::info!(call = self.n, "voice: DTLS connected; the listening cue");
                }
            }
            TransportState::Disconnected => {
                if !self.over() {
                    self.transport_note = Some(format!(
                        "the connection is interrupted ({detail}); lmgw keeps the call through a \
                         short pause"
                    ));
                }
            }
            TransportState::Failed | TransportState::Closed => {
                if self.over() {
                    self.close_peer_unconfirmed("the connection closed");
                } else {
                    tracing::info!(call = self.n, %detail, "voice: the connection failed");
                    self.lost(ends::connection_lost(detail));
                }
            }
        }
        self.settle(now);
    }

    /// The phone cannot carry the call on (libwebrtc failed at the answer, the offer or the channel;
    /// Android refused the microphone type): it ends in `words` as a lost call, its id hung up
    /// before the next bind.
    pub fn failed(&mut self, words: String, now: u64) {
        if self.over() {
            self.close_peer_unconfirmed(&words);
        } else {
            tracing::warn!(call = self.n, %words, "voice: the phone cannot carry the call on");
            self.lost(ends::failed(words));
        }
        self.settle(now);
    }

    /// `oai-events` opened.
    pub fn channel_open(&mut self, now: u64) {
        self.channel_open = true;
        self.settle(now);
    }

    /// One message on `oai-events`, in order: parts joined, then parsed (§7.10).
    pub fn channel_message(&mut self, text: String, now: u64) {
        self.message(text, now);
        self.settle(now);
    }

    /// `oai-events` closed.
    pub fn channel_closed(&mut self, why: &str, now: u64) {
        self.channel_open = false;
        if self.over() {
            self.close_peer("the channel closed");
        } else {
            tracing::info!(call = self.n, %why, "voice: the channel closed without lmgw.session.closed");
            self.lost(ends::connection_lost(why));
        }
        self.settle(now);
    }

    // ------------------------------------------------------------------ the link's facts

    /// The conversation's thread and who else answers in it (K16), from the link.
    pub fn conversation(&mut self, thread: Option<i64>, answering: Vec<String>, now: u64) {
        self.conversation_thread = thread;
        let bound = self.thread.as_ref().map(|(id, _)| *id);
        self.others = if self.phase == CallPhase::Live && thread.is_some() && thread == bound {
            answering
        } else {
            Vec::new()
        };
        // The folder's current thread moved on (`folder.current`): rebind at the next quiet point.
        if self.phase == CallPhase::Live
            && thread.is_some()
            && thread != bound
            && thread != self.thread_at_bind
            && !self.rebind
        {
            tracing::info!(
                call = self.n,
                ?thread,
                ?bound,
                "voice: the conversation moved on; the call rebinds at the next quiet point"
            );
            self.rebind = true;
        }
        self.settle(now);
    }

    /// The GPU hold (a badge).
    pub fn hold(&mut self, held: bool, fallback: Option<String>) {
        self.hold = held;
        self.hold_fallback = fallback;
    }

    // ------------------------------------------------------------------ time

    /// The earliest deadline (boot ms).
    pub fn deadline(&self, now: u64) -> Option<u64> {
        let idle = self
            .idle_since
            .filter(|_| self.facts.idle_end_s > 0)
            .map(|t| t + ms(self.facts.idle_end_s));
        let interrupted = self
            .machine
            .next_change(now as f64)
            .map(|t| t.ceil() as u64);
        [
            self.connect_by,
            self.close_by,
            self.tail_until,
            self.report_at,
            self.stop_by,
            idle,
            interrupted,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Acts on what is due.
    pub fn tick(&mut self, now: u64) {
        let connect_due = self.connect_by.is_some_and(|t| now >= t);
        if connect_due {
            self.connect_by = None;
        }
        if connect_due && self.phase == CallPhase::Connecting {
            tracing::info!(
                call = self.n,
                "voice: voice.connect_timeout_s passed before lmgw's first word"
            );
            self.stale = true;
            let end = ends::connect_timeout(self.facts.connect_timeout_s, self.cue);
            if self.channel_open {
                self.out.push(Effect::CloseChannel);
            }
            self.lost(end);
        }
        // The stop before the close: a stop whose stats never came still goes (bare), and the
        // channel's close it holds back is asked only then, with its own wait.
        if self.stop_by.is_some_and(|t| now >= t) {
            self.stop_by = None;
            if self.stop_request.is_some() {
                tracing::warn!(
                    call = self.n,
                    "voice: the playout stats did not come within lmgw.timeout_s; the stop goes without a position (heard = sent)"
                );
                self.send_stop(None, now);
            }
        }
        if self.close_by.is_some_and(|t| now >= t) {
            self.close_by = None;
            if !self.peer_closed {
                tracing::info!(
                    call = self.n,
                    secs = self.facts.timeout_ms / 1000,
                    "voice: the channel did not report closed within lmgw.timeout_s; the PeerConnection closes"
                );
                self.close_peer_unconfirmed("lmgw.timeout_s passed");
            }
        }
        if self.tail_until.is_some_and(|t| now >= t) {
            self.tail_until = None;
        }
        if self.report_at.is_some_and(|t| now >= t) {
            self.report_at = Some(now + u64::from(self.facts.playout_report_ms));
            let r = self.request(StatsFor::Report);
            self.out.push(Effect::ReadStats(r));
        }
        if let Some(since) = self.idle_since
            && self.facts.idle_end_s > 0
            && now >= since + ms(self.facts.idle_end_s)
        {
            self.idle_since = None;
            tracing::info!(call = self.n, "voice: voice.idle_end_s of quiet");
            self.hang_up(ends::idle(self.facts.idle_end_s), now);
        }
        self.settle(now);
    }

    // ------------------------------------------------------------------ inside

    fn effective_mode(&self) -> Option<TurnMode> {
        self.switch_to.clone().or_else(|| self.mode.clone())
    }

    /// A response is open: thinking (a committed turn waits for its response), speaking, a tool.
    fn response_open(&self, now: u64) -> bool {
        self.machine.current().is_some()
            || matches!(
                self.machine.state(now as f64),
                VoiceState::Thinking | VoiceState::Speaking
            )
    }

    fn request(&mut self, what: StatsFor) -> u64 {
        self.next_request += 1;
        self.stats_for.insert(self.next_request, what);
        self.next_request
    }

    /// The stop's first half: the local mute, then the stats (§7.6).
    fn stop(&mut self, now: u64) {
        self.machine.stop_tapped();
        self.sync_playback();
        self.report_at = None;
        if self.stop_request.is_some() {
            return;
        }
        let r = self.request(StatsFor::Stop);
        self.stop_request = Some(r);
        self.stop_by = Some(now + self.facts.timeout_ms);
        self.out.push(Effect::ReadStats(r));
    }

    /// The stop's second half: `lmgw.response.stop`, then what waited behind it.
    fn send_stop(&mut self, playout_ntp_ms: Option<f64>, now: u64) {
        self.stop_request = None;
        self.stop_by = None;
        self.emit(&ClientEvent::ResponseStop { playout_ntp_ms });
        for q in std::mem::take(&mut self.held) {
            match q {
                Queued::Event(ev, None) => self.send(ev),
                Queued::Event(ev, Some(tag)) => self.send_tagged(ev, tag),
                Queued::Close => self.close_channel(now),
            }
        }
    }

    /// Asks for `oai-events`' close; the PeerConnection waits for it at most `lmgw.timeout_s` from
    /// here (never from the hang-up, so a stop that waited for its stats is not cut short).
    fn close_channel(&mut self, now: u64) {
        self.out.push(Effect::CloseChannel);
        self.close_by = Some(now + self.facts.timeout_ms);
    }

    /// An event, by the rules of the module doc.
    fn send(&mut self, ev: ClientEvent) {
        match self.phase {
            CallPhase::Connecting => self.early.push(ev),
            CallPhase::Pending => {
                tracing::debug!(
                    call = self.n,
                    kind = ev.kind(),
                    "voice: held back while the bind is pending"
                );
            }
            CallPhase::Live | CallPhase::Ending if self.stop_request.is_some() => {
                self.held.push(Queued::Event(ev, None))
            }
            CallPhase::Live | CallPhase::Ending => self.emit(&ev),
            _ => {}
        }
    }

    /// An event with an `event_id` an `error` about it echoes; bound calls only (behind a stop as
    /// any event).
    fn send_tagged(&mut self, ev: ClientEvent, tag: String) {
        match self.phase {
            CallPhase::Live if self.stop_request.is_some() => {
                self.held.push(Queued::Event(ev, Some(tag)))
            }
            CallPhase::Live => self.emit_tagged(&ev, Some(&tag)),
            _ => tracing::debug!(
                call = self.n,
                kind = ev.kind(),
                %tag,
                "voice: not sent: the call is not bound"
            ),
        }
    }

    /// The event's messages, split to lmgw's message size (§7.10).
    fn emit(&mut self, ev: &ClientEvent) {
        self.emit_tagged(ev, None);
    }

    /// [`Self::emit`] with `event_id` on the event.
    fn emit_tagged(&mut self, ev: &ClientEvent, tag: Option<&str>) {
        if self.lmgw_ended || self.peer_closed {
            tracing::debug!(
                call = self.n,
                kind = ev.kind(),
                "voice: not sent: the call has ended"
            );
            return;
        }
        let text = match tag {
            None => ev.to_json(),
            Some(tag) => with_event_id(&ev.to_json(), tag),
        };
        let Some(splitter) = self.splitter.as_mut() else {
            tracing::warn!(
                call = self.n,
                kind = ev.kind(),
                "voice: an event before the answer; not sent"
            );
            return;
        };
        match splitter.split(&text) {
            Ok(messages) => {
                tracing::debug!(
                    call = self.n,
                    kind = ev.kind(),
                    parts = messages.len(),
                    "voice: sent"
                );
                self.out.extend(messages.into_iter().map(Effect::Send));
            }
            Err(e) => {
                let words = format!("Kai could not send {}: {e}", ev.kind());
                tracing::warn!(call = self.n, %words, "voice: an event could not be split");
                self.error = Some(words.clone());
                self.captions.note(words);
            }
        }
    }

    /// The phone ends the call (§7.6): the stop and the close behind it on a bound call, the close
    /// alone otherwise; the PeerConnection once the channel reports closed.
    fn hang_up(&mut self, end: CallEnd, now: u64) {
        let live = self.phase == CallPhase::Live;
        if live {
            // The stop's mute and its stats read go before the end is said, which drops the
            // call's audio.
            self.stop(now);
        }
        self.finish(end);
        if live {
            if self.stop_request.is_some() {
                self.held.push(Queued::Close);
            } else {
                self.close_channel(now);
            }
            self.phase = CallPhase::Ending;
        } else if self.channel_open {
            self.close_channel(now);
            self.phase = CallPhase::Ending;
        } else {
            self.close_peer("the call ended before its channel opened");
        }
    }

    /// The call is lost (the transport, the channel, a give-up): the PeerConnection closes at once.
    fn lost(&mut self, end: CallEnd) {
        // lmgw did not end it: its id (once the POST named one) is hung up over HTTP.
        self.stale = true;
        self.finish(end);
        self.close_peer("the call was lost");
    }

    /// Over for the owner: every timer of the call but the stop's stopped, the microphone off, then
    /// the end said once (after a stop's mute and stats read, which the caller pushed first).
    fn finish(&mut self, end: CallEnd) {
        if self.end.is_some() {
            return;
        }
        tracing::info!(call = self.n, reason = ?end.reason, words = ?end.words, "voice: the call ended");
        self.phase = CallPhase::Ended;
        self.end = Some(end.clone());
        self.connect_by = None;
        self.tail_until = None;
        self.report_at = None;
        self.idle_since = None;
        self.pressed = false;
        self.early.clear();
        self.continuation_owed = false;
        self.verdicts_unsettled();
        self.sync_mic();
        self.out.push(Effect::Ended(end));
    }

    /// The PeerConnection closes without the channel's close or lmgw's word (`lmgw.timeout_s`
    /// passed, the connection failed): lmgw may still hold the call and the thread's binding, so
    /// its id is hung up before the next bind (§7.8).
    fn close_peer_unconfirmed(&mut self, why: &str) {
        if !self.peer_closed && !self.lmgw_ended && !self.stale {
            tracing::info!(call = self.n, %why, "voice: the call closes unconfirmed; its id is hung up before the next bind");
            self.stale = true;
        }
        self.close_peer(why);
    }

    fn close_peer(&mut self, why: &str) {
        if self.peer_closed {
            return;
        }
        tracing::debug!(call = self.n, %why, "voice: the PeerConnection closes");
        self.peer_closed = true;
        self.phase = CallPhase::Ended;
        self.close_by = None;
        self.stop_by = None;
        self.stop_request = None;
        self.held.clear();
        self.out.push(Effect::ClosePeer);
    }

    fn desired_mic(&self) -> MicState {
        match self.phase {
            CallPhase::Off | CallPhase::Ending | CallPhase::Ended | CallPhase::Pending => {
                MicState::Off
            }
            CallPhase::Connecting if self.expect_pending => MicState::Off,
            _ => {
                if self.pressed || self.tail_until.is_some() {
                    MicState::Live
                } else if self.muted || self.effective_mode() == Some(TurnMode::PushToTalk) {
                    MicState::Silent
                } else if self.phase == CallPhase::Live && !self.others.is_empty() {
                    // K16: another principal's turn runs in the thread.
                    MicState::Silent
                } else {
                    MicState::Live
                }
            }
        }
    }

    fn sync_mic(&mut self) {
        let mic = self.desired_mic();
        if self.mic != Some(mic) {
            self.mic = Some(mic);
            self.out.push(Effect::Mic(mic));
        }
    }

    fn sync_playback(&mut self) {
        let audible = !self.machine.playback_muted();
        if audible != self.audible {
            self.audible = audible;
            self.out.push(Effect::Playback(audible));
        }
    }

    /// After every input: the microphone, Kai's track, the reports, the idle time, the rebind. The
    /// idle clock stands still while another app holds the audio (a phone call of any length ends
    /// no call by idle) and starts again when the focus is back.
    fn settle(&mut self, now: u64) {
        if self.phase == CallPhase::Live {
            let quiet = self.machine.quiet() && !self.pressed && self.tail_until.is_none();
            if self.rebind && quiet && self.stop_request.is_none() {
                self.rebind = false;
                self.stale = true;
                self.hang_up(ends::rebind(CallStart::default()), now);
            }
        }
        match self.phase {
            CallPhase::Live => {
                // A call of the thread waiting for the owner's verdict is no quiet (§7.7).
                let quiet = self.machine.quiet()
                    && !self.pressed
                    && self.tail_until.is_none()
                    && self.awaiting().is_empty();
                if !quiet || self.focus_away {
                    self.idle_since = None;
                } else if self.idle_since.is_none() {
                    self.idle_since = Some(now);
                }
            }
            CallPhase::Pending => {
                if self.focus_away {
                    self.idle_since = None;
                } else if self.idle_since.is_none() {
                    self.idle_since = Some(now);
                }
            }
            _ => self.idle_since = None,
        }
        self.sync_mic();
        if self.phase == CallPhase::Live {
            self.sync_playback();
            let playing = self.machine.playing().is_some()
                && !self.machine.playback_muted()
                && self.stop_request.is_none();
            if !playing {
                self.report_at = None;
            } else if self.report_at.is_none() {
                self.report_at = Some(now + u64::from(self.facts.playout_report_ms));
            }
        }
        self.continue_if_due(now);
    }
}

/// `text` (one event's JSON object) with `event_id` set.
fn with_event_id(text: &str, tag: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(mut o)) => {
            o.insert("event_id".into(), tag.into());
            serde_json::Value::Object(o).to_string()
        }
        _ => text.to_owned(),
    }
}
