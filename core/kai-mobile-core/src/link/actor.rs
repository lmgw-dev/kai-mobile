//! The link's actor (design §3.1): one task that owns the pairing, the held feed's connection and
//! its deadlines, the conversation's state, the open approvals and the notifications. Every FFI
//! entry point sends it a [`Cmd`]; the requests it needs run as tasks of their own that report back
//! the same way, so nothing waits behind a lock and nothing blocks the actor.
//!
//! **The held feed** (§5.4, §12): one connection at a time ([`super::feed`]), resumed from the
//! persisted cursor, asking `feed.keepalive_s`. Its deadlines count sleep (the boot clock): the
//! race and the first bytes within `lmgw.timeout_s`, then two keep-alive intervals without a byte
//! for the dead-link verdict. The actor sleeps on tokio's timer towards the earliest one and
//! compares with the boot clock at every wake; it publishes that deadline as `Update::WakeAt` for
//! Kotlin's one alarm.
//!
//! **Retries** (§5.3, §12.2): 1 s doubling to `lmgw.retry_max_s` while the app is visible and to
//! `lmgw.retry_max_background_s` while it is not; at once on a new default network or when the app
//! becomes visible, the backoff reset; after a `hello` the backoff starts over. While lmgw waits for
//! the owner's Confirm and the app is visible, every `pairing.confirm_poll_s` on the kept-alive
//! connection. A refusal over the authenticated link stops the retries (a key lmgw refuses, a
//! lapse by the phone's own date) until Try again or Pair again; a handshake's alert never does.

use std::{path::PathBuf, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use lmgw_client::{
    feed::{FeedEvent, FeedItem, FeedReader},
    requests::{self, ApiError, CredentialState, KeyRefused, ThreadDetail},
    types::realtime::RevokeKind,
};

use crate::voice::actor::{Cmd as VoiceCmd, Ticket};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{
    failure::Failure,
    feed::{self, Chunk, FeedMsg, Plan},
    http::Handle,
    tls::Tls,
    words::{Said, of_failures},
};
use crate::{
    CoreError, Settings,
    approvals::{self, Approvals, CallVerdicts, Shown},
    clock::{Clock, date},
    conversation::{
        Convo, Streaming,
        load::{Fail, Loaded},
        send::{self, Frame, StreamFail},
        streaming_view,
    },
    keystore::{KEY_PREFIX, Keystore, KeystoreKey},
    notify::Replies,
    pairing::Enrolled,
    store::{self, APPROVALS_FILE, ApprovalsFile, LINK_FILE, LinkFile},
    views::{
        ConnectionState, ConnectionView, ConversationStatus, Decided, GatewayView, KeyRefusal,
        MessagesDelta, NetworkFacts, NotificationKind, PairingView, Update,
    },
};

const FIRST_RETRY: Duration = Duration::from_secs(1);

/// What the actor is told.
pub enum Cmd {
    /// An enrolment succeeded: store it and start the new pairing.
    Paired {
        enrolled: Box<Enrolled>,
        name: String,
        fp: String,
        reply: oneshot::Sender<Result<PairingView, CoreError>>,
    },
    /// Forget gateway.
    Forget {
        reply: oneshot::Sender<()>,
    },
    /// Check now / Try again.
    Retry,
    Network(NetworkFacts),
    Foreground(bool),
    /// The assistant overlay shows the approval cards (unlocked) or no longer does (§9.2, §11):
    /// with the app's visibility, whether a card is seen, so no notification asks as well.
    Overlay(bool),
    /// An alarm or a wake: compare the deadlines with the boot clock.
    Wake,
    SettingsChanged,
    /// The user dismissed an approval's notification: answered once `approvals.json` holds it.
    Dismiss {
        id: String,
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
    /// A notification is gone by another way (tapped).
    Gone {
        kind: NotificationKind,
        key: String,
    },
    /// The stored pairing's first URL and key alias (for the preview and the enrolment).
    Current {
        reply: oneshot::Sender<(Option<String>, Option<String>)>,
    },
    /// A text send starts: the link and the thread, or why not.
    SendStart {
        reply: oneshot::Sender<Result<(Handle, i64), CoreError>>,
    },
    SendFrame(Frame),
    SendEnded {
        identity: [u8; 32],
        result: Result<(), StreamFail>,
    },
    /// New conversation starts: the link, the folder name and the remembered id, or why not.
    NewStart {
        reply: oneshot::Sender<Result<NewTicket, CoreError>>,
    },
    /// A lookup (or New conversation) ended.
    Loaded {
        generation: u64,
        identity: [u8; 32],
        loaded: Box<Loaded>,
        probe: bool,
    },
    ThreadLoaded {
        generation: u64,
        identity: [u8; 32],
        result: Box<Result<ThreadDetail, Fail>>,
    },
    /// A verdict given on the phone.
    Decide {
        id: String,
        approve: bool,
        reply: DecideReply,
    },
    DecideEnded {
        identity: [u8; 32],
        ids: Vec<String>,
        result: Result<(), StreamFail>,
        /// Sent again by the route after a call ended before lmgw answered them there:
        /// `approval_decided` then means the session took them.
        resent: bool,
    },
    /// A connection's message.
    Feed {
        generation: u64,
        msg: FeedMsg,
    },
    /// A call's trigger (design §7.3 step 1): the link's facts, or why voice cannot start now.
    VoiceTicket {
        reply: oneshot::Sender<Result<Ticket, CoreError>>,
    },
    /// The call bound another thread than the conversation's (the folder rolled over at the
    /// bind): the conversation follows (§6.2).
    VoiceThread(i64),
    /// A call of the phone's runs (its number) or none does: Kai's replies notify only when none
    /// does (§12.3), and the calls its session showed are its to answer (§11).
    CallRunning(Option<u64>),
    /// The running call's session showed a call waiting for an approval (§11, api §6.4).
    CallApproval(Box<Shown>),
    /// lmgw said on the call that another client decided `id` (`lmgw.approval.decided`).
    CallApprovalDecided {
        id: String,
    },
    /// What became of verdicts given to the call.
    CallVerdicts(CallVerdicts),
    /// lmgw ended a call because the device was revoked (§7.8): as the feed's `revoked`.
    VoiceRevoked {
        kind: RevokeKind,
        message: String,
    },
    /// A call's request was refused by a key refusal (§5.5).
    VoiceRefused {
        identity: [u8; 32],
        status: u16,
        error: ApiError,
    },
    /// The Kai folder is gone (a call's 404 `chat_folder_not_found`): look it up again.
    Reload,
    /// The core goes.
    Shutdown,
}

/// New conversation's start: the link, the folder's name, the remembered folder, the lookup's
/// generation.
pub type NewTicket = (Handle, String, Option<i64>, u64);

/// Verdicts to send: the link, the thread, `(id, approve)` for every call of the reply.
pub type Verdicts = (Handle, i64, Vec<(String, bool)>);

/// Sends a reply's verdicts by the route (`POST /chat/api/threads/{id}/approvals`, §11) and reads
/// the resumed turn's stream to its end in the background (a request that stopped reading would
/// stop the turn); `DecideEnded` says how it went.
pub fn spawn_verdicts(
    h: Handle,
    thread: i64,
    verdicts: Vec<(String, bool)>,
    timeout: Duration,
    cmds: mpsc::UnboundedSender<Cmd>,
    resent: bool,
) {
    tokio::spawn(async move {
        let req = send::approvals_request(thread, &verdicts);
        let result = send::stream(&h, &req, timeout, |_| {}).await;
        let _ = cmds.send(Cmd::DecideEnded {
            identity: h.identity(),
            ids: verdicts.into_iter().map(|(id, _)| id).collect(),
            result,
            resent,
        });
    });
}

/// `decide`'s answer.
type DecideReply = oneshot::Sender<Result<(Decided, Option<Verdicts>), CoreError>>;

/// A decision that came before the open set was known (a notification's action that started the
/// process): held until the first `caught_up` since start, at most until `until` (boot ms).
struct HeldDecision {
    id: String,
    approve: bool,
    reply: DecideReply,
    until: u64,
}

/// What the actor shares with the core.
pub struct Ctx {
    pub clock: Arc<dyn Clock>,
    pub keystore: Arc<dyn Keystore>,
    pub settings: Arc<ArcSwap<Settings>>,
    pub data_dir: PathBuf,
    pub updates: mpsc::UnboundedSender<Update>,
    pub cmds: mpsc::UnboundedSender<Cmd>,
    /// Whether a pairing is stored (read by `Core::paired` without the actor).
    pub paired: Arc<std::sync::atomic::AtomicBool>,
    /// The voice task: the conversation's thread and the hold, as they change.
    pub voice: mpsc::UnboundedSender<VoiceCmd>,
}

/// The stored pairing in use.
struct Paired {
    file: LinkFile,
    tls: Tls,
}

impl Paired {
    fn open(
        file: LinkFile,
        keystore: Arc<dyn Keystore>,
        timeout: Duration,
    ) -> Result<Self, String> {
        file.check()?;
        let spki = file.spki().ok_or("its public key is not base64")?;
        let key = KeystoreKey::new(keystore, file.alias.clone(), spki);
        let tls = Tls::paired(&file.credentials, key, timeout)?;
        Ok(Self { file, tls })
    }

    fn order(&self) -> Vec<String> {
        self.file
            .credentials
            .urls
            .order(self.file.last_good.as_deref())
    }

    fn handle(&self) -> Handle {
        Handle {
            tls: self.tls.clone(),
            base: self.order().swap_remove(0),
        }
    }
}

/// Where the connection stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No connection: waiting for a retry, stopped, or not paired.
    Idle,
    /// The race runs.
    Racing,
    /// lmgw answered; `began`: bytes came; `up`: `hello` came.
    Open { began: bool, up: bool },
}

pub struct Actor {
    ctx: Ctx,
    rx: mpsc::UnboundedReceiver<Cmd>,
    link: Option<Paired>,
    net: NetworkFacts,
    visible: bool,
    generation: u64,
    task: Option<JoinHandle<()>>,
    phase: Phase,
    /// The keep-alive lmgw stated in its last `hello` on this pairing.
    stated: Option<u32>,
    delay: Duration,
    retry_at: Option<u64>,
    dead_at: Option<u64>,
    /// The folder lookup's next try after a failure (the lookup's own backoff, while connected):
    /// reply notifications wait for the Kai folder's threads (§12.3), so a failed lookup is asked
    /// again rather than given up.
    lookup_at: Option<u64>,
    lookup_delay: Duration,
    /// No automatic retry (a refusal over the authenticated link, a lapse).
    stopped: bool,
    /// The last refusal was `device_unconfirmed`: the next try polls.
    unconfirmed: bool,
    said: Said,
    device_name: Option<String>,
    held: bool,
    hold_fallback: Option<String>,
    dashboard_url: Option<String>,
    caught_up: bool,
    /// A `caught_up` came since start (or since another gateway's pairing): the open set is known.
    open_known: bool,
    /// Decisions that came before it, held (§11).
    held_decisions: Vec<HeldDecision>,
    published_wake: Option<Option<u64>>,
    published_view: Option<ConnectionView>,
    approvals: Approvals,
    replies: Replies,
    convo: Convo,
    keepalive_asked: u32,
    timeout_used: Duration,
    /// A Keystore failure that is not a lost key was said in a notice (once until the next
    /// `hello`).
    keystore_noticed: bool,
    /// The device key the stored pairing names, for the start's sweep of orphan keys: `None` when
    /// `link.json` did not read (nothing is swept then), `Some(None)` when no pairing is stored.
    sweep: Option<Option<String>>,
    /// `conversation.folder_name` as the lookup last used it.
    folder_name: String,
    /// A call of the phone's runs (the owner hears its replies, §12.3).
    in_call: bool,
    /// The assistant overlay shows the approval cards (§9.2).
    overlay: bool,
    /// The hold as the voice task last heard it.
    voice_hold: Option<(bool, Option<String>)>,
}

impl Actor {
    /// The actor over `ctx`, with the stored pairing loaded (a file that does not hold together is
    /// said and treated as none).
    pub fn new(
        ctx: Ctx,
        rx: mpsc::UnboundedReceiver<Cmd>,
        notified_approvals: Vec<String>,
        notified_replies: Vec<i64>,
    ) -> Self {
        let settings = ctx.settings.load_full();
        let timeout = Duration::from_secs(u64::from(settings.lmgw.timeout_s));
        let mut notice = None;
        let mut sweep = Some(None);
        let link = match store::read::<LinkFile>(&ctx.data_dir.join(LINK_FILE)) {
            Ok(Some(file)) => {
                sweep = Some(Some(file.alias.clone()));
                match Paired::open(file, ctx.keystore.clone(), timeout) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        notice = Some(format!(
                            "The stored pairing does not hold together ({e}): pair again."
                        ));
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                sweep = None;
                notice = Some(format!("{e}: pair again."));
                None
            }
        };
        let dismissed = match store::read::<ApprovalsFile>(&ctx.data_dir.join(APPROVALS_FILE)) {
            Ok(f) => f.unwrap_or_default().dismissed,
            Err(e) => {
                tracing::warn!(error = %e, "link: approvals.json does not read; no dismissed calls");
                Vec::new()
            }
        };
        ctx.paired
            .store(link.is_some(), std::sync::atomic::Ordering::SeqCst);
        let said = if link.is_some() {
            Said::connecting()
        } else {
            Said::not_paired()
        };
        let mut approvals = Approvals::new(dismissed);
        approvals.set_notified(notified_approvals);
        let mut replies = Replies::default();
        replies.set_shown(notified_replies);
        let actor = Self {
            ctx,
            rx,
            link,
            net: NetworkFacts {
                available: true,
                id: String::new(),
                transport: String::new(),
            },
            visible: false,
            generation: 0,
            task: None,
            phase: Phase::Idle,
            stated: None,
            delay: FIRST_RETRY,
            retry_at: None,
            dead_at: None,
            lookup_at: None,
            lookup_delay: FIRST_RETRY,
            stopped: false,
            unconfirmed: false,
            said,
            device_name: None,
            held: false,
            hold_fallback: None,
            dashboard_url: None,
            caught_up: false,
            open_known: false,
            held_decisions: Vec::new(),
            published_wake: None,
            published_view: None,
            approvals,
            replies,
            convo: Convo::default(),
            keepalive_asked: settings.feed.keepalive_s,
            timeout_used: timeout,
            keystore_noticed: false,
            sweep,
            folder_name: settings.conversation.folder_name.clone(),
            in_call: false,
            overlay: false,
            voice_hold: None,
        };
        if let Some(n) = notice {
            tracing::warn!(%n, "link: the stored pairing was not loaded");
            let _ = actor.ctx.updates.send(Update::Notice { text: n });
        }
        actor
    }

    fn settings(&self) -> Arc<Settings> {
        self.ctx.settings.load_full()
    }

    /// At start, before any command: every device key (`kai-device-*`) that is neither the stored
    /// pairing's nor a pending token's is deleted (§4.3). A pending token's key exists only after
    /// an enrolment asked this actor for the stored pairing (`Cmd::Current`), which it answers
    /// after this, so at this point none does. Nothing is swept when `link.json` did not read.
    fn sweep_keys(&mut self) {
        let Some(keep) = self.sweep.take() else {
            tracing::warn!(
                "link: link.json did not read; the Keystore's device keys are left as they are"
            );
            return;
        };
        match self.ctx.keystore.aliases() {
            Ok(aliases) => {
                for alias in aliases {
                    if alias.starts_with(KEY_PREFIX) && Some(&alias) != keep.as_ref() {
                        tracing::info!(%alias, "link: a device key no pairing names; deleted");
                        self.ctx.keystore.delete(alias);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "link: the Keystore's aliases could not be listed; no orphan key swept")
            }
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(u64::from(self.settings().lmgw.timeout_s))
    }

    fn now(&self) -> u64 {
        self.ctx.clock.boot_ms()
    }

    fn emit(&self, u: Update) {
        let _ = self.ctx.updates.send(u);
    }

    fn emit_all(&self, us: impl IntoIterator<Item = Update>) {
        for u in us {
            self.emit(u);
        }
    }

    /// Runs until the core goes.
    pub async fn run(mut self) {
        self.sweep_keys();
        self.publish_conversation();
        if self.link.is_some() {
            self.retry_at = Some(self.now());
        }
        loop {
            self.due();
            self.publish();
            let deadline = self.deadline();
            let sleep = deadline.map(|d| Duration::from_millis(d.saturating_sub(self.now())));
            let cmd = match sleep {
                Some(s) => tokio::select! {
                    c = self.rx.recv() => c,
                    () = tokio::time::sleep(s) => continue,
                },
                None => self.rx.recv().await,
            };
            match cmd {
                None | Some(Cmd::Shutdown) => break,
                Some(cmd) => self.cmd(cmd),
            }
        }
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }

    // ------------------------------------------------------------------ deadlines

    /// The earliest deadline the actor needs the CPU for (§12.2).
    fn deadline(&self) -> Option<u64> {
        [self.retry_at, self.dead_at, self.lookup_at]
            .into_iter()
            .flatten()
            .chain(self.held_decisions.iter().map(|d| d.until))
            .min()
    }

    /// Acts on what is due by the boot clock.
    fn due(&mut self) {
        let now = self.now();
        if self.dead_at.is_some_and(|d| now >= d) {
            self.dead_at = None;
            let why = self.wait_said();
            let was_up = matches!(self.phase, Phase::Open { up: true, .. });
            tracing::info!(%why, "link: a deadline passed");
            self.drop_connection();
            if was_up {
                self.delay = FIRST_RETRY;
            }
            self.said = Said::unreachable(why);
            self.schedule();
        }
        if self.retry_at.is_some_and(|r| now >= r) {
            self.retry_at = None;
            self.connect();
        }
        if self.lookup_at.is_some_and(|r| now >= r) {
            self.lookup_at = None;
            tracing::info!("conversation: looking the folder up again");
            self.reload(false);
        }
        if self.held_decisions.iter().any(|d| now >= d.until) {
            let (due, keep) = std::mem::take(&mut self.held_decisions)
                .into_iter()
                .partition(|d| now >= d.until);
            self.held_decisions = keep;
            for d in due {
                tracing::info!(id = %d.id, "approvals: lmgw.timeout_s passed before the open set was known; the decision was not sent");
                let _ = d.reply.send(Err(CoreError::Unreachable {
                    reason: format!(
                        "Kai was not connected to lmgw within {} s (lmgw.timeout_s), so the \
                         decision was not sent; the call still waits. {}",
                        self.timeout().as_secs(),
                        self.said.words
                    ),
                    setting: Some("lmgw.timeout_s".to_owned()),
                }));
            }
        }
    }

    /// What the deadline that passed waited for (Kai desktop's `Wait`).
    fn wait_said(&self) -> String {
        let timeout = self.timeout().as_secs();
        match self.phase {
            Phase::Idle | Phase::Racing => format!(
                "lmgw did not answer the feed's request within {timeout} s (lmgw.timeout_s)"
            ),
            Phase::Open { began: false, .. } => {
                format!("lmgw sent no hello, nor any byte, within {timeout} s (lmgw.timeout_s)")
            }
            Phase::Open { up: false, .. } => format!(
                "lmgw began the feed but sent no hello and nothing else for {} s (two keep-alive \
                 intervals)",
                2 * self.stated.map_or(timeout, u64::from)
            ),
            Phase::Open { up: true, .. } => format!(
                "lmgw sent nothing for {} s (two hello.keepalive_s intervals): the link is dead",
                2 * self.stated.map_or(timeout, u64::from)
            ),
        }
    }

    /// The next try after the backoff (or the confirmation's poll).
    fn schedule(&mut self) {
        if self.stopped || self.link.is_none() || !self.net.available {
            self.retry_at = None;
            return;
        }
        let settings = self.settings();
        if self.unconfirmed && self.visible {
            let poll = Duration::from_secs(u64::from(settings.pairing.confirm_poll_s));
            self.retry_at = Some(self.now() + ms(poll));
            return;
        }
        let ceiling = self.ceiling();
        let d = self.delay.min(ceiling);
        self.delay = (d * 2).min(ceiling);
        self.retry_at = Some(self.now() + ms(d));
    }

    /// The backoff's ceiling: `lmgw.retry_max_s` while the app is visible,
    /// `lmgw.retry_max_background_s` while it is not (§12.2).
    fn ceiling(&self) -> Duration {
        let settings = self.settings();
        Duration::from_secs(u64::from(if self.visible {
            settings.lmgw.retry_max_s
        } else {
            settings.lmgw.retry_max_background_s
        }))
    }

    /// The folder lookup failed while connected: asked again on its own backoff (1 s doubling to
    /// the ceiling), the replies collected meanwhile.
    fn schedule_lookup(&mut self) {
        if !matches!(self.phase, Phase::Open { up: true, .. }) {
            self.lookup_at = None;
            return;
        }
        let ceiling = self.ceiling();
        let d = self.lookup_delay.min(ceiling);
        self.lookup_delay = (d * 2).min(ceiling);
        self.lookup_at = Some(self.now() + ms(d));
        tracing::info!(
            secs = d.as_secs(),
            "conversation: the folder lookup is asked again after the backoff"
        );
    }

    /// The Kai folder's threads came: the replies collected meanwhile notify.
    fn threads_known(&mut self, threads: Vec<i64>) {
        self.convo.kai_threads = threads.into_iter().collect();
        self.convo.threads_known = true;
        self.lookup_at = None;
        self.lookup_delay = FIRST_RETRY;
    }

    /// Retry now with the backoff reset (a new network, the app visible, Try again).
    fn retry_now(&mut self) {
        if self.link.is_none() || !self.net.available {
            return;
        }
        self.delay = FIRST_RETRY;
        self.stopped = false;
        if self.phase == Phase::Idle {
            self.retry_at = Some(self.now());
        }
    }

    // ------------------------------------------------------------------ the connection

    fn drop_connection(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
        if matches!(self.phase, Phase::Open { up: true, .. }) {
            self.convo.disconnected();
        }
        self.phase = Phase::Idle;
        self.dead_at = None;
        self.lookup_at = None;
        self.lookup_delay = FIRST_RETRY;
        self.caught_up = false;
    }

    fn connect(&mut self) {
        if self.link.is_none() {
            return;
        }
        if !self.net.available {
            self.said = Said::no_network();
            return;
        }
        self.drop_connection();
        let Some(link) = self.link.as_ref() else {
            return;
        };
        // The phone's own date decides a lapse (device link §5.3), checked before each try; a
        // handshake's alert never does.
        if link.file.credentials.state(self.ctx.clock.wall_ms()) == CredentialState::Lapsed {
            self.said = Said::lapsed(&date(link.file.credentials.not_after_ms));
            self.stopped = true;
            return;
        }
        let settings = self.settings();
        self.generation += 1;
        let timeout = self.timeout();
        self.keepalive_asked = settings.feed.keepalive_s;
        let reader = FeedReader::new(link.file.cursor.as_deref())
            .asking(Some(settings.feed.keepalive_s))
            .knowing(self.approvals.known());
        let shared = (self.unconfirmed && self.visible)
            .then(|| link.file.last_good.clone())
            .flatten();
        let plan = Plan {
            generation: self.generation,
            tls: link.tls.clone(),
            urls: link.order(),
            shared,
            stagger: Duration::from_millis(u64::from(settings.lmgw.url_stagger_ms)),
            timeout,
            reader,
        };
        if matches!(
            self.said.state,
            ConnectionState::NotPaired | ConnectionState::Connected
        ) {
            self.said = Said::connecting();
        }
        tracing::debug!(generation = self.generation, "link: connecting the feed");
        self.task = Some(tokio::spawn(feed::run(plan, self.ctx.cmds.clone())));
        self.phase = Phase::Racing;
        // The race's bound on the boot clock too (a sleep stalls tokio's timer), a second past
        // `lmgw.timeout_s` so that the race's own bound, which says each URL's failure, reports
        // first when the phone is awake.
        self.dead_at = Some(self.now() + ms(timeout) + 1000);
    }

    fn feed(&mut self, generation: u64, msg: FeedMsg) {
        if generation != self.generation || self.link.is_none() {
            return;
        }
        let timeout = self.timeout();
        match msg {
            FeedMsg::Answered { url } => {
                self.phase = Phase::Open {
                    began: false,
                    up: false,
                };
                self.dead_at = Some(self.now() + ms(timeout));
                if let Some(link) = self.link.as_mut()
                    && link.file.last_good.as_deref() != Some(url.as_str())
                {
                    tracing::info!(%url, "link: reached lmgw at this address");
                    link.file.last_good = Some(url);
                    self.save_link();
                }
            }
            FeedMsg::Failed { failures, .. } => {
                self.task = None;
                self.phase = Phase::Idle;
                self.dead_at = None;
                let (said, stop) = of_failures(&failures, timeout.as_secs());
                for (url, f) in &failures {
                    if let Failure::Alert(a) = f {
                        tracing::warn!(%url, alert = %a, "link: an alert about the device's certificate before the server proved itself; tried again");
                    }
                }
                if let Some((_, Failure::KeyFailed(why))) = failures
                    .iter()
                    .find(|(_, f)| matches!(f, Failure::KeyFailed(_)))
                    && !std::mem::replace(&mut self.keystore_noticed, true)
                {
                    tracing::warn!(%why, "link: the Keystore could not sign; tried again");
                    self.emit(Update::Notice {
                        text: format!(
                            "The Android Keystore could not sign for lmgw's connection ({why}). \
                             Kai keeps the pairing and tries again."
                        ),
                    });
                }
                tracing::info!(state = ?said.state, words = %said.words, "link: no URL answered the feed");
                self.said = said;
                if stop {
                    self.stopped = true;
                }
                self.schedule();
            }
            FeedMsg::Refused { status, error } => {
                self.task = None;
                self.phase = Phase::Idle;
                self.dead_at = None;
                self.refused_feed(status, &error);
            }
            FeedMsg::Chunk(chunk) => self.chunk(*chunk),
            FeedMsg::Ended { why } => {
                let was_up = matches!(self.phase, Phase::Open { up: true, .. });
                self.task = None;
                self.drop_connection();
                if was_up {
                    self.delay = FIRST_RETRY;
                }
                tracing::info!(%why, "link: the feed ended");
                self.said = Said::unreachable(why);
                self.schedule();
            }
        }
    }

    /// lmgw refused the feed (§5.5), over the authenticated link.
    fn refused_feed(&mut self, status: u16, e: &ApiError) {
        let why = requests::key_refused(e);
        tracing::warn!(status, code = %e.code, ?why, "link: lmgw refused the feed");
        self.unconfirmed = why == Some(KeyRefused::Unconfirmed);
        if self.unconfirmed {
            let code = self
                .link
                .as_ref()
                .map(|l| l.file.credentials.confirm_code.clone())
                .unwrap_or_default();
            self.said = Said::unconfirmed(&code);
            self.schedule();
            return;
        }
        if status == 401 || why == Some(KeyRefused::CredentialOnDeviceListener) {
            self.key_refused(why, &e.code, &e.message);
            return;
        }
        self.said = Said::unreachable(format!(
            "lmgw refused the feed (HTTP {status}, {}: {})",
            e.code, e.message
        ));
        self.schedule();
    }

    /// A key refusal (a 401 by code, a 400 credential), wherever it came from.
    fn key_refused(&mut self, why: Option<KeyRefused>, code: &str, message: &str) {
        match why {
            Some(KeyRefused::CertificateExpired) => {
                let d = self
                    .link
                    .as_ref()
                    .map(|l| date(l.file.credentials.not_after_ms))
                    .unwrap_or_default();
                self.said = Said::lapsed(&d);
                self.stopped = true;
            }
            Some(KeyRefused::CertificateRequired | KeyRefused::CredentialOnDeviceListener) => {
                // A bug in Kai, never the owner's to fix: said, then the backoff (§5.5).
                self.said = Said::refused(KeyRefusal::Bug, code, message);
                self.schedule();
                return;
            }
            Some(KeyRefused::Disabled) => {
                self.said = Said::refused(KeyRefusal::Disabled, code, message);
                self.stopped = true;
            }
            Some(KeyRefused::Expired) => {
                self.said = Said::refused(KeyRefusal::Expired, code, message);
                self.stopped = true;
            }
            Some(KeyRefused::PairAgain | KeyRefused::TokenUsedElsewhere) => {
                self.said = Said::refused(KeyRefusal::PairAgain, code, message);
                self.stopped = true;
            }
            _ => {
                // A code this build does not know (a 401 by another code): lmgw's sentence in a
                // notice, then the backoff, never a stop (§5.5's unknown kind, as `revoked`'s).
                self.drop_connection();
                self.said = Said::refused(KeyRefusal::Unknown, code, message);
                self.emit(Update::Notice {
                    text: format!("lmgw refused this device: {message} ({code})"),
                });
                self.schedule();
                return;
            }
        }
        self.drop_connection();
        self.retry_at = None;
    }

    /// A revocation's kind (§5.5): the feed's `revoked`.
    fn revoked(&mut self, kind: &RevokeKind, message: &str) {
        let code = kind.as_str().to_owned();
        match kind {
            RevokeKind::CertificateExpired => {
                self.key_refused(Some(KeyRefused::CertificateExpired), &code, message)
            }
            RevokeKind::DeviceDisabled => {
                self.key_refused(Some(KeyRefused::Disabled), &code, message)
            }
            RevokeKind::KeyExpired => self.key_refused(Some(KeyRefused::Expired), &code, message),
            RevokeKind::KeyUnknown | RevokeKind::Revoked => {
                self.key_refused(Some(KeyRefused::PairAgain), &code, message)
            }
            _ => {
                // An unknown kind: lmgw's sentence, then the backoff, never started over (K4-7).
                self.drop_connection();
                self.said = Said::refused(KeyRefusal::Unknown, &code, message);
                self.emit(Update::Notice {
                    text: format!("lmgw ended the feed: {message}"),
                });
                self.schedule();
            }
        }
    }

    fn chunk(&mut self, chunk: Chunk) {
        let now = self.now();
        let timeout = self.timeout();
        let (began, mut up) = match self.phase {
            Phase::Open { began, up } => (began, up),
            _ => (false, false),
        };
        let began = began || chunk.bytes > 0;
        let mut hello = false;
        let mut caught_up_now = false;
        for item in chunk.items {
            match item {
                FeedItem::Event(FeedEvent::Hello(h)) => {
                    hello = true;
                    up = true;
                    self.unconfirmed = false;
                    self.keystore_noticed = false;
                    self.convo.own_by = Some(h.principal.by());
                    self.convo.live(&h.voice, &h.turns);
                    self.device_name = Some(h.principal.name.clone());
                    self.held = h.hold.active;
                    self.hold_fallback = h.hold.fallback_alias.clone();
                    self.stated = Some(h.keepalive_s);
                    tracing::info!(device = %h.principal.name, keepalive_s = h.keepalive_s, held = h.hold.active, "link: the feed is up");
                }
                FeedItem::Event(FeedEvent::CaughtUp(_)) => caught_up_now = true,
                FeedItem::Event(FeedEvent::Revoked(r)) => {
                    let message = if r.message.is_empty() {
                        r.reason.clone()
                    } else {
                        r.message.clone()
                    };
                    tracing::warn!(kind = %r.kind, "link: revoked");
                    self.revoked(&r.kind, &message);
                    return;
                }
                FeedItem::Event(ev) => self.event(ev),
                FeedItem::Approvals(change) => {
                    self.approvals.set_open(chunk.open.clone());
                    let out = self.approvals.change(change, self.seen());
                    self.approvals_out(out);
                }
                FeedItem::Unknown { event, data } => {
                    tracing::debug!(%event, bytes = data.len(), "link: an event this build does not know, skipped")
                }
                FeedItem::Unreadable { event, data } => {
                    tracing::warn!(%event, bytes = data.len(), "link: an event whose data does not read; reloading as for resync");
                    if up {
                        self.reload(false);
                    }
                }
                FeedItem::KeepAlive => {}
                _ => tracing::debug!("link: a feed item this build does not know, skipped"),
            }
        }
        // A new reader's set is empty until its `hello`: taken from then on, so a chunk before it
        // (a split `hello`) drops neither the cards nor a held verdict.
        if up && self.approvals.set_open(chunk.open) {
            self.publish_approvals();
        }
        self.phase = Phase::Open { began, up };
        // Device facts (device link §7.2): hello.urls replaces the list, hello.cert the dates.
        if hello {
            self.delay = FIRST_RETRY;
            self.dashboard_url = chunk.dashboard_url.clone();
            if let Some(link) = self.link.as_mut() {
                let mut changed = false;
                if let Some(urls) = chunk.urls.clone()
                    && urls != link.file.credentials.urls
                {
                    link.file.credentials.urls = urls;
                    changed = true;
                }
                if let Some(cert) = chunk.cert
                    && (cert.not_after_ms, cert.notice_after_ms)
                        != (
                            link.file.credentials.not_after_ms,
                            link.file.credentials.notice_after_ms,
                        )
                {
                    link.file.credentials.not_after_ms = cert.not_after_ms;
                    link.file.credentials.notice_after_ms = cert.notice_after_ms;
                    changed = true;
                }
                if changed {
                    self.save_link();
                }
            }
            let name = self.device_name.clone().unwrap_or_default();
            self.said = Said::connected(&name);
            self.stopped = false;
            self.reload(false);
        }
        if caught_up_now {
            self.caught_up = true;
            let out = self.approvals.caught_up();
            self.approvals_out(out);
            self.flush_replies();
            self.open_known = true;
            for d in std::mem::take(&mut self.held_decisions) {
                let r = self.decide(&d.id, d.approve);
                let _ = d.reply.send(r);
            }
        }
        // The cursor: the last stored event applied (§5.4).
        if let Some(c) = chunk.cursor
            && let Some(link) = self.link.as_mut()
            && link.file.cursor.as_deref() != Some(c.as_str())
        {
            link.file.cursor = Some(c);
            self.save_link();
        }
        let interval = |k: Option<u32>| {
            ms(Duration::from_secs(u64::from(
                k.map_or(timeout.as_secs() as u32, |k| k.max(1)),
            )))
        };
        self.dead_at = Some(match (up, began) {
            (true, _) => now + 2 * interval(self.stated),
            (false, false) => now + ms(timeout),
            (false, true) => now + 2 * interval(self.stated),
        });
    }

    fn event(&mut self, ev: FeedEvent) {
        match &ev {
            FeedEvent::State(s) => {
                self.held = s.hold.active;
                self.hold_fallback = s.hold.fallback_alias.clone();
                self.convo.live(&s.voice, &s.turns);
                self.publish_conversation();
                return;
            }
            FeedEvent::Hold(h) => {
                self.held = h.active;
                self.hold_fallback = h.fallback_alias.clone();
                return;
            }
            FeedEvent::Resync(r) => {
                tracing::warn!(reason = %r.reason, "link: resync; reloading the conversation");
                self.reload(false);
                return;
            }
            FeedEvent::MessageAdded(m) | FeedEvent::MessageUpdated(m) => {
                let updated = matches!(ev, FeedEvent::MessageUpdated(_));
                let collect = !self.caught_up || !self.convo.threads_known;
                let convo = &self.convo;
                let quiet = self.visible || self.in_call;
                let ups = self
                    .replies
                    .message(m, updated, collect, quiet, |t| convo.is_kai_thread(t));
                self.emit_all(ups);
            }
            FeedEvent::MessageDeleted(m) => {
                let ups = self.replies.deleted(m);
                self.emit_all(ups);
            }
            // lmgw decided otherwise than the verdict the phone sent (the owner's own words came
            // first, another client did, its turn could not start): the owner is told (§11).
            FeedEvent::ApprovalDecided(d) => {
                if let Some(text) = self.approvals.feed_decided(d, self.convo.own_by.as_deref()) {
                    tracing::info!(id = %d.approval_request_id, approve = d.approve, not_run = d.not_run, "approvals: lmgw decided otherwise than the phone's verdict");
                    self.emit(Update::Notice { text });
                }
            }
            _ => {}
        }
        let asks = self.convo.event(&ev);
        if let Some(d) = asks.delta {
            self.emit(Update::Messages { delta: d });
        }
        if asks.reload {
            self.reload(false);
        } else if let Some(t) = asks.thread {
            self.load_thread(t);
        }
        if asks.view {
            self.publish_conversation();
        }
    }

    // ------------------------------------------------------------------ the conversation

    /// Looks the folder up and asks `current` (§6.1). `probe`: the app became visible, and a
    /// failure reopens the feed at once (§12.2).
    fn reload(&mut self, probe: bool) {
        let Some(link) = self.link.as_ref() else {
            return;
        };
        if !matches!(self.phase, Phase::Open { up: true, .. }) && !probe {
            return;
        }
        self.convo.load_gen += 1;
        let generation = self.convo.load_gen;
        if self.convo.status.is_none() || self.convo.status == Some(ConversationStatus::Failed) {
            self.convo.status = Some(ConversationStatus::Loading);
            self.publish_conversation();
        }
        let h = link.handle();
        let name = self.settings().conversation.folder_name.clone();
        let remembered = link.file.folder_id;
        let timeout = self.timeout();
        let tx = self.ctx.cmds.clone();
        tokio::spawn(async move {
            let identity = h.identity();
            let loaded =
                crate::conversation::load::load(&h, &name, remembered, false, timeout).await;
            let _ = tx.send(Cmd::Loaded {
                generation,
                identity,
                loaded: Box::new(loaded),
                probe,
            });
        });
    }

    fn load_thread(&mut self, thread: i64) {
        let Some(link) = self.link.as_ref() else {
            return;
        };
        self.convo.load_gen += 1;
        let generation = self.convo.load_gen;
        let h = link.handle();
        let timeout = self.timeout();
        let tx = self.ctx.cmds.clone();
        tokio::spawn(async move {
            let identity = h.identity();
            let result = crate::conversation::load::load_thread(&h, thread, timeout).await;
            let _ = tx.send(Cmd::ThreadLoaded {
                generation,
                identity,
                result: Box::new(result),
            });
        });
    }

    fn identity(&self) -> Option<[u8; 32]> {
        self.link.as_ref().map(|l| l.tls.identity())
    }

    fn loaded(&mut self, generation: u64, identity: [u8; 32], loaded: Loaded, probe: bool) {
        if Some(identity) != self.identity() {
            return;
        }
        let fresh = generation == self.convo.load_gen;
        match loaded {
            Loaded::Ready {
                folder,
                remember,
                detail,
                threads,
                rolled_over,
            } => {
                if !fresh {
                    return;
                }
                self.remember(remember);
                match threads {
                    Some(t) => self.threads_known(t),
                    // The thread list did not read: the replies wait, the lookup is asked again.
                    None => self.schedule_lookup(),
                }
                tracing::info!(
                    folder = folder.id,
                    thread = detail.thread.row.id,
                    rolled_over,
                    "conversation: ready"
                );
                self.convo.folder = Some(*folder);
                self.convo.status = Some(ConversationStatus::Ready);
                self.convo.steps = None;
                self.convo.note = None;
                let delta = self.convo.thread_loaded(&detail);
                self.emit(Update::Messages { delta });
                if self.visible {
                    let ups = self.replies.opened(detail.thread.row.id);
                    self.emit_all(ups);
                }
                self.flush_replies();
            }
            Loaded::NotReady {
                folder,
                remember,
                steps,
                threads,
            } => {
                if !fresh {
                    return;
                }
                self.remember(remember);
                match threads {
                    Some(t) => self.threads_known(t),
                    None => self.schedule_lookup(),
                }
                self.convo.folder = Some(*folder);
                self.convo.status = Some(ConversationStatus::NotReady);
                self.convo.steps = Some(steps);
                self.flush_replies();
            }
            Loaded::Failed(f) => {
                if let Fail::Refused { status, error } = &f
                    && self.request_refused(identity, *status, error)
                {
                    return;
                }
                if probe
                    && matches!(f, Fail::NoAnswer(_))
                    && matches!(self.phase, Phase::Open { up: true, .. })
                {
                    // The held feed's path may be gone (§12.2): reopen it at once.
                    tracing::info!("link: the visible app's probe failed; reopening the feed");
                    self.drop_connection();
                    self.retry_now();
                }
                if fresh {
                    self.convo.status = Some(ConversationStatus::Failed);
                    self.convo.note = Some(format!(
                        "Kai could not look up its folder '{}' on lmgw: {}",
                        self.settings().conversation.folder_name,
                        f.words(self.timeout())
                    ));
                    // The replies keep waiting for the folder's threads (their notifications need
                    // them, §12.3): the lookup is asked again on its backoff.
                    self.schedule_lookup();
                }
            }
        }
        self.publish_conversation();
    }

    fn remember(&mut self, remember: Option<i64>) {
        if let Some(link) = self.link.as_mut()
            && link.file.folder_id != remember
        {
            link.file.folder_id = remember;
            self.save_link();
        }
    }

    fn thread_loaded(
        &mut self,
        generation: u64,
        identity: [u8; 32],
        result: Result<ThreadDetail, Fail>,
    ) {
        if Some(identity) != self.identity() || generation != self.convo.load_gen {
            return;
        }
        match result {
            Ok(detail) => {
                let delta = self.convo.thread_loaded(&detail);
                self.emit(Update::Messages { delta });
                self.publish_conversation();
            }
            Err(Fail::Refused { status, error }) => {
                if !self.request_refused(identity, status, &error) {
                    self.reload(false);
                }
            }
            Err(_) => self.reload(false),
        }
    }

    fn flush_replies(&mut self) {
        if !self.caught_up || !self.convo.threads_known {
            return;
        }
        let convo = &self.convo;
        let ups = self
            .replies
            .flush(self.visible || self.in_call, |t| convo.is_kai_thread(t));
        self.emit_all(ups);
    }

    /// A request's refusal under `identity`: acted on (and `true`) when it is a key refusal of the
    /// current pairing; a stale pairing's is ignored.
    fn request_refused(&mut self, identity: [u8; 32], status: u16, e: &ApiError) -> bool {
        let why = requests::key_refused(e);
        let key = status == 401 || why == Some(KeyRefused::CredentialOnDeviceListener);
        if !key && why != Some(KeyRefused::Unconfirmed) {
            return false;
        }
        if Some(identity) != self.identity() {
            tracing::info!(code = %e.code, "link: a refusal of an earlier pairing's request, ignored");
            return true;
        }
        if why == Some(KeyRefused::Unconfirmed) {
            self.drop_connection();
            self.refused_feed(status, e);
        } else {
            self.key_refused(why, &e.code, &e.message);
        }
        true
    }

    // ------------------------------------------------------------------ commands

    fn cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Feed { generation, msg } => self.feed(generation, msg),
            Cmd::Wake | Cmd::Shutdown => {}
            Cmd::Retry => {
                tracing::info!("link: try again");
                self.unconfirmed_poll_or_retry();
            }
            Cmd::Network(net) => self.network(net),
            Cmd::Foreground(visible) => self.foreground(visible),
            Cmd::Overlay(showing) => self.overlay(showing),
            Cmd::SettingsChanged => self.settings_changed(),
            Cmd::Dismiss { id, reply } => {
                let out = self.approvals.dismiss(&id);
                let save = out.save;
                self.approvals_out(approvals::Out { save: false, ..out });
                let written = if save {
                    self.write_approvals().map_err(|reason| CoreError::Storage {
                        what: "approvals.json".to_owned(),
                        reason,
                    })
                } else {
                    Ok(())
                };
                let _ = reply.send(written);
            }
            Cmd::Gone { kind, key } => match kind {
                NotificationKind::Approval => self.approvals.gone(&key),
                NotificationKind::Reply => {
                    if let Ok(t) = key.parse() {
                        self.replies.gone(t);
                    }
                }
                NotificationKind::Notice => {}
            },
            Cmd::Current { reply } => {
                let _ = reply.send((
                    self.link
                        .as_ref()
                        .map(|l| l.file.credentials.urls.as_slice()[0].clone()),
                    self.link.as_ref().map(|l| l.file.alias.clone()),
                ));
            }
            Cmd::Paired {
                enrolled,
                name,
                fp,
                reply,
            } => {
                let r = self.paired(*enrolled, name, fp);
                let _ = reply.send(r);
            }
            Cmd::Forget { reply } => {
                self.forget();
                let _ = reply.send(());
            }
            Cmd::SendStart { reply } => {
                let r = self.send_start();
                let _ = reply.send(r);
            }
            Cmd::SendFrame(frame) => self.send_frame(frame),
            Cmd::SendEnded { identity, result } => self.send_ended(identity, result),
            Cmd::NewStart { reply } => {
                let r = self.ready_handle().map(|h| {
                    self.convo.load_gen += 1;
                    // The screen shows the empty new conversation now, not when lmgw answers.
                    let delta = self.convo.cleared();
                    self.emit(Update::Messages { delta });
                    self.publish_conversation();
                    let name = self.settings().conversation.folder_name.clone();
                    let remembered = self.link.as_ref().and_then(|l| l.file.folder_id);
                    (h, name, remembered, self.convo.load_gen)
                });
                let _ = reply.send(r);
            }
            Cmd::Loaded {
                generation,
                identity,
                loaded,
                probe,
            } => self.loaded(generation, identity, *loaded, probe),
            Cmd::ThreadLoaded {
                generation,
                identity,
                result,
            } => self.thread_loaded(generation, identity, *result),
            Cmd::Decide { id, approve, reply } => self.decide_or_hold(id, approve, reply),
            Cmd::DecideEnded {
                identity,
                ids,
                result,
                resent,
            } => self.decide_ended(identity, ids, result, resent),
            Cmd::VoiceTicket { reply } => {
                let _ = reply.send(self.voice_ticket());
            }
            Cmd::VoiceThread(t) => {
                if self.convo.thread_id != Some(t) && self.link.is_some() {
                    tracing::info!(
                        thread = t,
                        "conversation: follows the thread the call bound"
                    );
                    self.load_thread(t);
                }
            }
            Cmd::CallRunning(call) => {
                self.in_call = call.is_some();
                let out = self.approvals.call_running(call);
                self.approvals_out(out);
            }
            Cmd::CallApproval(shown) => {
                tracing::info!(call = shown.call, id = %shown.request.approval_request_id, "approvals: the call's session shows a call waiting");
                let out = self.approvals.call_shown(*shown, self.seen());
                self.approvals_out(out);
            }
            Cmd::CallApprovalDecided { id } => {
                tracing::info!(%id, "approvals: decided by another client (the call's word)");
                let out = self.approvals.call_decided(&id);
                self.approvals_out(out);
            }
            Cmd::CallVerdicts(outcome) => self.call_verdicts(outcome),
            Cmd::VoiceRevoked { kind, message } => {
                tracing::warn!(kind = %kind.as_str(), "link: a call ended as revoked");
                self.revoked(&kind, &message);
            }
            Cmd::VoiceRefused {
                identity,
                status,
                error,
            } => {
                self.request_refused(identity, status, &error);
            }
            Cmd::Reload => self.reload(false),
        }
    }

    /// What a call needs of the link (§7.3 step 1), or the state's words when voice cannot start:
    /// not paired, no network, a refusal that stopped the link, the owner's Confirm still owed. A
    /// link that is only reconnecting does not hold the call back: the POST races on its own.
    fn voice_ticket(&self) -> Result<Ticket, CoreError> {
        let Some(link) = self.link.as_ref() else {
            return Err(CoreError::NotConnected {
                reason: Said::not_paired().words,
            });
        };
        if !self.net.available {
            return Err(CoreError::NotConnected {
                reason: Said::no_network().words,
            });
        }
        let refused = matches!(
            self.said.state,
            ConnectionState::WaitingForConfirmation
                | ConnectionState::KeyRefused
                | ConnectionState::KeyMissing
                | ConnectionState::CertificateLapsed
                | ConnectionState::NotThisLmgw
        );
        if self.stopped || refused {
            return Err(CoreError::NotConnected {
                reason: self.said.words.clone(),
            });
        }
        if self.convo.status == Some(ConversationStatus::NotReady) {
            return Err(CoreError::NotReady {
                reason: self.convo.steps.clone().unwrap_or_default(),
            });
        }
        let view = self.convo.view(&self.settings().conversation.folder_name);
        Ok(Ticket {
            tls: link.tls.clone(),
            urls: link.order(),
            last_good: link.file.last_good.clone(),
            folder: self
                .convo
                .folder
                .as_ref()
                .map(|f| f.id)
                .or(link.file.folder_id),
            folder_name: self.settings().conversation.folder_name.clone(),
            voice_by: view.voice_by.clone(),
            thread: view.thread_id.zip(view.thread_title.clone()),
            held: self.held,
            hold_fallback: self.hold_fallback.clone(),
        })
    }

    fn unconfirmed_poll_or_retry(&mut self) {
        if self.link.is_none() {
            return;
        }
        if self.phase != Phase::Idle && !self.stopped {
            // A connection is under way or up: nothing to try again.
            if matches!(self.phase, Phase::Open { up: true, .. }) {
                return;
            }
            self.drop_connection();
        }
        self.retry_now();
    }

    fn network(&mut self, net: NetworkFacts) {
        let changed = net.available != self.net.available || net.id != self.net.id;
        if !changed {
            self.net = net;
            return;
        }
        tracing::info!(available = net.available, transport = %net.transport, "link: the default network changed");
        let first = self.net.id.is_empty() && self.net.available && net.available;
        self.net = net;
        if !self.net.available {
            self.drop_connection();
            self.retry_at = None;
            if self.link.is_some() && !self.stopped {
                self.said = Said::no_network();
            }
            return;
        }
        if first && self.phase != Phase::Idle {
            // Kotlin's first report of the network the connection already uses.
            return;
        }
        // A new network: the pooled connections go, the URLs are raced again (§5.2).
        let timeout = self.timeout();
        if let Some(link) = self.link.as_mut()
            && let Ok(t) = link.tls.fresh(timeout)
        {
            link.tls = t;
        }
        if !self.stopped {
            self.drop_connection();
            self.retry_now();
        }
    }

    fn foreground(&mut self, visible: bool) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        tracing::debug!(visible, "link: the app's visibility changed");
        if visible {
            if matches!(self.phase, Phase::Open { up: true, .. }) {
                // `current` whenever the app becomes visible (§6.1), a probe of the path too.
                self.reload(true);
            } else if !self.stopped {
                // At once, the backoff reset (a try under way keeps going; its failure waits 1 s).
                self.delay = FIRST_RETRY;
                if self.phase == Phase::Idle {
                    self.retry_now();
                }
            }
            if let Some(t) = self.convo.thread_id {
                let ups = self.replies.opened(t);
                self.emit_all(ups);
            }
        } else if !self.overlay {
            let out = self.approvals.backgrounded();
            self.approvals_out(out);
        }
    }

    /// A card is on screen: the app is visible, or the assistant overlay shows the cards (§9.2).
    fn seen(&self) -> bool {
        self.visible || self.overlay
    }

    fn overlay(&mut self, showing: bool) {
        if showing == self.overlay {
            return;
        }
        let was = self.seen();
        self.overlay = showing;
        tracing::debug!(showing, "link: the overlay's approval cards");
        if was && !self.seen() {
            let out = self.approvals.backgrounded();
            self.approvals_out(out);
        }
    }

    fn settings_changed(&mut self) {
        let settings = self.settings();
        let timeout = Duration::from_secs(u64::from(settings.lmgw.timeout_s));
        if timeout != self.timeout_used {
            self.timeout_used = timeout;
            if let Some(link) = self.link.as_mut()
                && let Ok(t) = link.tls.fresh(timeout)
            {
                link.tls = t;
            }
        }
        if settings.feed.keepalive_s != self.keepalive_asked && self.link.is_some() && !self.stopped
        {
            self.drop_connection();
            self.retry_now();
        }
        if settings.conversation.folder_name != self.folder_name {
            // Another folder: the lookup runs again, remembering none of the old name's.
            self.folder_name = settings.conversation.folder_name.clone();
            self.remember(None);
            self.convo.reset();
            self.emit(Update::Messages {
                delta: MessagesDelta {
                    reset: true,
                    thread_id: None,
                    upsert: Vec::new(),
                    removed: Vec::new(),
                },
            });
            self.reload(false);
        }
        self.publish_conversation();
    }

    fn paired(
        &mut self,
        enrolled: Enrolled,
        name: String,
        fp: String,
    ) -> Result<PairingView, CoreError> {
        let same_gateway = self.link.as_ref().is_some_and(|o| o.file.fp == fp);
        let file = LinkFile {
            name: name.clone(),
            fp,
            alias: enrolled.alias.clone(),
            public_key: lmgw_client::base64::encode(&enrolled.spki),
            credentials: enrolled.credentials.clone(),
            last_good: None,
            cursor: None,
            folder_id: if same_gateway {
                self.link.as_ref().and_then(|o| o.file.folder_id)
            } else {
                None
            },
        };
        // Opened and written before anything of the old pairing goes: a pairing that does not
        // hold together, or a link.json that cannot be written, leaves the old one as it was (its
        // key included; the new key stays kept for the token, so Pair again sends it again).
        let link =
            Paired::open(file, self.ctx.keystore.clone(), self.timeout()).map_err(|reason| {
                CoreError::Storage {
                    what: "the new pairing".to_owned(),
                    reason,
                }
            })?;
        store::write(&self.ctx.data_dir.join(LINK_FILE), &link.file).map_err(|reason| {
            tracing::error!(%reason, "link: the new pairing's link.json could not be written");
            CoreError::Storage {
                what: "the new pairing (link.json)".to_owned(),
                reason,
            }
        })?;
        let old = self.link.take();
        // A re-pair ends the old link and forgets its gateway's thread first (kai §3.1).
        self.drop_connection();
        self.convo.reset();
        if !same_gateway {
            let mut ups = self.approvals.clear();
            ups.extend(self.replies.clear());
            self.emit_all(ups);
            self.save_approvals();
            self.open_known = false;
            self.drop_held_decisions("Kai paired with another lmgw: the decision was not sent.");
        }
        self.emit(Update::Messages {
            delta: MessagesDelta {
                reset: true,
                thread_id: None,
                upsert: Vec::new(),
                removed: Vec::new(),
            },
        });
        self.link = Some(link);
        if let Some(o) = old
            && o.file.alias != enrolled.alias
        {
            // lmgw keeps the old key only until the new one is confirmed; this phone uses the new.
            self.ctx.keystore.delete(o.file.alias);
        }
        self.ctx
            .paired
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.stopped = false;
        self.unconfirmed = !enrolled.confirmed;
        self.stated = None;
        self.device_name = None;
        self.dashboard_url = None;
        self.said = if enrolled.confirmed {
            Said::connecting()
        } else {
            Said::unconfirmed(&enrolled.credentials.confirm_code)
        };
        self.delay = FIRST_RETRY;
        self.retry_at = Some(self.now());
        self.publish_conversation();
        Ok(PairingView {
            device_name: name,
            confirm_code: enrolled.credentials.confirm_code.clone(),
            urls: enrolled.credentials.urls.as_slice().to_vec(),
            confirmed: enrolled.confirmed,
        })
    }

    fn forget(&mut self) {
        tracing::info!("link: forget gateway");
        self.drop_connection();
        self.retry_at = None;
        if let Some(link) = self.link.take() {
            self.ctx.keystore.delete(link.file.alias);
        }
        let mut ups = self.approvals.clear();
        ups.extend(self.replies.clear());
        self.emit_all(ups);
        self.open_known = false;
        self.drop_held_decisions("Kai forgot the gateway: the decision was not sent.");
        for f in [LINK_FILE, APPROVALS_FILE] {
            if let Err(e) = store::remove(&self.ctx.data_dir.join(f)) {
                tracing::warn!(error = %e, "link: could not remove a file of the pairing");
            }
        }
        self.ctx
            .paired
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.convo.reset();
        self.emit(Update::Messages {
            delta: MessagesDelta {
                reset: true,
                thread_id: None,
                upsert: Vec::new(),
                removed: Vec::new(),
            },
        });
        self.stopped = false;
        self.unconfirmed = false;
        self.device_name = None;
        self.said = Said::not_paired();
        self.publish_conversation();
        self.publish_approvals();
    }

    /// The link and the conversation's thread, or why a send cannot go now (§5.3: at once, with
    /// the state's words).
    fn ready_handle(&self) -> Result<Handle, CoreError> {
        let Some(link) = self.link.as_ref() else {
            return Err(CoreError::NotConnected {
                reason: Said::not_paired().words,
            });
        };
        if !matches!(self.phase, Phase::Open { up: true, .. }) {
            return Err(CoreError::NotConnected {
                reason: self.said.words.clone(),
            });
        }
        match self.convo.status {
            Some(ConversationStatus::Ready) => Ok(link.handle()),
            Some(ConversationStatus::NotReady) => Err(CoreError::NotReady {
                reason: self.convo.steps.clone().unwrap_or_default(),
            }),
            Some(ConversationStatus::Failed) => Err(CoreError::NotReady {
                reason: self.convo.note.clone().unwrap_or_default(),
            }),
            _ => Err(CoreError::NotReady {
                reason: format!(
                    "Kai is still looking up its folder '{}' on lmgw.",
                    self.settings().conversation.folder_name
                ),
            }),
        }
    }

    fn send_start(&mut self) -> Result<(Handle, i64), CoreError> {
        let h = self.ready_handle()?;
        let thread = self.convo.thread_id.ok_or_else(|| CoreError::NotReady {
            reason: "the conversation has no thread yet".to_owned(),
        })?;
        if self.convo.streaming.is_some() {
            return Err(CoreError::NotReady {
                reason: "a send is still streaming".to_owned(),
            });
        }
        self.convo.unsaved = None;
        self.convo.streaming = Some(Streaming {
            thread_id: thread,
            ..Default::default()
        });
        self.publish_conversation();
        Ok((h, thread))
    }

    fn send_frame(&mut self, frame: Frame) {
        if let Some((delta, done)) = self.convo.frame(frame) {
            self.emit(Update::Messages { delta });
            if done {
                self.publish_conversation();
            }
        }
    }

    fn send_ended(&mut self, identity: [u8; 32], result: Result<(), StreamFail>) {
        if let Err(StreamFail::Refused { status, error }) = &result {
            self.request_refused(identity, *status, error);
        }
        if let Some(s) = self.convo.streaming.take() {
            // The stream ended without its `done`: what came stays, with why.
            let mut s = s;
            if s.error.is_none() {
                s.error = Some(match &result {
                    Err(StreamFail::Refused { error, .. }) => {
                        format!("{} ({})", error.message, error.code)
                    }
                    Err(StreamFail::NoAnswer(n)) => n.words(self.timeout()),
                    Err(StreamFail::Broke(why)) => format!("the reply broke off: {why}"),
                    Ok(()) => "the reply ended without its done".to_owned(),
                });
            }
            let mut v = streaming_view(&s);
            v.streaming = false;
            self.convo.unsaved = Some(s.clone());
            self.emit(Update::Messages {
                delta: MessagesDelta {
                    reset: false,
                    thread_id: Some(s.thread_id),
                    upsert: vec![v],
                    removed: Vec::new(),
                },
            });
        }
        self.publish_conversation();
    }

    /// A decision before the first `caught_up` since start is held (a notification's action that
    /// started the process: the open set is not known yet, and "decided elsewhere" would be a
    /// guess), bounded by `lmgw.timeout_s`; when the link is stopped or there is no network it is
    /// refused at once with the state's words.
    fn decide_or_hold(&mut self, id: String, approve: bool, reply: DecideReply) {
        if self.link.is_none() || self.open_known {
            let r = self.decide(&id, approve);
            let _ = reply.send(r);
            return;
        }
        if self.stopped || !self.net.available {
            let _ = reply.send(Err(CoreError::NotConnected {
                reason: format!(
                    "Kai is not connected to lmgw, so the decision was not sent; the call still \
                     waits. {}",
                    self.said.words
                ),
            }));
            return;
        }
        tracing::info!(%id, "approvals: a decision before the open set is known; held until caught_up");
        let until = self.now() + ms(self.timeout());
        self.held_decisions.push(HeldDecision {
            id,
            approve,
            reply,
            until,
        });
    }

    /// Every held decision refused with `reason` (Forget gateway, another gateway).
    fn drop_held_decisions(&mut self, reason: &str) {
        for d in std::mem::take(&mut self.held_decisions) {
            let _ = d.reply.send(Err(CoreError::NotConnected {
                reason: reason.to_owned(),
            }));
        }
    }

    fn decide(
        &mut self,
        id: &str,
        approve: bool,
    ) -> Result<(Decided, Option<Verdicts>), CoreError> {
        let Some(link) = self.link.as_ref() else {
            return Err(CoreError::NotConnected {
                reason: Said::not_paired().words,
            });
        };
        let h = link.handle();
        let d = self
            .approvals
            .decide(id, approve, self.seen())
            .map_err(|reason| CoreError::NotReady { reason })?;
        self.emit_all(d.updates);
        if d.save {
            self.save_approvals();
        }
        self.publish_approvals();
        match (d.send, d.via_call) {
            (None, _) => Ok((
                Decided::Waiting {
                    remaining: d.remaining,
                },
                None,
            )),
            (Some(verdicts), Some(call)) => {
                // The session that showed them answers them, so Kai speaks the resumed reply in
                // the call (§11); the voice task hands them back when the call cannot.
                tracing::info!(
                    call,
                    n = verdicts.len(),
                    "approvals: the verdicts go through the call"
                );
                let _ = self.ctx.voice.send(VoiceCmd::Verdicts {
                    call,
                    thread_id: d.thread_id,
                    verdicts,
                });
                Ok((Decided::Sent, None))
            }
            (Some(verdicts), None) => Ok((Decided::Sent, Some((h, d.thread_id, verdicts)))),
        }
    }

    /// What became of verdicts given to the call (§11).
    fn call_verdicts(&mut self, outcome: CallVerdicts) {
        match outcome {
            CallVerdicts::Taken(ids) => {
                // The phone's inference only: the cards stay sent until the feed's
                // approval.decided closes them and says how lmgw decided.
                tracing::info!(
                    ?ids,
                    "approvals: the response after the verdicts ran; they stay sent until the feed says"
                );
            }
            CallVerdicts::NotTaken { ids, code, words } => {
                tracing::info!(?ids, ?code, "approvals: the call did not take the verdicts");
                self.verdicts_refused(ids, code.as_deref(), &words, false);
            }
            CallVerdicts::Unsent {
                thread_id,
                verdicts,
            } => {
                tracing::info!(
                    thread_id,
                    "approvals: the call could not take the verdicts; they go by the route"
                );
                self.post_verdicts(thread_id, verdicts, false);
            }
            CallVerdicts::Unsettled {
                thread_id,
                verdicts,
            } => {
                tracing::info!(
                    thread_id,
                    "approvals: the call ended before lmgw answered the verdicts; sent again by the route"
                );
                self.post_verdicts(thread_id, verdicts, true);
            }
        }
    }

    /// The verdicts by the route, from the actor (the call handed them back).
    fn post_verdicts(&mut self, thread_id: i64, verdicts: Vec<(String, bool)>, resent: bool) {
        let Some(link) = self.link.as_ref() else {
            let ids: Vec<String> = verdicts.into_iter().map(|(id, _)| id).collect();
            let out = self.approvals.not_taken(
                &ids,
                Some(format!(
                    "the verdict was not sent: {}",
                    Said::not_paired().words
                )),
                false,
                self.seen(),
            );
            self.approvals_out(out);
            return;
        };
        spawn_verdicts(
            link.handle(),
            thread_id,
            verdicts,
            self.timeout(),
            self.ctx.cmds.clone(),
            resent,
        );
    }

    fn decide_ended(
        &mut self,
        identity: [u8; 32],
        ids: Vec<String>,
        result: Result<(), StreamFail>,
        resent: bool,
    ) {
        match result {
            Ok(()) => {}
            Err(StreamFail::Refused { status, error }) => {
                if self.request_refused(identity, status, &error) {
                    return;
                }
                self.verdicts_refused(ids, Some(&error.code), &error.message, resent);
            }
            Err(e) => {
                let why = match e {
                    StreamFail::NoAnswer(n) => n.words(self.timeout()),
                    StreamFail::Broke(w) => format!("the answer broke off: {w}"),
                    StreamFail::Refused { .. } => unreachable!("handled above"),
                };
                let words = format!("The decision may not have reached lmgw: {why}");
                if !self.seen() {
                    self.emit(Update::Notice {
                        text: words.clone(),
                    });
                }
                let out = self
                    .approvals
                    .not_taken(&ids, Some(words), false, self.seen());
                self.approvals_out(out);
            }
        }
    }

    /// lmgw refused verdicts (the route's answer or the call's): `approval_decided` closes the
    /// cards ("decided on <by>" in lmgw's words; nothing for verdicts sent again after the call,
    /// which lmgw most likely took there: the feed's `approval.decided` says if it decided
    /// otherwise); anything else asks again with lmgw's words on the card, and in a notice while
    /// no card is seen. A code that does not say lmgw refused before deciding says it may have
    /// decided (`approvals::refusal_note`).
    fn verdicts_refused(
        &mut self,
        ids: Vec<String>,
        code: Option<&str>,
        message: &str,
        resent: bool,
    ) {
        if code == Some("approval_decided") {
            if !resent {
                self.emit(Update::Notice {
                    text: format!("Already decided: {message}"),
                });
                self.approvals.told(&ids);
            }
            let out = self.approvals.verdicts_taken(&ids);
            self.approvals_out(out);
            return;
        }
        let refused = approvals::refused_before_deciding(code);
        let note = approvals::refusal_note(code, message);
        if !self.seen() {
            self.emit(Update::Notice { text: note.clone() });
        }
        let out = self
            .approvals
            .not_taken(&ids, Some(note), refused, self.seen());
        self.approvals_out(out);
    }

    fn approvals_out(&mut self, out: approvals::Out) {
        self.emit_all(out.updates);
        if out.save {
            self.save_approvals();
        }
        if out.cards {
            self.publish_approvals();
        }
    }

    // ------------------------------------------------------------------ files and views

    fn save_link(&self) {
        if let Some(link) = self.link.as_ref()
            && let Err(e) = store::write(&self.ctx.data_dir.join(LINK_FILE), &link.file)
        {
            tracing::error!(error = %e, "link: link.json could not be written");
            self.emit(Update::Notice {
                text: format!("The pairing could not be saved: {e}"),
            });
        }
    }

    fn save_approvals(&self) {
        let _ = self.write_approvals();
    }

    fn write_approvals(&self) -> Result<(), String> {
        let f = ApprovalsFile {
            dismissed: self.approvals.dismissed(),
        };
        store::write(&self.ctx.data_dir.join(APPROVALS_FILE), &f).inspect_err(|e| {
            tracing::error!(error = %e, "link: approvals.json could not be written");
        })
    }

    fn publish_conversation(&self) {
        let name = self.settings().conversation.folder_name.clone();
        let view = self.convo.view(&name);
        let _ = self.ctx.voice.send(VoiceCmd::Conversation {
            thread: view.thread_id,
            answering: view.answering.clone(),
        });
        self.emit(Update::Conversation { view });
    }

    fn publish_approvals(&self) {
        self.emit(Update::Approvals {
            open: self.approvals.views(),
        });
        // What a call waits for (its thread's calls with no verdict sent): its words and its idle
        // clock (§7.7).
        let _ = self.ctx.voice.send(VoiceCmd::Approvals {
            waiting: self.approvals.waiting(),
        });
    }

    /// The connection's view and the wake-up, when they changed.
    fn publish(&mut self) {
        let hold = (self.held, self.hold_fallback.clone());
        if self.voice_hold.as_ref() != Some(&hold) {
            let _ = self.ctx.voice.send(VoiceCmd::Hold {
                held: hold.0,
                fallback: hold.1.clone(),
            });
            self.voice_hold = Some(hold);
        }
        let view = self.view();
        if self.published_view.as_ref() != Some(&view) {
            self.emit(Update::Connection { view: view.clone() });
            self.published_view = Some(view);
        }
        let wake = self.deadline();
        if self.published_wake != Some(wake) {
            self.emit(Update::WakeAt { boot_ms: wake });
            self.published_wake = Some(wake);
        }
    }

    fn view(&self) -> ConnectionView {
        let wall = self.ctx.clock.wall_ms();
        let now = self.now();
        let link = self.link.as_ref();
        let cert_notice = link.and_then(|l| match l.file.credentials.state(wall) {
            CredentialState::Notice => Some(format!(
                "This device's certificate lapses on {}; pair it again before then (New token… on \
                 lmgw's Devices card).",
                date(l.file.credentials.not_after_ms)
            )),
            _ => None,
        });
        let waiting = self.said.state == ConnectionState::WaitingForConfirmation;
        ConnectionView {
            state: self.said.state,
            words: self.said.words.clone(),
            detail: self.said.detail.clone(),
            actions: self.said.actions.clone(),
            refusal: self.said.refusal,
            paired: link.is_some(),
            confirm_code: link
                .filter(|_| waiting)
                .map(|l| l.file.credentials.confirm_code.clone()),
            device_name: self.device_name.clone(),
            next_try_at_ms: self
                .retry_at
                .filter(|_| self.phase == Phase::Idle)
                .map(|r| wall + i64::try_from(r.saturating_sub(now)).unwrap_or(i64::MAX)),
            held: self.held,
            hold_fallback: self.hold_fallback.clone(),
            cert_notice,
            gateway: link.map(|l| GatewayView {
                name: l.file.name.clone(),
                urls: l.file.credentials.urls.as_slice().to_vec(),
                last_good: l.file.last_good.clone(),
                fp_short: lmgw_client::pairing::Fingerprint::parse(&l.file.fp)
                    .map(|f| f.short())
                    .unwrap_or_else(|_| l.file.fp.clone()),
                not_after: date(l.file.credentials.not_after_ms),
                notice_after: date(l.file.credentials.notice_after_ms),
                confirm_code: l.file.credentials.confirm_code.clone(),
                dashboard_url: self.dashboard_url.clone(),
                key_alias: l.file.alias.clone(),
            }),
        }
    }
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}
