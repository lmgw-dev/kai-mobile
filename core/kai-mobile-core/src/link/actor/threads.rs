//! The actor's side of the threads browser and the opened thread (design §6.4): the commands,
//! the reads it spawns and what it publishes. The rules are `crate::threads`'; this is the wiring.

use lmgw_client::{
    feed::FeedEvent,
    requests::{ThreadDetail, ThreadRow},
};

use super::{Actor, Cmd, Phase};
use crate::{
    CoreError,
    conversation::{
        Streaming,
        load::{Fail, load_thread},
        send::{Frame, StreamFail},
    },
    link::{http::Handle, words::Said},
    threads::{
        Opened,
        browser::{Browser, Lists},
        io,
        view::OpenedStatus,
    },
    views::Update,
};

impl Actor {
    fn up(&self) -> bool {
        matches!(self.phase, Phase::Open { up: true, .. })
    }

    /// The link for a browser request, or why there is none now (the state's words, at once).
    pub(super) fn linked(&self) -> Result<(Handle, Option<i64>), CoreError> {
        let Some(link) = self.link.as_ref() else {
            return Err(CoreError::NotConnected {
                reason: Said::not_paired().words,
            });
        };
        if !self.up() {
            return Err(CoreError::NotConnected {
                reason: self.said.words.clone(),
            });
        }
        Ok((link.handle(), self.convo.thread_id))
    }

    // ------------------------------------------------------------------ the browser

    pub(super) fn browser_show(&mut self, shown: bool) {
        if self.browser.show(shown) {
            self.read_browser();
        }
        self.publish_browser();
    }

    pub(super) fn browser_archived(&mut self, shown: bool) {
        if self.browser.show_archived(shown) {
            self.read_browser();
        }
        self.publish_browser();
    }

    /// Reads the lists while the browser shows (coalesced: one read in flight).
    pub(super) fn read_browser(&mut self) {
        if !self.browser.shown {
            return;
        }
        if !self.visible {
            // Not on screen (the back stack still shows the browser): read when the app is
            // visible again (`threads_reread`).
            self.browser.defer();
            return;
        }
        let Some(link) = self.link.as_ref().filter(|_| self.up()) else {
            let words = self.said.words.clone();
            self.browser.waiting(&words);
            self.publish_browser();
            return;
        };
        let Some(generation) = self.browser.want() else {
            return;
        };
        let h = link.handle();
        let archived = self.browser.archived_shown;
        let timeout = self.timeout();
        let tx = self.ctx.cmds.clone();
        tokio::spawn(async move {
            let result = io::lists(&h, archived, timeout).await;
            let _ = tx.send(Cmd::BrowserRead {
                generation,
                identity: h.identity(),
                result: Box::new(result),
            });
        });
    }

    pub(super) fn browser_read(
        &mut self,
        generation: u64,
        identity: [u8; 32],
        result: Result<Lists, Fail>,
    ) {
        if Some(identity) != self.identity() {
            return;
        }
        if let Err(Fail::Refused { status, error }) = &result
            && self.request_refused(identity, *status, error)
        {
            self.browser.abandon(generation);
            return;
        }
        let timeout = self.timeout();
        let result = result.map_err(|f| {
            format!(
                "Kai could not read the threads on lmgw: {}",
                f.words(timeout)
            )
        });
        let (changed, again) = self.browser.read(generation, result);
        if changed {
            self.publish_browser();
        }
        if again {
            self.read_browser();
        }
    }

    pub(super) fn publish_browser(&self) {
        if self.browser.shown {
            self.emit(Update::Browser {
                view: self.browser.view(
                    self.convo.folder.as_ref().map(|f| f.id),
                    self.convo.thread_id,
                ),
            });
        }
    }

    // ------------------------------------------------------------------ the opened thread

    /// Opens thread `id` (`None`: closes the opened one). Opening the one already open reads it
    /// again and keeps what it shows.
    pub(super) fn open(&mut self, id: Option<i64>) {
        match id {
            None => {
                if self.opened.take().is_some() {
                    self.emit(Update::Opened { view: None });
                }
            }
            Some(id) if self.opened.as_ref().is_some_and(|o| o.id == id) => self.read_opened(),
            Some(id) => {
                self.open_seq += 1;
                tracing::info!(thread = id, "threads: opened");
                self.opened = Some(Opened::new(self.open_seq, id, &self.convo));
                // The Kai thread is the conversation's, never a second copy of it.
                let kai = self.settle_opened_kai();
                self.emit(Update::OpenedMessages {
                    delta: crate::views::MessagesDelta {
                        reset: true,
                        thread_id: Some(id),
                        upsert: Vec::new(),
                        removed: Vec::new(),
                    },
                });
                self.publish_opened();
                if self.visible {
                    let ups = self.replies.opened(id);
                    self.emit_all(ups);
                }
                if !kai {
                    self.read_opened();
                }
            }
        }
    }

    /// The Kai conversation's thread became known or changed: an opening of it is marked and
    /// published.
    pub(super) fn kai_known(&mut self) {
        if self.settle_opened_kai() {
            self.publish_opened();
        }
    }

    /// The Kai conversation's thread is known: when it is the opened one, the opening is marked
    /// (`OpenedView::kai`) and the app leaves for the conversation. Whether it was.
    pub(super) fn settle_opened_kai(&mut self) -> bool {
        let kai = self.convo.thread_id;
        self.opened.as_mut().is_some_and(|o| o.is_kai(kai))
    }

    pub(super) fn read_opened(&mut self) {
        let Some(link) = self.link.as_ref() else {
            return;
        };
        let Some(o) = self.opened.as_mut() else {
            return;
        };
        if !o.want() {
            return;
        }
        let (seq, id, h) = (o.seq, o.id, link.handle());
        let timeout = self.timeout();
        let tx = self.ctx.cmds.clone();
        tokio::spawn(async move {
            let result = load_thread(&h, id, timeout).await;
            let _ = tx.send(Cmd::OpenedRead {
                seq,
                identity: h.identity(),
                result: Box::new(result),
            });
        });
    }

    pub(super) fn opened_read(
        &mut self,
        seq: u64,
        identity: [u8; 32],
        result: Result<ThreadDetail, Fail>,
    ) {
        if Some(identity) != self.identity() || self.opened.as_ref().map(|o| o.seq) != Some(seq) {
            return;
        }
        let again = self.opened.as_mut().is_some_and(Opened::read_ended);
        if let Err(Fail::Refused { status, error }) = &result
            && self.request_refused(identity, *status, error)
        {
            return;
        }
        let timeout = self.timeout();
        let Some(o) = self.opened.as_mut() else {
            return;
        };
        match result {
            Ok(detail) => {
                let delta = o.loaded(&detail);
                self.emit(Update::OpenedMessages { delta });
            }
            Err(f) => {
                let gone = matches!(&f, Fail::Refused { status: 404, .. });
                o.failed(gone, f.words(timeout));
            }
        }
        self.publish_opened();
        if again {
            self.read_opened();
        }
    }

    pub(super) fn publish_opened(&self) {
        if let Some(o) = self.opened.as_ref() {
            self.emit(Update::Opened {
                view: Some(o.view(self.convo.thread_id)),
            });
        }
    }

    /// A send into the opened thread starts: the link and the opening's number, or why not.
    pub(super) fn thread_send_start(&mut self, thread: i64) -> Result<(Handle, u64), CoreError> {
        let (h, _) = self.linked()?;
        let Some(o) = self.opened.as_mut().filter(|o| o.id == thread) else {
            return Err(CoreError::NotReady {
                reason: "this thread is not open any more".to_owned(),
            });
        };
        if o.kai() {
            return Err(CoreError::NotReady {
                reason: "this is the Kai conversation's thread: send there".to_owned(),
            });
        }
        if o.status() != OpenedStatus::Ready {
            return Err(CoreError::NotReady {
                reason: "the thread has not been read yet".to_owned(),
            });
        }
        if o.convo.streaming.is_some() {
            return Err(CoreError::NotReady {
                reason: "a send is still streaming".to_owned(),
            });
        }
        o.convo.unsaved = None;
        o.convo.streaming = Some(Streaming {
            thread_id: thread,
            ..Default::default()
        });
        let seq = o.seq;
        self.publish_opened();
        Ok((h, seq))
    }

    pub(super) fn thread_frame(&mut self, seq: u64, frame: Frame) {
        let Some(o) = self.opened.as_mut().filter(|o| o.seq == seq) else {
            return;
        };
        if let Some((delta, done)) = o.convo.frame(frame) {
            self.emit(Update::OpenedMessages { delta });
            if done {
                self.publish_opened();
            }
        }
    }

    pub(super) fn thread_send_ended(
        &mut self,
        seq: u64,
        identity: [u8; 32],
        result: Result<(), StreamFail>,
    ) {
        if let Err(StreamFail::Refused { status, error }) = &result {
            self.request_refused(identity, *status, error);
        }
        let timeout = self.timeout();
        let Some(o) = self.opened.as_mut().filter(|o| o.seq == seq) else {
            return;
        };
        if let Some(delta) = o.convo.stream_ended(super::ended_words(&result, timeout)) {
            self.emit(Update::OpenedMessages { delta });
        }
        self.publish_opened();
    }

    /// An action answered: the thread as it is now, `None` once deleted.
    pub(super) fn acted(&mut self, identity: [u8; 32], thread: i64, row: Option<ThreadRow>) {
        if Some(identity) != self.identity() {
            return;
        }
        let mut again = false;
        if let Some(o) = self.opened.as_mut().filter(|o| o.id == thread) {
            match row {
                Some(r) => again = o.changed(r),
                None => o.deleted(),
            }
            self.publish_opened();
        }
        if again {
            self.read_opened();
        }
        self.read_browser();
    }

    // ------------------------------------------------------------------ the feed and the link

    /// A feed event for the opened thread and the browser (the Kai conversation had it first).
    pub(super) fn threads_event(&mut self, ev: &FeedEvent) {
        if Browser::touches(ev) {
            self.read_browser();
        }
        let Some(o) = self.opened.as_mut() else {
            return;
        };
        if let FeedEvent::State(s) = ev {
            o.convo.live(&s.voice, &s.turns);
            self.publish_opened();
            return;
        }
        if matches!(ev, FeedEvent::Resync(_)) {
            self.read_opened();
            return;
        }
        let asks = o.event(ev);
        if let Some(delta) = asks.delta {
            o.delta_applied();
            self.emit(Update::OpenedMessages { delta });
        }
        if asks.read {
            self.read_opened();
        }
        if asks.view {
            self.publish_opened();
        }
    }

    /// A `hello`, an event that did not read, the app made visible: everything shown is read
    /// again.
    pub(super) fn threads_reread(&mut self) {
        if let Some(o) = self.opened.as_mut() {
            o.convo.seed_live(&self.convo);
        }
        self.read_opened();
        self.read_browser();
    }

    /// The connection ended: what lmgw told live is unknown until the next `hello`.
    pub(super) fn threads_disconnected(&mut self) {
        if let Some(o) = self.opened.as_mut() {
            o.convo.disconnected();
        }
    }

    /// Another pairing, or none: nothing of the old gateway's threads stays.
    pub(super) fn threads_reset(&mut self) {
        self.open(None);
        let shown = self.browser.shown;
        self.browser = Browser::default();
        if shown {
            self.browser.show(true);
            self.read_browser();
        }
    }
}
