//! What Kotlin sees (design §3.2): the [`Update`]s `Core::next_update` hands over and the records
//! they carry. Snapshots (the connection, the conversation, the open approvals) or deltas with
//! ids (the messages), so a collector that falls behind merges them and loses nothing.

use crate::markdown::MdBlock;
pub use crate::voice::view::{CaptionLine, VoiceView};

/// One update (design §3.2).
// UniFFI carries a record by value (a boxed variant is not expressible across the FFI), and the
// updates are few: the connection's snapshot stays inline.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Update {
    /// The connection's state (§5.3), whole.
    Connection { view: ConnectionView },
    /// The Kai conversation's folder and thread (§6.1), whole.
    Conversation { view: ConversationView },
    /// The Kai thread's messages changed (§6.2).
    Messages { delta: MessagesDelta },
    /// The calls waiting for a decision that the phone can see (§11), whole.
    Approvals { open: Vec<ApprovalView> },
    /// Raise or replace a notification (§12.3).
    Notify { notification: NotificationView },
    /// Withdraw a notification.
    Withdraw { id: NotificationId },
    /// The earliest deadline the core needs the CPU for, in boot milliseconds
    /// (`SystemClock.elapsedRealtime()`): Kotlin keeps one inexact alarm for it, whose receiver
    /// calls `Core::wake` (§12.2). `None`: no alarm.
    WakeAt { boot_ms: Option<u64> },
    /// A one-time sentence for the owner (a refusal the screen does not show otherwise).
    Notice { text: String },
    /// The voice call's snapshot (§7.1), whole.
    Voice { view: VoiceView },
    /// One caption line of the call, new or changed (§7.6), by `(call, id)`.
    Caption { line: CaptionLine },
}

/// The connection's states, first match wins (design §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    NotPaired,
    /// The first try is under way (no failure yet).
    Connecting,
    /// lmgw answers 403 `device_unconfirmed`: the owner has not confirmed the code yet.
    WaitingForConfirmation,
    /// The feed's `hello` came.
    Connected,
    /// No default network.
    NoNetwork,
    /// The certificate lapsed (the app's own date, 401 `device_certificate_expired`, or a link
    /// ended with the kind `certificate_expired`).
    CertificateLapsed,
    /// lmgw refused the device's key over the authenticated link (a 401 by code, the feed's
    /// `revoked`): [`ConnectionView::refusal`] says which.
    KeyRefused,
    /// The Keystore cannot sign with the device key (gone, invalidated).
    KeyMissing,
    /// An unknown issuer after enrolment: lmgw's device authority changed, or this is not the
    /// owner's lmgw.
    NotThisLmgw,
    /// A host lmgw's certificate does not name, on every URL that answered.
    HostNotNamed,
    /// Every URL refused the connection.
    NotRunning,
    /// No TCP answer from any URL within `lmgw.timeout_s`.
    NotAnswering,
    /// Any other failure (an alert from whoever answered, a broken feed, lmgw's refusal of the
    /// feed by another code), in its own words.
    Unreachable,
}

/// How lmgw refused the key (design §5.5), read by code or by the `revoked` kind, never by lmgw's
/// words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum KeyRefusal {
    /// `key_unknown` / `device_key_unknown` (re-paired elsewhere, or deleted), another kind's
    /// `revoked`: pair again.
    PairAgain,
    /// `device_disabled`: enable it on its Devices card, or pair again; the key stays.
    Disabled,
    /// `key_expired` (the row's policy): move its expiry, or pair again; the key stays.
    Expired,
    /// `client_certificate_required`, `credential_on_device_listener`: a bug in Kai.
    Bug,
    /// A `revoked` of a kind, or a 401 of a code, this build does not know: lmgw's sentence in a
    /// notice, then the backoff (never a stop).
    Unknown,
}

/// One action of a state (design §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionAction {
    Pair,
    PairAgain,
    CheckNow,
    TryAgain,
    OpenWireGuard,
}

/// The connection (design §5.3): its state, words and actions, and the badges.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConnectionView {
    pub state: ConnectionState,
    /// The state's sentence (§5.3's words).
    pub words: String,
    /// Each URL's failure, `<url>: <why>`, when the state came from a race.
    pub detail: Vec<String>,
    pub actions: Vec<ConnectionAction>,
    /// For [`ConnectionState::KeyRefused`].
    pub refusal: Option<KeyRefusal>,
    /// A pairing is stored (`link.json`).
    pub paired: bool,
    /// The confirmation code, while the state waits for it, and in the gateway's facts.
    pub confirm_code: Option<String>,
    /// The device's name as lmgw gave it (`hello.principal.name`), once connected.
    pub device_name: Option<String>,
    /// When the next try runs, unix milliseconds; `None` when none is scheduled.
    pub next_try_at_ms: Option<i64>,
    /// The GPU hold is on (a badge on any state): local models are paused.
    pub held: bool,
    pub hold_fallback: Option<String>,
    /// The certificate's coming lapse (a badge): "this device's certificate lapses on <date>;
    /// pair it again before then".
    pub cert_notice: Option<String>,
    /// The stored pairing's facts, for the settings page.
    pub gateway: Option<GatewayView>,
}

/// The stored pairing, as the settings page shows it (design §10.1).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GatewayView {
    /// The device's name as the link gave it.
    pub name: String,
    /// lmgw's URLs, in the owner's order.
    pub urls: Vec<String>,
    /// The one that last connected.
    pub last_good: Option<String>,
    /// lmgw's CA fingerprint, shortened (`sha256:9f2c…41ab`).
    pub fp_short: String,
    /// The certificate's dates (UTC days).
    pub not_after: String,
    pub notice_after: String,
    pub confirm_code: String,
    /// lmgw's dashboard for this device (`hello.dashboard_url`), when lmgw offers one.
    pub dashboard_url: Option<String>,
    /// The device key's Keystore alias (public): the page reads the key's security level by it.
    pub key_alias: String,
}

/// Whether the Kai conversation can be used (design §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConversationStatus {
    /// Not connected yet, or looking up the folder.
    Loading,
    /// The folder is ongoing with a model; the thread is known.
    Ready,
    /// The folder needs the owner's steps on lmgw ([`ConversationView::steps`]).
    NotReady,
    /// The lookup failed ([`ConversationView::note`]); it is asked again on its own backoff and at
    /// the next connection.
    Failed,
}

/// The Kai conversation (design §6).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConversationView {
    pub status: ConversationStatus,
    /// `conversation.folder_name`.
    pub folder_name: String,
    pub folder_id: Option<i64>,
    /// The owner's steps on lmgw, in the dashboard's words, while the folder is not ready.
    pub steps: Option<String>,
    /// Why the lookup failed, or a refusal of `current`.
    pub note: Option<String>,
    pub thread_id: Option<i64>,
    pub thread_title: Option<String>,
    /// Who else is answering in the thread now ("device 'desktop'", "the dashboard").
    pub answering: Vec<String>,
    /// Who holds the thread's voice, when another client does ("Voice is on <by>").
    pub voice_by: Option<String>,
    /// A text send of the phone's is streaming.
    pub sending: bool,
}

/// A change of the Kai thread's messages (design §6.2), by message id.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MessagesDelta {
    /// Drop every message shown before applying `upsert` (a new thread, a reload).
    pub reset: bool,
    pub thread_id: Option<i64>,
    /// Messages to show or replace, by id, in the thread's order.
    pub upsert: Vec<MessageView>,
    /// Message ids that went.
    pub removed: Vec<i64>,
}

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Role {
    User,
    Assistant,
    /// A late MCP task result.
    Tool,
    Other,
}

/// One message (design §6.2).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MessageView {
    /// lmgw's id; a reply still streaming to the phone's own send has a negative one until its
    /// `done` names the real id.
    pub id: i64,
    pub role: Role,
    /// The text as written.
    pub text: String,
    /// Its Markdown, parsed (M14).
    pub blocks: Vec<MdBlock>,
    /// `YYYY-MM-DD HH:MM:SS` UTC; empty while streaming.
    pub created_at: String,
    /// The alias that answered in `model`'s place (a fallback, ruling 10: shown, never refused).
    pub answered_by: Option<String>,
    /// A voice turn (the microphone mark).
    pub voice: bool,
    /// Still streaming.
    pub streaming: bool,
    /// The tools the streaming turn called, by name, in order.
    pub tools: Vec<String>,
    /// The turn's error, verbatim with its code (no answer came, or the stream broke).
    pub error: Option<String>,
    /// Quiet lines about the answer: an MCP label it was answered without (design §6.3). Kept on
    /// the saved reply; shown in the conversation only, never in a notification.
    pub notices: Vec<String>,
}

/// A call waiting for a decision (design §11).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ApprovalView {
    pub approval_request_id: String,
    pub thread_id: i64,
    pub thread_title: String,
    pub message_id: i64,
    pub server_label: String,
    /// The tool's own name.
    pub name: String,
    /// The arguments in compact lines (`key: value`).
    pub arguments: String,
    /// The verdict given on the phone and held until every call of the reply has one.
    pub verdict: Option<bool>,
    /// The reply's verdicts went to lmgw and wait for its answer: nothing to decide here now.
    pub sent: bool,
    /// How many other calls of the reply wait for a verdict (the held one waits for them).
    pub waiting: u32,
    /// The running voice call's session showed it: the verdict goes through the call and Kai
    /// speaks the resumed reply there (§11).
    pub in_call: bool,
    /// Why lmgw did not take the last verdict (its words and what the owner can do), until the
    /// next one.
    pub note: Option<String>,
}

/// Which notification (design §12.3): stable ids, so a resumed feed or a restarted process never
/// alerts twice for one thing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct NotificationId {
    pub kind: NotificationKind,
    /// The approval's id, the thread's id, or the notice's name.
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum NotificationKind {
    /// Channel `approvals` (high).
    Approval,
    /// Channel `messages` (default): Kai's replies.
    Reply,
    /// Channel `notices` (default).
    Notice,
}

/// A notification, built from events alone (design §12.3).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NotificationView {
    pub id: NotificationId,
    pub title: String,
    pub text: String,
    /// The thread it is about (the tap opens the conversation).
    pub thread_id: Option<i64>,
    /// For an approval: what its Approve and Decline actions decide.
    pub approval: Option<ApprovalNote>,
    /// For a reply: how many replies it stands for (collapsed at `caught_up`).
    pub count: u32,
}

/// What an approval notification's actions decide.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ApprovalNote {
    pub thread_id: i64,
    pub approval_request_id: String,
}

/// The default network as Kotlin sees it (design §5.2): `registerDefaultNetworkCallback`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NetworkFacts {
    /// A default network exists.
    pub available: bool,
    /// Which one (Android's `Network.toString()`): a change of it is a change of network.
    pub id: String,
    /// `wifi`, `cellular`, `vpn`, `ethernet`, … for the words.
    pub transport: String,
}

/// What enrolment answered, for the confirmation screen (design §3.2).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairingView {
    /// The device's name as lmgw gave it.
    pub device_name: String,
    /// The confirmation code (`7F3A-91C2`).
    pub confirm_code: String,
    pub urls: Vec<String>,
    /// lmgw said the device is confirmed already (a repeat after the owner's Confirm).
    pub confirmed: bool,
}

/// What the owner confirms before pairing (design §4.1, K13): `lmgw_client::pairing::PairFacts`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairingPreview {
    /// The device name, safe to show.
    pub name: String,
    pub urls: Vec<String>,
    /// lmgw's CA fingerprint, shortened.
    pub fp: String,
    /// What names the pairing this one replaces (its first URL); `None` for a first pairing.
    pub replaces: Option<String>,
    /// The dialog's lines, in order; `warning` marks the replacement's.
    pub lines: Vec<PreviewLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PreviewLine {
    pub text: String,
    pub warning: bool,
}

/// Whether a decision went to lmgw or waits for the reply's other calls (design §11).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Decided {
    /// Every call of the reply had a verdict: sent.
    Sent,
    /// Held until these other calls of the reply have one.
    Waiting { remaining: u32 },
}
