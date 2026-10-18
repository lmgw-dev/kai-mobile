//! Voice over WebRTC (design §7): a call bound by the Kai folder over the device link, its events
//! channel through the core, the microphone's and Kai's track's decisions, the stop and the playout
//! reports, and the end with its reason. Kotlin owns libwebrtc (the PeerConnection, its audio
//! device module, the data channel) and carries out what the core decides through [`EventSink`];
//! the core owns every rule.
//!
//! - [`call`]: one call's rules, sans-IO;
//! - [`signal`]: the offer's POST, the stale call's hangup, the warm connection;
//! - [`actor`]: the voice task, which owns the call and talks to the link's actor;
//! - [`view`]: what Kotlin sees ([`VoiceView`], [`CaptionLine`], [`CallEnd`]).
//!
//! **The flow** (§7.3): Kotlin calls [`Voice::start`] at the trigger (answered at once with the
//! state's words when voice cannot start), builds the PeerConnection and its offer, and hands the
//! offer to [`Voice::offer`], which POSTs it and answers lmgw's SDP. libwebrtc's states and the
//! channel's messages come back through [`Voice::transport`] and the `channel_*` calls, each with
//! the call's number [`Voice::start`] answered, so an older call's PeerConnection (still closing
//! when the next call starts) never reaches the new call; the owner's controls through the rest.

pub mod actor;
pub mod call;
mod ends;
mod playout;
pub mod signal;
pub mod view;

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use self::actor::Cmd;
use crate::{CoreError, link::actor::Cmd as LinkCmd};

pub use view::{
    CallEnd, CallPhase, CaptionLine, EndReason, RingState, Speaker, VoiceAction, VoiceView,
};

/// What starts a call (design §3.2, §7.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
pub struct CallStart {
    /// `new=true`: lmgw rolls the folder's current thread over at the bind (a new conversation).
    pub new: bool,
    /// The owner already chose Take over (an ended call's words): `lmgw.session.take_over` goes as
    /// soon as `bind_pending` arrives.
    pub take_over: bool,
}

/// The microphone (design §7.5, §7.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MicState {
    /// The track enabled.
    Live,
    /// The track disabled: RTP with silence (mute, push-to-talk between turns, K16).
    Silent,
    /// The sender's encoding inactive: no capture, no RTP, the microphone indicator off (a pending
    /// bind).
    Off,
}

/// The remote audio track's `inbound-rtp` stats as libwebrtc reports them (design §3.2), each
/// optional: Kotlin answers every [`EventSink::read_stats`], with empty fields when a stat is
/// missing or the read failed.
#[derive(Debug, Clone, Copy, PartialEq, Default, uniffi::Record)]
pub struct PlayoutStats {
    /// `estimatedPlayoutTimestamp`: the NTP time (ms) of the audio playing now, on lmgw's clock.
    pub estimated_playout_timestamp: Option<f64>,
    /// `jitterBufferDelay`, seconds, cumulative.
    pub jitter_buffer_delay: Option<f64>,
    /// `jitterBufferEmittedCount`, cumulative.
    pub jitter_buffer_emitted_count: Option<u64>,
}

/// The call's audio focus as Android reports it (design §8.5); Kotlin maps Android's codes
/// (a loss that only asks to duck is no loss for a call and is not reported).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AudioFocus {
    /// A permanent loss: another app took the audio for good; the call ends.
    Lost,
    /// A transient loss (a ringing phone call, another app's short focus): Kai stops and the
    /// microphone is muted, paused until the focus comes back; the idle clock stands still.
    LostTransient,
    /// The focus came back: the pause lifts, the mute stays until the owner unmutes, the idle
    /// clock starts again.
    Gained,
}

/// A headset button as the call's MediaSession reads it (design §8.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HeadsetPress {
    /// One press: in push-to-talk the turn's start and, at the next, its commit; in automatic
    /// mode the ring's tap.
    Click,
    /// A long press, where the headset delivers one: End.
    Long,
}

/// libwebrtc's connection state (`PeerConnectionState`): `Connected` is ICE and DTLS up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TransportState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

/// What Kotlin carries out for a call (design §3.2). Called from the core's runtime in order;
/// each method returns at once (Kotlin posts the work to its own thread).
#[uniffi::export(callback_interface)]
pub trait EventSink: Send + Sync {
    /// One `oai-events` message (an event, or one of its parts), as text.
    fn send(&self, text: String);
    fn set_mic(&self, mic: MicState);
    /// Kai's remote track audible (`true`) or muted.
    fn set_playback(&self, audible: bool);
    /// Read the remote audio track's stats; answer with [`Voice::stats`] and the same `request`.
    fn read_stats(&self, request: u64);
    /// Close `oai-events` (End, after the stop).
    fn close_channel(&self);
    /// Close and dispose of the PeerConnection.
    fn close_peer(&self);
    /// The listening cue: the earcon and the haptic tick, as `voice.earcon` and `voice.haptics`
    /// allow.
    fn cue(&self);
    /// A call waits for the owner's verdict in this call (§11): the ask earcon and its haptic, as
    /// `voice.earcon` and `voice.haptics` allow, so an owner listening with the screen off knows to
    /// look.
    fn asked(&self);
    /// The call is over for the owner: drop the microphone type and the call's audio settings
    /// (the PeerConnection closes with [`EventSink::close_peer`]). A [`CallEnd::restart`] starts
    /// the next call once the PeerConnection is closed.
    fn ended(&self, end: CallEnd);
}

/// The voice entry points Kotlin holds (design §3.2), from `Core::voice`. Each sends the voice
/// task a message; `start` and `offer` wait for its answer.
#[derive(uniffi::Object)]
pub struct Voice {
    cmds: mpsc::UnboundedSender<Cmd>,
    link: mpsc::UnboundedSender<LinkCmd>,
}

impl Voice {
    pub fn new(cmds: mpsc::UnboundedSender<Cmd>, link: mpsc::UnboundedSender<LinkCmd>) -> Self {
        Self { cmds, link }
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }

    async fn ticket(&self) -> Result<actor::Ticket, CoreError> {
        let (reply, rx) = oneshot::channel();
        let _ = self.link.send(LinkCmd::VoiceTicket { reply });
        rx.await.unwrap_or_else(|_| Err(stopped()))
    }
}

fn stopped() -> CoreError {
    CoreError::Storage {
        what: "the core".to_owned(),
        reason: "the core stopped".to_owned(),
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Voice {
    /// Opens a connection to the URL that last answered while Kotlin builds the offer (§7.2).
    pub fn prepare(&self) {
        let (reply, rx) = oneshot::channel();
        let _ = self.link.send(LinkCmd::VoiceTicket { reply });
        self.send(Cmd::Prepare { ticket: rx });
    }

    /// A call's trigger (§7.3 step 1): answered at once with the state's words when voice cannot
    /// start (not paired, unconfirmed, the key refused, no network, a call running), else with the
    /// call's number, which every libwebrtc event of this call carries back. The microphone's first
    /// decision reaches `sink` (`Off` when a pending bind is expected, §7.9).
    pub async fn start(
        &self,
        start: CallStart,
        sink: Box<dyn EventSink>,
    ) -> Result<u64, CoreError> {
        let ticket = self.ticket().await?;
        let (reply, rx) = oneshot::channel();
        self.send(Cmd::Start {
            ticket: Box::new(ticket),
            start,
            sink: Arc::from(sink),
            reply,
        });
        rx.await.unwrap_or_else(|_| Err(stopped()))
    }

    /// The offer (no ICE gathering waited for) POSTed by the Kai folder with `takeover=ask` and
    /// the `session` part (§7.3 step 3): lmgw's answer SDP, or why not (the call has ended then).
    /// An offer for another call than the current one is refused, as every per-call event is dropped.
    pub async fn offer(&self, call: u64, sdp: String) -> Result<String, CoreError> {
        let (reply, rx) = oneshot::channel();
        self.send(Cmd::Offer { call, sdp, reply });
        rx.await.unwrap_or_else(|_| Err(stopped()))
    }

    /// Call `call`'s libwebrtc connection state; `detail` is its own name for it (for "connection
    /// lost"). An event of any call but the current one is dropped, here as in the `channel_*`
    /// calls, `capturing` and `stats`.
    pub fn transport(&self, call: u64, state: TransportState, detail: String) {
        self.send(Cmd::Transport {
            call,
            state,
            detail,
        });
    }

    pub fn channel_open(&self, call: u64) {
        self.send(Cmd::ChannelOpen { call });
    }

    /// One `oai-events` message as it arrived; enqueued only (parts are joined in the core).
    pub fn channel_message(&self, call: u64, text: String) {
        self.send(Cmd::ChannelMessage { call, text });
    }

    pub fn channel_closed(&self, call: u64, why: String) {
        self.send(Cmd::ChannelClosed { call, why });
    }

    /// Call `call` cannot go on on the phone (libwebrtc failed at the answer, the offer or the
    /// channel; Android refused the microphone type): it ends with `words`, which say why.
    pub fn failed(&self, call: u64, words: String) {
        self.send(Cmd::Failed { call, words });
    }

    /// The pending bind's Take over (§7.9).
    pub fn take_over(&self) {
        self.send(Cmd::TakeOver);
    }

    /// Push-to-talk pressed (§7.5).
    pub fn press(&self) {
        self.send(Cmd::Press);
    }

    pub fn release(&self) {
        self.send(Cmd::Release);
    }

    /// A tap on the ring: stops Kai while a response is open, else toggles the mute (kai K6).
    pub fn tap(&self) {
        self.send(Cmd::Tap);
    }

    /// Stops Kai (§7.6) whatever is open; idempotent.
    pub fn stop(&self) {
        self.send(Cmd::Stop);
    }

    pub fn set_muted(&self, muted: bool) {
        self.send(Cmd::Muted(muted));
    }

    /// The mode switch during a call (§7.4).
    pub fn set_push_to_talk(&self, on: bool) {
        self.send(Cmd::PushToTalk(on));
    }

    /// The device module's first captured samples after call `call`'s microphone came back from
    /// `Off`.
    pub fn capturing(&self, call: u64) {
        self.send(Cmd::Capturing { call });
    }

    /// Call `call`'s audio focus changed (§8.5): a transient loss stops Kai and mutes, paused
    /// until the focus comes back (the mute stays then) and never idle meanwhile; a permanent loss
    /// ends the call.
    pub fn focus(&self, call: u64, focus: AudioFocus) {
        self.send(Cmd::Focus { call, focus });
    }

    /// A headset button during the call (§8.4).
    pub fn headset(&self, press: HeadsetPress) {
        self.send(Cmd::Headset(press));
    }

    /// Kotlin's answer to [`EventSink::read_stats`] of call `call`.
    pub fn stats(&self, call: u64, request: u64, stats: PlayoutStats) {
        self.send(Cmd::Stats {
            call,
            request,
            stats,
        });
    }

    /// End (§7.6): the stop, then the channel's close; a decline while a bind is pending.
    pub fn end(&self) {
        self.send(Cmd::End);
    }

    /// The assistant overlay that started call `call` was dismissed (§9.2): before the owner's
    /// first turn the call ends (a pending bind is declined), after it the call goes on; a turn
    /// under way decides by its words. Another call's dismissal is dropped.
    pub fn dismissed(&self, call: u64) {
        self.send(Cmd::Dismissed { call });
    }
}
