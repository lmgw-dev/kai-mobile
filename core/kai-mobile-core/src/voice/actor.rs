//! The voice task (design §3.1): it owns the call ([`Call`]), carries out its effects through
//! Kotlin's [`EventSink`] and the link's actor, runs the signalling ([`super::signal`]) as tasks of
//! its own that report back, and publishes the call's view and captions. Its deadlines are on the
//! boot clock, as the link's are.
//!
//! - **The link's facts** come with each call's [`Ticket`] (the pinned client, the URLs, the URL
//!   that last answered, the Kai folder, who else holds the thread's voice), and while a call runs
//!   from the link's conversation view (the thread, who else answers in it, K16) and its hold.
//! - **A stale call** (one that ended without lmgw ending it) is hung up over HTTP before the next
//!   call's POST (§7.8): the hangup answers once lmgw let its binding go, so the next bind is
//!   neither refused nor left pending by it. Every call lmgw may still hold is owed a hangup from
//!   lmgw's answer on (the live one too), kept in `calls.json` so a process that dies mid-call
//!   still hangs it up; a clean end (lmgw's word, the channel's close) and an answered hangup
//!   clear it.
//! - **The POST** goes to the URL that last answered the feed, else the one the warm-up's race
//!   found; a POST that cannot connect races the other URLs once. A 404 `chat_folder_not_found`
//!   looks the folder up again (§6.1's rule, which may create it) and POSTs once more.

use std::{path::PathBuf, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use lmgw_client::requests::{
    ApiError,
    calls::{self, CallAnswer, CallBind, Takeover},
};
use tokio::sync::{mpsc, oneshot, watch};

use super::{
    AudioFocus, CallStart, EventSink, HeadsetPress, PlayoutStats, TransportState,
    call::{Call, Effect, Facts},
    ends,
    signal::{self, Fail},
    view::VoiceView,
};
use crate::{
    CoreError, EchoMode, Settings,
    approvals::{CallVerdicts, Shown, Waiting},
    clock::Clock,
    conversation::load::{self, Loaded},
    link::{actor::Cmd as LinkCmd, http::Handle, tls::Tls},
    store::{self, CallsFile},
    views::Update,
};

/// What the link answers a call's trigger with (module doc).
#[derive(Debug, Clone)]
pub struct Ticket {
    pub tls: Tls,
    /// The URLs in the race's order (the one that last answered first).
    pub urls: Vec<String>,
    /// The URL that last answered the feed.
    pub last_good: Option<String>,
    /// The Kai folder: the conversation's, else the one the pairing remembers.
    pub folder: Option<i64>,
    pub folder_name: String,
    /// Another client holds the conversation's thread's voice (the feed's `voice`): a pending bind
    /// is expected.
    pub voice_by: Option<String>,
    pub thread: Option<(i64, String)>,
    pub held: bool,
    pub hold_fallback: Option<String>,
}

/// What the voice task is told.
pub enum Cmd {
    Prepare {
        ticket: oneshot::Receiver<Result<Ticket, CoreError>>,
    },
    Start {
        ticket: Box<Ticket>,
        start: CallStart,
        sink: Arc<dyn EventSink>,
        /// The call's number.
        reply: oneshot::Sender<Result<u64, CoreError>>,
    },
    Offer {
        call: u64,
        sdp: String,
        reply: oneshot::Sender<Result<String, CoreError>>,
    },
    /// The POST's outcome for call `n`.
    Posted {
        n: u64,
        result: Result<CallAnswer, Failed>,
    },
    /// An owed call's hangup went through (`done`), or did not.
    HungUp {
        call_id: String,
        done: bool,
    },
    /// The warm-up found its URL.
    Warmed {
        base: Option<String>,
    },
    /// libwebrtc's events, each naming its call (module doc).
    Transport {
        call: u64,
        state: TransportState,
        detail: String,
    },
    ChannelOpen {
        call: u64,
    },
    ChannelMessage {
        call: u64,
        text: String,
    },
    ChannelClosed {
        call: u64,
        why: String,
    },
    Failed {
        call: u64,
        words: String,
    },
    TakeOver,
    Press,
    Release,
    Tap,
    Stop,
    Muted(bool),
    PushToTalk(bool),
    Capturing {
        call: u64,
    },
    Focus {
        call: u64,
        focus: AudioFocus,
    },
    Headset(HeadsetPress),
    Stats {
        call: u64,
        request: u64,
        stats: PlayoutStats,
    },
    End,
    /// The assistant overlay that started call `call` was dismissed (§9.2).
    Dismissed {
        call: u64,
    },
    /// The link's conversation: its thread, who else answers in it.
    Conversation {
        thread: Option<i64>,
        answering: Vec<String>,
    },
    Hold {
        held: bool,
        fallback: Option<String>,
    },
    /// The calls waiting for the owner's verdict, by thread (the link's book, §11).
    Approvals {
        waiting: Vec<Waiting>,
    },
    /// The verdicts of a reply whose every call call `call`'s session showed (§11): through the
    /// call, or back to the link for the route when the call cannot take them.
    Verdicts {
        call: u64,
        thread_id: i64,
        verdicts: Vec<(String, bool)>,
    },
    Shutdown,
}

/// Why a POST did not give a call.
#[derive(Debug, Clone)]
pub enum Failed {
    /// lmgw refused, or did not answer: the request's failure.
    Request(Fail),
    /// The folder needs the owner's steps (§6.1).
    NotReady(String),
}

/// What the voice task shares with the core.
pub struct Ctx {
    pub clock: Arc<dyn Clock>,
    pub settings: Arc<ArcSwap<Settings>>,
    pub updates: mpsc::UnboundedSender<Update>,
    pub link: mpsc::UnboundedSender<LinkCmd>,
    pub cmds: mpsc::UnboundedSender<Cmd>,
    /// Where `calls.json` lives.
    pub data_dir: PathBuf,
}

struct Slot {
    call: Call,
    sink: Arc<dyn EventSink>,
    ticket: Ticket,
    offer: Option<oneshot::Sender<Result<String, CoreError>>>,
    warm: watch::Receiver<Option<Option<String>>>,
}

pub struct VoiceActor {
    ctx: Ctx,
    rx: mpsc::UnboundedReceiver<Cmd>,
    slot: Option<Slot>,
    n: u64,
    /// The call ids lmgw may still hold, hung up before the next POST (§7.8; module doc), as
    /// `calls.json` keeps them.
    owed: Vec<String>,
    published: Option<VoiceView>,
    /// The hold as the link last said it, for a call that starts.
    held: (bool, Option<String>),
    /// The calls waiting for the owner's verdict as the link last said them, for a call that starts.
    waiting: Vec<Waiting>,
}

impl VoiceActor {
    pub fn new(ctx: Ctx, rx: mpsc::UnboundedReceiver<Cmd>) -> Self {
        // A process that died mid-call left its call's id here: hung up before the first POST.
        let owed = match store::read::<CallsFile>(&ctx.data_dir.join(store::CALLS_FILE)) {
            Ok(f) => f.map(|f| f.owed).unwrap_or_default(),
            Err(why) => {
                tracing::warn!(%why, "voice: the owed call ids do not read; none is hung up");
                Vec::new()
            }
        };
        if !owed.is_empty() {
            tracing::info!(
                ?owed,
                "voice: calls of an earlier process are hung up before the next POST"
            );
        }
        Self {
            ctx,
            rx,
            slot: None,
            n: 0,
            owed,
            published: None,
            held: (false, None),
            waiting: Vec::new(),
        }
    }

    /// `id` is owed a hangup (lmgw answered its POST).
    fn owe(&mut self, id: &str) {
        if !self.owed.iter().any(|o| o == id) {
            self.owed.push(id.to_owned());
            self.write_owed();
        }
    }

    /// `id` needs no hangup any more (a clean end, or its hangup answered).
    fn settle(&mut self, id: &str) {
        let before = self.owed.len();
        self.owed.retain(|o| o != id);
        if self.owed.len() != before {
            self.write_owed();
        }
    }

    fn write_owed(&self) {
        let path = self.ctx.data_dir.join(store::CALLS_FILE);
        let file = CallsFile {
            owed: self.owed.clone(),
        };
        if let Err(why) = store::write(&path, &file) {
            tracing::warn!(%why, "voice: the owed call ids were not written; a process that dies now does not hang them up");
        }
    }

    fn now(&self) -> u64 {
        self.ctx.clock.boot_ms()
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(u64::from(self.ctx.settings.load().lmgw.timeout_s))
    }

    /// Runs until the core goes.
    pub async fn run(mut self) {
        loop {
            let now = self.now();
            let deadline = self.slot.as_ref().and_then(|s| s.call.deadline(now));
            let cmd = match deadline {
                Some(d) => tokio::select! {
                    c = self.rx.recv() => c,
                    () = tokio::time::sleep(Duration::from_millis(d.saturating_sub(now))) => {
                        let now = self.now();
                        if let Some(s) = self.slot.as_mut() {
                            s.call.tick(now);
                        }
                        self.carry_out();
                        continue;
                    }
                },
                None => self.rx.recv().await,
            };
            match cmd {
                None | Some(Cmd::Shutdown) => break,
                Some(cmd) => self.cmd(cmd),
            }
            self.carry_out();
        }
    }

    fn cmd(&mut self, cmd: Cmd) {
        let now = self.now();
        match cmd {
            Cmd::Prepare { ticket } => {
                let tx = self.ctx.cmds.clone();
                let stagger = self.stagger();
                let timeout = self.timeout();
                tokio::spawn(async move {
                    if let Ok(Ok(t)) = ticket.await {
                        let base = warm(&t, stagger, timeout).await;
                        let _ = tx.send(Cmd::Warmed { base });
                    }
                });
            }
            Cmd::Warmed { base } => {
                tracing::debug!(?base, "voice: prepared");
            }
            Cmd::Start {
                ticket,
                start,
                sink,
                reply,
            } => {
                let r = self.start(*ticket, start, sink, now);
                let _ = reply.send(r);
            }
            Cmd::Offer { call, sdp, reply } => self.offer(call, sdp, reply),
            Cmd::Posted { n, result } => self.posted(n, result, now),
            Cmd::HungUp { call_id, done } => {
                if done {
                    tracing::info!(%call_id, "voice: the owed call is hung up");
                    self.settle(&call_id);
                }
            }
            Cmd::Conversation { thread, answering } => {
                if let Some(s) = self.slot.as_mut() {
                    s.call.conversation(thread, answering, now);
                }
            }
            Cmd::Hold { held, fallback } => {
                self.held = (held, fallback.clone());
                if let Some(s) = self.slot.as_mut() {
                    s.call.hold(held, fallback);
                }
            }
            Cmd::Approvals { waiting } => {
                self.waiting = waiting.clone();
                if let Some(s) = self.slot.as_mut() {
                    s.call.approvals(waiting, now);
                }
            }
            Cmd::Verdicts {
                call,
                thread_id,
                verdicts,
            } => {
                let taken = match self.slot.as_mut() {
                    Some(s) if s.call.number() == call => {
                        s.call.verdicts(thread_id, verdicts.clone(), now)
                    }
                    _ => false,
                };
                if !taken {
                    let _ = self
                        .ctx
                        .link
                        .send(LinkCmd::CallVerdicts(CallVerdicts::Unsent {
                            thread_id,
                            verdicts,
                        }));
                }
            }
            Cmd::Shutdown => {}
            Cmd::Transport {
                call,
                state,
                detail,
            } => {
                if let Some(c) = self.peer_call(call, "transport") {
                    c.transport(state, &detail, now);
                }
            }
            Cmd::ChannelOpen { call } => {
                if let Some(c) = self.peer_call(call, "channel_open") {
                    c.channel_open(now);
                }
            }
            Cmd::ChannelMessage { call, text } => {
                if let Some(c) = self.peer_call(call, "channel_message") {
                    c.channel_message(text, now);
                }
            }
            Cmd::ChannelClosed { call, why } => {
                if let Some(c) = self.peer_call(call, "channel_closed") {
                    c.channel_closed(&why, now);
                }
            }
            Cmd::Failed { call, words } => {
                if let Some(c) = self.peer_call(call, "failed") {
                    c.failed(words, now);
                }
            }
            Cmd::Capturing { call } => {
                if let Some(c) = self.peer_call(call, "capturing") {
                    c.capturing(now);
                }
            }
            Cmd::Focus { call, focus } => {
                if let Some(c) = self.peer_call(call, "focus") {
                    c.focus(focus, now);
                }
            }
            Cmd::Stats {
                call,
                request,
                stats,
            } => {
                if let Some(c) = self.peer_call(call, "stats") {
                    c.stats(request, stats, now);
                }
            }
            Cmd::Dismissed { call } => {
                if let Some(c) = self.peer_call(call, "dismissed") {
                    c.dismissed(now);
                }
            }
            owner => {
                // The owner's controls act on the current call.
                let Some(s) = self.slot.as_mut() else {
                    return;
                };
                let c = &mut s.call;
                match owner {
                    Cmd::TakeOver => c.take_over(now),
                    Cmd::Press => c.press(now),
                    Cmd::Release => c.release(now),
                    Cmd::Tap => c.tap(now),
                    Cmd::Stop => c.stop_kai(now),
                    Cmd::Muted(m) => c.set_muted(m, now),
                    Cmd::PushToTalk(on) => c.set_push_to_talk(on, now),
                    Cmd::Headset(press) => c.headset(press, now),
                    Cmd::End => c.end(now),
                    _ => {}
                }
            }
        }
    }

    /// The current call when libwebrtc's event names it. An older call's events are dropped: its
    /// PeerConnection may still be closing when the next call starts (libwebrtc reports `closed`
    /// as `close()` begins), and its stats answer numbers of its own.
    fn peer_call(&mut self, n: u64, what: &str) -> Option<&mut Call> {
        match self.slot.as_mut() {
            Some(s) if s.call.number() == n => Some(&mut s.call),
            current => {
                tracing::debug!(
                    call = n,
                    current = current.map(|s| s.call.number()),
                    what,
                    "voice: an event of another call than the current one; dropped"
                );
                None
            }
        }
    }

    fn stagger(&self) -> Duration {
        Duration::from_millis(u64::from(self.ctx.settings.load().lmgw.url_stagger_ms))
    }

    fn facts(&self) -> Facts {
        let s = self.ctx.settings.load();
        Facts {
            mode: s.voice.mode,
            connect_timeout_s: s.voice.connect_timeout_s,
            idle_end_s: s.voice.idle_end_s,
            playout_report_ms: s.voice.playout_report_ms,
            ptt_tail_ms: s.voice.ptt_tail_ms,
            half_duplex: s.voice.echo == EchoMode::HalfDuplex,
            echo_tail_ms: s.voice.echo_tail_ms,
            timeout_ms: u64::from(s.lmgw.timeout_s) * 1000,
        }
    }

    fn start(
        &mut self,
        ticket: Ticket,
        start: CallStart,
        sink: Arc<dyn EventSink>,
        now: u64,
    ) -> Result<u64, CoreError> {
        if let Some(old) = self.slot.as_mut() {
            if !old.call.over() {
                return Err(CoreError::NotReady {
                    reason: "A voice call is running already.".to_owned(),
                });
            }
            if !old.call.done() {
                // The old PeerConnection is still closing (its stop may not have gone yet): it
                // goes now, and its call (owed since lmgw answered, never settled) is hung up
                // before the next bind, so the new call meets no binding of the phone's own.
                old.sink.close_peer();
            }
        }
        self.n += 1;
        let facts = self.facts();
        let held = (
            ticket.held || self.held.0,
            ticket.hold_fallback.clone().or(self.held.1.clone()),
        );
        let mut call = Call::new(
            self.n,
            start,
            facts,
            ticket.voice_by.clone(),
            ticket.thread.clone(),
            held,
            now,
        );
        call.approvals(self.waiting.clone(), now);
        tracing::info!(
            call = self.n,
            new = start.new,
            take_over = start.take_over,
            expect_pending = ticket.voice_by.is_some(),
            "voice: a call starts"
        );
        // The warm connection (§7.2): to the URL that last answered, or the race's winner.
        let (warm_tx, warm_rx) = watch::channel(None);
        {
            let t = ticket.clone();
            let stagger = self.stagger();
            let timeout = self.timeout();
            tokio::spawn(async move {
                let base = warm(&t, stagger, timeout).await;
                let _ = warm_tx.send(Some(base));
            });
        }
        let _ = self.ctx.link.send(LinkCmd::CallRunning(Some(self.n)));
        self.published = None;
        self.slot = Some(Slot {
            call,
            sink,
            ticket,
            offer: None,
            warm: warm_rx,
        });
        Ok(self.n)
    }

    fn offer(&mut self, call: u64, sdp: String, reply: oneshot::Sender<Result<String, CoreError>>) {
        let Some(s) = self.slot.as_mut() else {
            let _ = reply.send(Err(CoreError::NotReady {
                reason: "No voice call was started.".to_owned(),
            }));
            return;
        };
        if s.call.number() != call {
            tracing::debug!(
                call,
                current = s.call.number(),
                "voice: an offer of another call than the current one; refused"
            );
            let _ = reply.send(Err(CoreError::NotReady {
                reason: "This offer is for another call than the current one.".to_owned(),
            }));
            return;
        }
        if s.call.over() || s.offer.is_some() || s.call.call_id().is_some() {
            let _ = reply.send(Err(CoreError::NotReady {
                reason: "This call has ended, or its offer went already.".to_owned(),
            }));
            return;
        }
        s.offer = Some(reply);
        let place = Place {
            n: s.call.number(),
            ticket: s.ticket.clone(),
            start: s.call.start(),
            part: s.call.session_part(),
            sdp,
            owed: self.owed.clone(),
            warm: s.warm.clone(),
            stagger: self.stagger(),
            timeout: self.timeout(),
            cmds: self.ctx.cmds.clone(),
            link: self.ctx.link.clone(),
        };
        tokio::spawn(place.run());
    }

    fn posted(&mut self, n: u64, result: Result<CallAnswer, Failed>, now: u64) {
        let timeout = self.timeout();
        let current = self.slot.as_ref().is_some_and(|s| s.call.number() == n);
        if let Ok(a) = &result {
            // lmgw holds this call from here until its clean end or its hangup.
            self.owe(&a.call_id);
        }
        if !current {
            // An older call's answer: its call is hung up before the next POST.
            return;
        }
        let Some(s) = self.slot.as_mut() else {
            return;
        };
        let reply = s.offer.take();
        let answer = match result {
            Ok(a) => {
                s.call.answered(a.call_id.clone(), &a.sdp, now);
                if s.call.over() {
                    Err(CoreError::NotReady {
                        reason: "The call was ended while lmgw answered.".to_owned(),
                    })
                } else {
                    Ok(a.sdp)
                }
            }
            Err(f) => {
                let (end, err) = refusal(&f, timeout);
                s.call.not_answered(end, now);
                Err(err)
            }
        };
        if let Some(r) = reply {
            let _ = r.send(answer);
        }
    }

    /// The call's effects, then its view and captions when they changed.
    fn carry_out(&mut self) {
        let now = self.now();
        let Some(s) = self.slot.as_mut() else {
            return;
        };
        let n = s.call.number();
        for e in s.call.drain() {
            match e {
                Effect::Send(text) => s.sink.send(text),
                Effect::Mic(m) => s.sink.set_mic(m),
                Effect::Playback(a) => s.sink.set_playback(a),
                Effect::ReadStats(r) => s.sink.read_stats(r),
                Effect::CloseChannel => s.sink.close_channel(),
                Effect::ClosePeer => s.sink.close_peer(),
                Effect::Cue => s.sink.cue(),
                Effect::Ended(end) => {
                    s.sink.ended(end);
                    let _ = self.ctx.link.send(LinkCmd::CallRunning(None));
                }
                Effect::FollowThread(t) => {
                    let _ = self.ctx.link.send(LinkCmd::VoiceThread(t));
                }
                Effect::Revoked(kind, message) => {
                    let _ = self.ctx.link.send(LinkCmd::VoiceRevoked { kind, message });
                }
                Effect::ApprovalShown(request) => {
                    let _ = self.ctx.link.send(LinkCmd::CallApproval(Box::new(Shown {
                        call: n,
                        request: *request,
                    })));
                }
                Effect::ApprovalDecided(id) => {
                    let _ = self.ctx.link.send(LinkCmd::CallApprovalDecided { id });
                }
                Effect::Asked => s.sink.asked(),
                Effect::Verdicts(outcome) => {
                    let _ = self.ctx.link.send(LinkCmd::CallVerdicts(outcome));
                }
            }
        }
        // A clean end (lmgw's word, or the channel's close after End): lmgw let the call go.
        let clean = (s.call.done() && s.call.stale_id().is_none())
            .then(|| s.call.call_id().map(str::to_owned))
            .flatten();
        for line in s.call.take_captions() {
            let _ = self.ctx.updates.send(Update::Caption { line });
        }
        let view = s.call.view(now);
        if self.published.as_ref() != Some(&view) {
            self.published = Some(view.clone());
            let _ = self.ctx.updates.send(Update::Voice { view });
        }
        if let Some(id) = clean {
            self.settle(&id);
        }
    }
}

/// The warm-up (§7.2): the URL that last answered, else the race's winner. Bounded as a whole by
/// `lmgw.timeout_s` (its request's answer and body are each bounded by it too, so without this the
/// warm-up could take twice that), so the POST that waits for it waits at most that long.
async fn warm(t: &Ticket, stagger: Duration, timeout: Duration) -> Option<String> {
    let found = tokio::time::timeout(timeout, async {
        match &t.last_good {
            Some(base) => {
                if let Err(f) = signal::warm(&t.tls, base, timeout).await {
                    tracing::debug!(%base, why = %f.words(timeout), "voice: the warm-up failed");
                }
                Some(base.clone())
            }
            None => signal::race_base(&t.tls, t.urls.clone(), stagger, timeout)
                .await
                .map_err(|failed| tracing::info!(?failed, "voice: no URL answered the warm-up"))
                .ok(),
        }
    })
    .await;
    found.unwrap_or_else(|_| {
        tracing::info!(
            secs = timeout.as_secs(),
            "voice: the warm-up did not finish within lmgw.timeout_s"
        );
        t.last_good.clone()
    })
}

/// A failed POST's end and error (§7.3 step 3).
fn refusal(f: &Failed, timeout: Duration) -> (crate::voice::CallEnd, CoreError) {
    match f {
        Failed::NotReady(steps) => (
            ends::refused(steps.clone(), false),
            CoreError::NotReady {
                reason: steps.clone(),
            },
        ),
        Failed::Request(Fail::Refused { status, error }) => {
            let words = format!("lmgw refused the call: {} ({})", error.message, error.code);
            let reconnect = !matches!(*status, 401 | 403);
            (
                ends::refused(words, reconnect),
                CoreError::Refused {
                    code: error.code.clone(),
                    reason: error.message.clone(),
                    hint: (*status == 429 && error.code == "key_rate").then(|| {
                        "a voice call holds one of this device's concurrency slots for its life"
                            .to_owned()
                    }),
                },
            )
        }
        Failed::Request(other) => {
            let words = other.words(timeout);
            (
                ends::refused(words.clone(), true),
                CoreError::Unreachable {
                    reason: words,
                    setting: Some("lmgw.timeout_s".to_owned()),
                },
            )
        }
    }
}

/// One call's signalling, as a task (module doc).
struct Place {
    n: u64,
    ticket: Ticket,
    start: CallStart,
    part: Option<lmgw_client::realtime::BoundSession>,
    sdp: String,
    /// The calls lmgw may still hold, hung up first.
    owed: Vec<String>,
    warm: watch::Receiver<Option<Option<String>>>,
    stagger: Duration,
    timeout: Duration,
    cmds: mpsc::UnboundedSender<Cmd>,
    link: mpsc::UnboundedSender<LinkCmd>,
}

impl Place {
    async fn run(mut self) {
        let result = self.place().await;
        let _ = self.cmds.send(Cmd::Posted { n: self.n, result });
    }

    async fn base(&mut self) -> Result<String, Failed> {
        // The warm-up first, so the POST takes the connection it opened rather than a second
        // handshake. It started before this wait and is bounded by `lmgw.timeout_s` as a whole, so
        // this bound (the same setting) is never the one that ends the wait.
        let found = tokio::time::timeout(self.timeout, self.warm.wait_for(Option::is_some))
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|r| r.clone().flatten());
        if let Some(base) = found.or_else(|| self.ticket.last_good.clone()) {
            return Ok(base);
        }
        signal::race_base(
            &self.ticket.tls,
            self.ticket.urls.clone(),
            self.stagger,
            self.timeout,
        )
        .await
        .map_err(|failed| {
            Failed::Request(Fail::NoAnswer(crate::link::http::NoAnswer::Failed(
                crate::link::failure::Failure::Other(
                    failed
                        .iter()
                        .map(|(u, w)| format!("{u}: {w}"))
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
            )))
        })
    }

    fn refused(&self, status: u16, error: &ApiError) {
        if signal::key_refusal(status, error) {
            let _ = self.link.send(LinkCmd::VoiceRefused {
                identity: self.ticket.tls.identity(),
                status,
                error: error.clone(),
            });
        }
    }

    async fn place(&mut self) -> Result<CallAnswer, Failed> {
        let mut base = self.base().await?;
        let tls = self.ticket.tls.clone();
        // The owed calls first (§7.8): their bindings let go before this bind.
        for id in std::mem::take(&mut self.owed) {
            let done = match signal::hangup(&tls, &base, &id, self.timeout).await {
                Ok(how) => {
                    tracing::info!(call_id = %id, ?how, "voice: the owed call's hangup answered");
                    true
                }
                Err(Fail::Refused { status, error }) => {
                    self.refused(status, &error);
                    tracing::warn!(call_id = %id, code = %error.code, "voice: the owed call's hangup was refused");
                    false
                }
                Err(f) => {
                    tracing::info!(call_id = %id, why = %f.words(self.timeout), "voice: the owed call's hangup got no answer");
                    false
                }
            };
            let _ = self.cmds.send(Cmd::HungUp { call_id: id, done });
        }
        let mut folder = match self.ticket.folder {
            Some(f) => f,
            None => self.lookup(&base).await?,
        };
        let mut looked_again = false;
        let mut raced = false;
        loop {
            let req = calls::call(
                CallBind::Folder {
                    id: folder,
                    new: self.start.new,
                },
                Takeover::Ask,
                &self.sdp,
                self.part.as_ref(),
            );
            match signal::post(&tls, &base, &req, self.timeout).await {
                Ok(a) => return Ok(a),
                Err(Fail::Connect(f)) if !raced => {
                    // The network changed: the other URLs race once (§7.11).
                    raced = true;
                    tracing::info!(%base, why = %f.words(), "voice: the POST could not connect; racing the URLs");
                    let others: Vec<String> = self
                        .ticket
                        .urls
                        .iter()
                        .filter(|u| **u != base)
                        .cloned()
                        .collect();
                    if others.is_empty() {
                        return Err(Failed::Request(Fail::Connect(f)));
                    }
                    match signal::race_base(&tls, others, self.stagger, self.timeout).await {
                        Ok(b) => base = b,
                        Err(_) => return Err(Failed::Request(Fail::Connect(f))),
                    }
                }
                Err(Fail::Refused { status: 404, error })
                    if error.code == "chat_folder_not_found" && !looked_again =>
                {
                    // The folder is gone or out of reach: the lookup again, which may create it.
                    looked_again = true;
                    tracing::info!(
                        folder,
                        "voice: lmgw does not know the Kai folder; looking it up again"
                    );
                    folder = self.lookup(&base).await?;
                    let _ = self.link.send(LinkCmd::Reload);
                }
                Err(Fail::Refused { status, error }) => {
                    self.refused(status, &error);
                    return Err(Failed::Request(Fail::Refused { status, error }));
                }
                Err(f) => return Err(Failed::Request(f)),
            }
        }
    }

    /// The Kai folder by §6.1's rule.
    async fn lookup(&self, base: &str) -> Result<i64, Failed> {
        let h = Handle {
            tls: self.ticket.tls.clone(),
            base: base.to_owned(),
        };
        match load::load(&h, &self.ticket.folder_name, None, false, self.timeout).await {
            Loaded::Ready { folder, .. } => Ok(folder.id),
            Loaded::NotReady { steps, .. } => Err(Failed::NotReady(steps)),
            Loaded::Failed(load::Fail::Refused { status, error }) => {
                self.refused(status, &error);
                Err(Failed::Request(Fail::Refused { status, error }))
            }
            Loaded::Failed(load::Fail::NoAnswer(n)) => Err(Failed::Request(Fail::NoAnswer(n))),
        }
    }
}
