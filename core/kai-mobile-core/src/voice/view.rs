//! What Kotlin sees of a call (design §3.2, §7.1): the call's snapshot ([`VoiceView`], published as
//! `Update::Voice`), its captions as lines changed by id ([`CaptionLine`], `Update::Caption`), and
//! how it ended ([`CallEnd`]).

use super::CallStart;

/// Where a call stands (design §7.1: Kai's own states beside the Machine's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CallPhase {
    /// No call yet in this process.
    Off,
    /// From the trigger to lmgw's first word on the channel (`session.created` or
    /// `lmgw.session.bind_pending`); the listening cue plays at DTLS-connected within it.
    Connecting,
    /// The call is connected and holds no thread: "Voice is on <held_by>" with Take over (§7.9).
    Pending,
    /// Bound: the session runs; [`VoiceView::ring`] says what the voice does.
    Live,
    /// The phone ended it: the stop went and the channel closes (§7.6).
    Ending,
    /// Over: [`VoiceView::end`] says why.
    Ended,
}

/// What the ring shows (design §10.3): the Machine's state, or Kai's own around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RingState {
    /// No call, or a call that ended.
    Rest,
    /// Connecting, before the listening cue.
    Connecting,
    Listening,
    Thinking,
    Speaking,
    Interrupted,
    /// The bound call is idle (nothing open, nobody speaks).
    Idle,
    /// The owner's mute (§7.5).
    Muted,
    /// A pending bind's prompt (§7.9).
    Pending,
}

/// How a call ended (design §7.7, §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EndReason {
    /// The owner's End.
    Ended,
    /// End while a bind was pending (§7.9).
    Declined,
    /// The assistant overlay that started the call was dismissed before the owner's first turn
    /// (§7.7, §9.2): no words.
    Dismissed,
    /// `voice.idle_end_s` of quiet.
    Idle,
    /// The conversation's thread moved (`folder.current`): the call ended at a quiet point and a
    /// new one binds by the folder ([`CallEnd::restart`]).
    Rebind,
    /// `lmgw.session.closed {kind: taken_over}`.
    TakenOver,
    /// `thread_bound`: the pending bind was left unanswered.
    ThreadBound,
    /// `revoked`: the device's key or certificate.
    Revoked,
    /// `out_of_reach`: the thread is gone.
    OutOfReach,
    /// `closed`: lmgw ended it for a reason of its own (named in the message).
    Closed,
    /// `shutting_down`: lmgw stops, or its device listener changed.
    ShuttingDown,
    /// A `kind` this build does not know.
    Other,
    /// The transport failed, or the channel closed without `lmgw.session.closed`.
    ConnectionLost,
    /// lmgw did not start the session within `voice.connect_timeout_s`.
    ConnectTimeout,
    /// The POST was refused, or the call could not be set up.
    Refused,
    /// The phone could not carry the call on: a libwebrtc failure (its SDP, its channel) or
    /// Android's refusal of the microphone type, in the words Kotlin gave.
    Failed,
    /// A permanent loss of the audio focus: another app took the audio (§8.5).
    AudioTaken,
}

/// An action an ended call offers (design §7.8, §7.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceAction {
    /// A new call by the folder (`takeover=ask`), the stale call hung up first.
    Reconnect,
    /// A new call that takes the thread over as soon as `bind_pending` arrives.
    TakeOver,
}

/// How a call ended, with its words (design §7.8's table).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CallEnd {
    pub reason: EndReason,
    /// The words; `None` where the table has none (the owner's End, a decline, a rebind).
    pub words: Option<String>,
    /// lmgw's sentence, shown under the words and never parsed.
    pub message: Option<String>,
    /// Who (`by`, `held_by`), as lmgw names a client.
    pub by: Option<String>,
    pub actions: Vec<VoiceAction>,
    /// A rebind: Kotlin starts this call at once, once the old PeerConnection is closed.
    pub restart: Option<CallStart>,
}

/// Who a caption line is from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Speaker {
    /// The owner's words (the transcript).
    Owner,
    Kai,
    /// lmgw's word on the call: an error with its code, a broken part set.
    Note,
}

/// One caption line (design §7.6), replaced by `(call, id)` while it changes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CaptionLine {
    /// The call it belongs to ([`VoiceView::call`]); a new call's lines replace the old ones.
    pub call: u64,
    pub id: u64,
    pub speaker: Speaker,
    pub text: String,
    /// Final: the transcript completed, or the thread kept the reply (`lmgw.chat.reply`).
    pub done: bool,
    /// "answered by <alias> (fallback)", "not heard".
    pub note: Option<String>,
}

/// The call's snapshot (design §7.1, §3.2).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceView {
    /// Counts the calls of this process from 1; 0 before the first.
    pub call: u64,
    pub phase: CallPhase,
    pub ring: RingState,
    /// The state line ("Listening", "Voice is on device 'desktop'", the end's words).
    pub words: String,
    /// The listening cue played (DTLS connected, or the first samples after a pending bind).
    pub cue: bool,
    /// The owner's first turn reached the thread with words (or Kai completed a reply): dismissing
    /// the assistant overlay leaves the call going on (§9.2); before it, a dismissal ends the call.
    pub spoken: bool,
    /// The call's turn mode is push-to-talk (lmgw's `session.lmgw.turn_mode`, or the owner's
    /// override `voice.mode`).
    pub push_to_talk: bool,
    /// The owner's mute.
    pub muted: bool,
    /// Another app holds the audio for a while (a transient focus loss, §8.5): Kai was stopped
    /// and the microphone muted; the mute stays when the focus comes back.
    pub paused: bool,
    /// The call runs half duplex (`voice.echo` `half_duplex` at its start, §8.2): speech does not
    /// interrupt Kai, the tap does.
    pub half_duplex: bool,
    /// Push-to-talk is held.
    pub pressed: bool,
    /// Kai's audio muted locally (a stop or `output_audio_buffer.cleared`, until the next
    /// `started`): a fact beside the ring (§7.1).
    pub kai_muted: bool,
    /// A response is open (thinking, speaking, a tool): a tap stops it (§7.6).
    pub stoppable: bool,
    /// The pending bind's holder (§7.9).
    pub held_by: Option<String>,
    /// Take over was sent and `session.created` has not come yet.
    pub taking_over: bool,
    /// The prompt came after the listening cue: what was said since was not heard.
    pub pending_note: Option<String>,
    /// K16: another principal's turn runs in the thread (automatic mode keeps the track silent).
    pub waiting_for: Option<String>,
    pub thread_id: Option<i64>,
    pub thread_title: Option<String>,
    /// The tool the turn runs now.
    pub tool: Option<String>,
    /// The bound thread's calls waiting for the owner's verdict (their tools, `<label>__<name>`):
    /// Kai waits for a decision (§11); their cards are the hub's approvals.
    pub awaiting: Vec<String>,
    /// The GPU hold (a badge), and the fallback alias that answers in its place.
    pub hold: bool,
    pub hold_fallback: Option<String>,
    /// `lmgw.model.state {state: held}`: the stage and alias paused ("chat gemma").
    pub held_stage: Option<String>,
    /// The latest error lmgw sent, verbatim with its code.
    pub error: Option<String>,
    /// A static value the owner should know about in this call (push-to-talk's tail when
    /// `voice.ptt_tail_ms` is not set): shown, never hidden.
    pub note: Option<String>,
    pub end: Option<CallEnd>,
}

impl VoiceView {
    /// Before any call.
    pub fn off() -> Self {
        Self {
            call: 0,
            phase: CallPhase::Off,
            ring: RingState::Rest,
            words: String::new(),
            cue: false,
            spoken: false,
            push_to_talk: false,
            muted: false,
            paused: false,
            half_duplex: false,
            pressed: false,
            kai_muted: false,
            stoppable: false,
            held_by: None,
            taking_over: false,
            pending_note: None,
            waiting_for: None,
            thread_id: None,
            thread_title: None,
            tool: None,
            awaiting: Vec::new(),
            hold: false,
            hold_fallback: None,
            held_stage: None,
            error: None,
            note: None,
            end: None,
        }
    }
}
