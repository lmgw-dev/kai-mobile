//! The threads browser and the opened thread (design §6.4): any thread the device can see, beside
//! the Kai conversation and never instead of it. [`browser::Browser`] holds the browser's lists,
//! [`Opened`] the one opened thread (sans-IO, kept by the link's actor), [`io`] the requests: the
//! lists, the search, a create in a folder and the actions on a thread.
//!
//! **The opened thread** wraps a second [`Convo`] without a folder, so its messages, the phone's
//! send streaming into it, the label notices, who else answers and who holds its voice follow the
//! Kai thread's rules (§6.2, §6.3). It is read whole when it opens, at each `hello`, `resync` or
//! event that does not read, when the app becomes visible, and when lmgw says it went (a 404 then
//! shows it gone); between reads the feed keeps it current. Each opening has a number of its own
//! ([`Opened::seq`]): a send's frames and a read's answer carry it, so nothing of a thread the
//! owner left lands in the next one.

pub mod browser;
pub mod io;
pub mod view;

#[cfg(test)]
mod tests;

use lmgw_client::{
    feed::{FeedEvent, ThreadNow},
    requests::{ThreadDetail, ThreadRow},
};

use self::view::{OpenedStatus, OpenedView};
use crate::{
    conversation::{Asks, Convo},
    views::MessagesDelta,
};

/// The opened thread (module doc).
#[derive(Debug)]
pub struct Opened {
    /// This opening's number.
    pub seq: u64,
    pub id: i64,
    pub convo: Convo,
    /// The thread's row as last read or changed.
    row: Option<ThreadRow>,
    status: OpenedStatus,
    note: Option<String>,
    /// This is the Kai conversation's thread: the conversation shows it, so this opening reads
    /// nothing and sends nothing (the app leaves for the conversation).
    kai: bool,
    /// A read is in flight; one more follows it when `dirty` (reads are not stacked, and the
    /// answer is never older than a delta already applied).
    reading: bool,
    dirty: bool,
}

/// What a feed event asks of the opened thread.
#[derive(Debug, Default, PartialEq)]
pub struct OpenedAsks {
    /// Read it again (deleted, archived, out of reach: the read says which).
    pub read: bool,
    /// Its view changed.
    pub view: bool,
    pub delta: Option<MessagesDelta>,
}

impl Opened {
    /// Thread `id` opened as opening `seq`, starting from what `kai` knows live.
    pub fn new(seq: u64, id: i64, kai: &Convo) -> Self {
        let mut convo = Convo::default();
        convo.thread_id = Some(id);
        convo.seed_live(kai);
        Self {
            seq,
            id,
            convo,
            row: None,
            status: OpenedStatus::Loading,
            note: None,
            kai: false,
            reading: false,
            dirty: false,
        }
    }

    /// The Kai conversation's thread is `kai_thread`: whether this opening just turned out to be
    /// it.
    pub fn is_kai(&mut self, kai_thread: Option<i64>) -> bool {
        if self.kai || kai_thread != Some(self.id) {
            return false;
        }
        self.kai = true;
        true
    }

    /// Whether this opening is the Kai conversation's thread (it reads and sends nothing).
    pub fn kai(&self) -> bool {
        self.kai
    }

    /// A read is wanted: whether to start it now (`false`: one is in flight, and one more follows
    /// it).
    pub fn want(&mut self) -> bool {
        if self.kai {
            return false;
        }
        if self.reading {
            self.dirty = true;
            return false;
        }
        self.reading = true;
        self.dirty = false;
        true
    }

    /// A delta was applied from the feed: a read in flight started before it may not hold it, so
    /// another follows.
    pub fn delta_applied(&mut self) {
        if self.reading {
            self.dirty = true;
        }
    }

    /// The read ended: whether another is owed.
    pub fn read_ended(&mut self) -> bool {
        self.reading = false;
        std::mem::take(&mut self.dirty)
    }

    pub fn status(&self) -> OpenedStatus {
        self.status
    }

    /// The view, `kai_thread` the Kai conversation's thread.
    pub fn view(&self, kai_thread: Option<i64>) -> OpenedView {
        let live = self.convo.view("");
        let row = self.row.as_ref();
        OpenedView {
            status: self.status,
            thread_id: self.id,
            title: row.map(|r| r.title.clone()).unwrap_or_default(),
            folder_id: row.and_then(|r| r.folder_id),
            pinned: row.is_some_and(|r| r.pinned),
            archived: row.is_some_and(|r| r.archived_at.is_some()),
            temporary: row.is_some_and(|r| r.temporary) || self.id < 0,
            kai: self.kai || kai_thread == Some(self.id),
            note: self.note.clone(),
            answering: live.answering,
            voice_by: live.voice_by,
            sending: live.sending,
        }
    }

    /// The thread came whole: the delta that shows it.
    pub fn loaded(&mut self, detail: &ThreadDetail) -> MessagesDelta {
        self.row = Some(detail.thread.row.clone());
        self.status = OpenedStatus::Ready;
        self.note = None;
        // The Kai conversation's thread test is by id; a detail of another id is lmgw's answer for
        // this one all the same.
        let mut delta = self.convo.thread_loaded(detail);
        self.convo.thread_id = Some(self.id);
        delta.thread_id = Some(self.id);
        delta
    }

    /// The read failed: `gone` when lmgw answered 404 (deleted, or out of this device's reach).
    pub fn failed(&mut self, gone: bool, why: String) {
        if gone {
            self.status = OpenedStatus::Gone;
            self.note = Some(format!(
                "This thread is gone: it was deleted, or is no longer within this device's reach \
                 on lmgw ({why})."
            ));
        } else {
            if self.status != OpenedStatus::Ready {
                self.status = OpenedStatus::Failed;
            }
            self.note = Some(format!("Kai could not read this thread on lmgw: {why}"));
        }
    }

    /// An action answered the thread as it is now (pin, archive, move, rename). Whether it is read
    /// again: an archive or a restore is (lmgw says what the device may still see), as the feed's
    /// `thread.updated` of one is.
    pub fn changed(&mut self, row: ThreadRow) -> bool {
        if row.id != self.id {
            return false;
        }
        let was_archived = self.row.as_ref().is_some_and(|t| t.archived_at.is_some());
        if let Some(old) = self.row.as_ref()
            && old.title != row.title
        {
            self.convo.thread_title = Some(row.title.clone());
        }
        let moved = was_archived != row.archived_at.is_some();
        self.row = Some(row);
        moved
    }

    /// The phone deleted it.
    pub fn deleted(&mut self) {
        self.status = OpenedStatus::Gone;
        self.note = Some("This thread was deleted.".to_owned());
    }

    /// What `ev` asks of the opened thread (§5.4's table, as for the Kai thread).
    pub fn event(&mut self, ev: &FeedEvent) -> OpenedAsks {
        let mut out = OpenedAsks::default();
        if let FeedEvent::ThreadCreated(ThreadNow::Row(r))
        | FeedEvent::ThreadUpdated(ThreadNow::Row(r)) = ev
            && r.thread.id == self.id
        {
            let was_archived = self.row.as_ref().is_some_and(|t| t.archived_at.is_some());
            out.view = self.row.as_ref() != Some(&r.thread);
            self.row = Some(r.thread.clone());
            let Asks { delta, view, .. } = self.convo.event(ev);
            out.delta = delta;
            out.view |= view;
            // An archive (or a restore) is read again: lmgw says what the device may still see.
            out.read = was_archived != r.thread.archived_at.is_some();
            return out;
        }
        let Asks {
            reload,
            delta,
            view,
            ..
        } = self.convo.event(ev);
        out.read = reload;
        out.delta = delta;
        out.view = view;
        out
    }
}
