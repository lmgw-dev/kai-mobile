//! The Kai conversation (design §6), the desktop's: the folder found by kai §4's rule, its current
//! thread, the thread's messages kept current by the feed's `message.*` events, and the phone's own
//! text sends streaming into it. [`Convo`] is the state the link's actor keeps (sans-IO);
//! [`load`] and [`send`] are its requests.
//!
//! **When it asks** (§6.1): `current` at each connection's `hello`, whenever the app becomes
//! visible (a probe of the held feed's path too), after a `resync` or an event that does not read,
//! on `folder.current` of its folder or a deleted current thread, and after a lookup that failed
//! (or whose thread list did not read) on its own backoff while connected: the reply
//! notifications wait for the Kai folder's threads (§12.3). New conversation asks `current` with
//! `{new: true}`.

pub mod load;
pub mod notice;
pub mod send;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use lmgw_client::{
    feed::{FeedEvent, FolderNow, ThreadNow},
    requests::{Folder, Message, ThreadDetail},
    types::chat_feed::{LiveTurn, LiveVoice},
};

use self::send::Frame;
use crate::{
    markdown,
    views::{ConversationStatus, ConversationView, MessageView, MessagesDelta, Role},
};

/// The id of the reply streaming to the phone's own send, until its `done` names the real one.
pub const STREAMING_ID: i64 = -1;

/// A reply streaming to the phone's own send.
#[derive(Debug, Clone, Default)]
pub struct Streaming {
    pub thread_id: i64,
    pub text: String,
    pub tools: Vec<String>,
    pub error: Option<String>,
    /// Label notices (see [`notice`]).
    pub notices: Vec<String>,
}

/// What a turn said besides its text, kept on the saved reply (`done` does not wipe it).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Notes {
    pub error: Option<String>,
    pub notices: Vec<String>,
}

impl Notes {
    pub fn of(s: &Streaming) -> Self {
        Self {
            error: s.error.clone(),
            notices: s.notices.clone(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.error.is_none() && self.notices.is_empty()
    }
}

/// What a feed event asks of the conversation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asks {
    /// Look the folder up again and ask `current` (§6.1).
    pub reload: bool,
    /// Read this thread again (the folder's current thread moved).
    pub thread: Option<i64>,
    /// The view changed.
    pub view: bool,
    /// The messages changed: this delta.
    pub delta: Option<MessagesDelta>,
}

/// The conversation's state (module doc).
#[derive(Debug, Default)]
pub struct Convo {
    pub status: Option<ConversationStatus>,
    pub folder: Option<Folder>,
    pub steps: Option<String>,
    pub note: Option<String>,
    pub thread_id: Option<i64>,
    pub thread_title: Option<String>,
    pub messages: BTreeMap<i64, Message>,
    /// The threads in the Kai folder (the reply notifications' scope, §12.3).
    pub kai_threads: HashSet<i64>,
    /// Whether [`Self::kai_threads`] was read on this connection (a failed read leaves it unknown,
    /// and the replies keep collecting until a lookup reads it).
    pub threads_known: bool,
    /// Who else answers in which thread now.
    answering: HashMap<i64, BTreeSet<String>>,
    /// Who holds which thread's voice.
    voice: HashMap<i64, String>,
    /// The `by` of the phone's own changes (`hello.principal`).
    pub own_by: Option<String>,
    pub streaming: Option<Streaming>,
    /// A turn that ended without a saved reply (an error, a stop): its view stays until the next
    /// send, also over a reload of its thread.
    pub unsaved: Option<Streaming>,
    /// The notes of the phone's own saved replies, by message id.
    pub notes: HashMap<i64, Notes>,
    /// Counts the lookups started; a result of an older one is dropped.
    pub load_gen: u64,
}

impl Convo {
    /// The view (§6), `name` the folder's name setting.
    pub fn view(&self, name: &str) -> ConversationView {
        let thread = self.thread_id;
        ConversationView {
            status: self.status.unwrap_or(ConversationStatus::Loading),
            folder_name: name.to_owned(),
            folder_id: self.folder.as_ref().map(|f| f.id),
            steps: self.steps.clone(),
            note: self.note.clone(),
            thread_id: thread,
            thread_title: self.thread_title.clone(),
            answering: thread
                .and_then(|t| self.answering.get(&t))
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default(),
            voice_by: thread.and_then(|t| self.voice.get(&t)).cloned(),
            sending: self.streaming.is_some(),
        }
    }

    /// A frame of the phone's own send: the delta it changes, and whether it was the `done`. A
    /// label's notice and a turn's error stay on the reply: on the saved one past `done`, and
    /// on the unsaved view of a turn that ended without one.
    pub fn frame(&mut self, frame: Frame) -> Option<(MessagesDelta, bool)> {
        let s = self.streaming.as_mut()?;
        let thread = s.thread_id;
        match frame {
            Frame::Started { .. } => return None,
            Frame::Delta(t) => s.text.push_str(&t),
            Frame::Tool(name) => s.tools.push(name),
            Frame::Error(e) => match notice::said(&e) {
                notice::Said::Notice(t) => s.notices.push(t),
                notice::Said::Error(t) => s.error = Some(t),
            },
            Frame::Done {
                message_id,
                answered_by,
                aborted,
            } => {
                let s = self.streaming.take().unwrap_or_default();
                let mut upsert = Vec::new();
                let here = Some(thread) == self.thread_id;
                if let Some(id) = message_id {
                    let notes = Notes::of(&s);
                    if !notes.is_empty() {
                        self.notes.insert(id, notes);
                    }
                    if here {
                        if let Some(m) = self.messages.get(&id) {
                            // The feed's copy came first: show it with the notes.
                            if let Some(n) = self.notes.get(&id) {
                                upsert.push(view_with(m, Some(n)));
                            }
                        } else {
                            let mut v = streaming_view(&s);
                            v.id = id;
                            v.streaming = false;
                            v.answered_by = answered_by;
                            upsert.push(v);
                        }
                    }
                } else if s.error.is_some() || aborted || !s.notices.is_empty() {
                    // An unsaved turn keeps its error on screen until the next send.
                    let mut s = s;
                    if aborted && s.error.is_none() {
                        s.error = Some("the turn was stopped".to_owned());
                    }
                    let mut v = streaming_view(&s);
                    v.streaming = false;
                    upsert.push(v);
                    self.unsaved = Some(s);
                }
                let removed = if upsert.iter().any(|v| v.id == STREAMING_ID) {
                    Vec::new()
                } else {
                    vec![STREAMING_ID]
                };
                return Some((
                    MessagesDelta {
                        reset: false,
                        thread_id: Some(thread),
                        upsert,
                        removed,
                    },
                    true,
                ));
            }
        }
        (Some(thread) == self.thread_id).then(|| {
            (
                MessagesDelta {
                    reset: false,
                    thread_id: Some(thread),
                    upsert: vec![streaming_view(s)],
                    removed: Vec::new(),
                },
                false,
            )
        })
    }

    /// A connection ended: what lmgw told live (turns, voice) is unknown until the next `hello`.
    pub fn disconnected(&mut self) {
        self.answering.clear();
        self.voice.clear();
        self.threads_known = false;
    }

    /// Forget everything (a new pairing, Forget gateway).
    pub fn reset(&mut self) {
        *self = Self {
            load_gen: self.load_gen + 1,
            ..Default::default()
        };
    }

    /// Whether `thread` is in the Kai folder, as far as the phone knows.
    pub fn is_kai_thread(&self, thread: i64) -> bool {
        self.thread_id == Some(thread) || self.kai_threads.contains(&thread)
    }

    /// New conversation was chosen (§6.1): the screen shows an empty conversation at once, before
    /// lmgw has named the new thread. The old thread's messages, its unsaved turn and its live
    /// facts go; with no thread, every later frame or `message.*` event of the old one fails the
    /// thread test and is dropped, until `current` (or `folder.current`) names the new thread.
    pub fn cleared(&mut self) -> MessagesDelta {
        self.thread_id = None;
        self.thread_title = None;
        self.messages.clear();
        self.unsaved = None;
        self.answering.clear();
        MessagesDelta {
            reset: true,
            thread_id: None,
            upsert: Vec::new(),
            removed: Vec::new(),
        }
    }

    /// The thread came whole: the delta that shows it.
    pub fn thread_loaded(&mut self, detail: &ThreadDetail) -> MessagesDelta {
        let id = detail.thread.row.id;
        let moved = self.thread_id != Some(id);
        self.thread_id = Some(id);
        self.thread_title = Some(detail.thread.row.title.clone());
        self.kai_threads.insert(id);
        self.messages = detail.messages.iter().map(|m| (m.id, m.clone())).collect();
        let notes = &self.notes;
        if moved {
            self.answering.retain(|t, _| *t == id);
        }
        let mut upsert: Vec<MessageView> = self
            .messages
            .values()
            .map(|m| view_with(m, notes.get(&m.id)))
            .collect();
        if let Some(s) = self
            .streaming
            .as_ref()
            .or(self.unsaved.as_ref())
            .filter(|s| s.thread_id == id)
        {
            upsert.push(streaming_view(s));
        }
        MessagesDelta {
            reset: true,
            thread_id: Some(id),
            upsert,
            removed: Vec::new(),
        }
    }

    /// The live facts of a `hello` or a `state`.
    pub fn live(&mut self, voice: &[LiveVoice], turns: &[LiveTurn]) {
        self.voice = voice
            .iter()
            .filter(|v| Some(&v.by) != self.own_by.as_ref())
            .map(|v| (v.thread_id, v.by.clone()))
            .collect();
        self.answering.clear();
        for t in turns {
            if Some(&t.by) != self.own_by.as_ref() {
                self.answering
                    .entry(t.thread_id)
                    .or_default()
                    .insert(t.by.clone());
            }
        }
    }

    /// What `ev` asks of the conversation (§5.4's table).
    pub fn event(&mut self, ev: &FeedEvent) -> Asks {
        let mut asks = Asks::default();
        let folder_id = self.folder.as_ref().map(|f| f.id);
        match ev {
            FeedEvent::FolderCurrent(c) if Some(c.folder_id) == folder_id => {
                if let Some(t) = c.thread_id {
                    self.kai_threads.insert(t);
                    if Some(t) != self.thread_id {
                        asks.thread = Some(t);
                    }
                } else {
                    asks.reload = true;
                }
            }
            FeedEvent::FolderCreated(FolderNow::Row(r))
            | FeedEvent::FolderUpdated(FolderNow::Row(r))
                if Some(r.folder.id) == folder_id =>
            {
                let was_ready = self.status == Some(ConversationStatus::Ready);
                self.folder = Some(r.folder.clone());
                if !was_ready {
                    // Waiting for a model or the ongoing mark (§6.1): look again.
                    asks.reload = true;
                }
            }
            FeedEvent::FolderDeleted(g) if Some(g.folder_id) == folder_id => asks.reload = true,
            FeedEvent::FolderCreated(FolderNow::Gone(g))
            | FeedEvent::FolderUpdated(FolderNow::Gone(g))
                if Some(g.folder_id) == folder_id =>
            {
                asks.reload = true
            }
            FeedEvent::ThreadCreated(ThreadNow::Row(r))
            | FeedEvent::ThreadUpdated(ThreadNow::Row(r)) => {
                let t = &r.thread;
                if t.folder_id.is_some() && t.folder_id == folder_id {
                    self.kai_threads.insert(t.id);
                } else {
                    self.kai_threads.remove(&t.id);
                }
                if Some(t.id) == self.thread_id {
                    if t.archived_at.is_some() {
                        asks.reload = true;
                    } else if self.thread_title.as_deref() != Some(t.title.as_str()) {
                        self.thread_title = Some(t.title.clone());
                        asks.view = true;
                    }
                }
            }
            FeedEvent::ThreadDeleted(g) => {
                self.kai_threads.remove(&g.thread_id);
                if Some(g.thread_id) == self.thread_id {
                    asks.reload = true;
                }
            }
            FeedEvent::ThreadCreated(ThreadNow::Gone(g))
            | FeedEvent::ThreadUpdated(ThreadNow::Gone(g)) => {
                self.kai_threads.remove(&g.thread_id);
                if Some(g.thread_id) == self.thread_id {
                    asks.reload = true;
                }
            }
            FeedEvent::MessageAdded(m) | FeedEvent::MessageUpdated(m)
                if Some(m.thread_id) == self.thread_id =>
            {
                let upsert: Vec<MessageView> = m
                    .messages
                    .iter()
                    .map(|msg| {
                        self.messages.insert(msg.id, msg.clone());
                        view_with(msg, self.notes.get(&msg.id))
                    })
                    .collect();
                if !upsert.is_empty() {
                    asks.delta = Some(MessagesDelta {
                        reset: false,
                        thread_id: self.thread_id,
                        upsert,
                        removed: Vec::new(),
                    });
                }
            }
            FeedEvent::MessageDeleted(m) if Some(m.thread_id) == self.thread_id => {
                for id in &m.message_ids {
                    self.messages.remove(id);
                }
                asks.delta = Some(MessagesDelta {
                    reset: false,
                    thread_id: self.thread_id,
                    upsert: Vec::new(),
                    removed: m.message_ids.clone(),
                });
            }
            FeedEvent::TurnStarted(t) if Some(&t.by) != self.own_by.as_ref() => {
                asks.view = self
                    .answering
                    .entry(t.thread_id)
                    .or_default()
                    .insert(t.by.clone())
                    && Some(t.thread_id) == self.thread_id;
            }
            FeedEvent::TurnDone(t) => {
                let had = self.answering.remove(&t.thread_id).is_some();
                asks.view = had && Some(t.thread_id) == self.thread_id;
            }
            FeedEvent::VoiceBound(v) if Some(&v.by) != self.own_by.as_ref() => {
                self.voice.insert(v.thread_id, v.by.clone());
                asks.view = Some(v.thread_id) == self.thread_id;
            }
            FeedEvent::VoiceEnded(v) => {
                let had = self.voice.remove(&v.thread_id).is_some();
                asks.view = had && Some(v.thread_id) == self.thread_id;
            }
            _ => {}
        }
        asks
    }
}

/// A message as the conversation shows it, with the notes its turn left (if it was the phone's).
pub fn view_with(m: &Message, notes: Option<&Notes>) -> MessageView {
    let mut v = view(m);
    if let Some(n) = notes {
        v.error = n.error.clone();
        v.notices = n.notices.clone();
    }
    v
}

/// A message as lmgw has it.
pub fn view(m: &Message) -> MessageView {
    MessageView {
        id: m.id,
        role: match m.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::Other,
        },
        text: m.content.clone(),
        blocks: markdown::parse(&m.content),
        created_at: m.created_at.clone(),
        answered_by: m.answered_by.clone(),
        voice: m.voice.is_some(),
        streaming: false,
        tools: Vec::new(),
        error: None,
        notices: Vec::new(),
    }
}

/// The reply streaming to the phone's own send.
pub fn streaming_view(s: &Streaming) -> MessageView {
    MessageView {
        id: STREAMING_ID,
        role: Role::Assistant,
        text: s.text.clone(),
        blocks: markdown::parse(&s.text),
        created_at: String::new(),
        answered_by: None,
        voice: false,
        streaming: true,
        tools: s.tools.clone(),
        error: s.error.clone(),
        notices: s.notices.clone(),
    }
}

#[cfg(test)]
mod tests {
    use lmgw_client::requests::ErrorFrame;

    use super::*;

    fn convo() -> Convo {
        Convo {
            thread_id: Some(7),
            streaming: Some(Streaming {
                thread_id: 7,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn label_frame() -> Frame {
        Frame::Error(ErrorFrame {
            message: "MCP server 'kai': device 'Kai desktop' is offline".into(),
            code: Some("tool_offline".into()),
            server_label: Some("kai".into()),
            ..Default::default()
        })
    }

    fn done(id: Option<i64>) -> Frame {
        Frame::Done {
            message_id: id,
            answered_by: None,
            aborted: false,
        }
    }

    #[test]
    fn a_label_notice_survives_done_onto_the_saved_reply() {
        let mut c = convo();
        c.frame(label_frame());
        c.frame(Frame::Delta("Hi".into()));
        let (d, was_done) = c.frame(done(Some(11))).unwrap();
        assert!(was_done);
        assert_eq!(d.removed, [STREAMING_ID]);
        let v = &d.upsert[0];
        assert_eq!((v.id, v.text.as_str(), v.error.as_ref()), (11, "Hi", None));
        assert_eq!(
            v.notices,
            ["Kai desktop is offline. Answered without its tools."]
        );
        assert!(c.unsaved.is_none());
    }

    #[test]
    fn the_notice_stays_when_the_feed_or_a_reload_shows_the_reply_again() {
        let mut c = convo();
        c.frame(label_frame());
        c.frame(done(Some(11)));
        let m = Message {
            id: 11,
            thread_id: 7,
            role: "assistant".into(),
            content: "Hi".into(),
            ..Default::default()
        };
        assert_eq!(view_with(&m, c.notes.get(&11)).notices.len(), 1);
        // The feed's copy came first: done shows it with the notice.
        let mut c2 = convo();
        c2.messages.insert(11, m);
        c2.frame(label_frame());
        let (d, _) = c2.frame(done(Some(11))).unwrap();
        assert_eq!(d.upsert[0].id, 11);
        assert_eq!(d.upsert[0].notices.len(), 1);
    }

    #[test]
    fn a_fatal_error_persists_on_the_unsaved_turn() {
        let mut c = convo();
        c.frame(Frame::Error(ErrorFrame {
            message: "held".into(),
            code: Some("gpu_hold".into()),
            ..Default::default()
        }));
        let (d, _) = c.frame(done(None)).unwrap();
        assert!(d.removed.is_empty());
        assert_eq!(d.upsert[0].id, STREAMING_ID);
        assert_eq!(d.upsert[0].error.as_deref(), Some("held (gpu_hold)"));
        assert!(d.upsert[0].notices.is_empty());
        assert!(c.unsaved.is_some());
    }

    #[test]
    fn a_notice_alone_with_no_saved_reply_is_kept_too() {
        let mut c = convo();
        c.frame(label_frame());
        let (d, _) = c.frame(done(None)).unwrap();
        assert_eq!(d.upsert[0].notices.len(), 1);
    }

    fn message(id: i64, thread_id: i64) -> Message {
        Message {
            id,
            thread_id,
            role: "assistant".into(),
            content: "old".into(),
            ..Default::default()
        }
    }

    #[test]
    fn new_conversation_empties_the_view_at_once() {
        let mut c = convo();
        c.messages.insert(3, message(3, 7));
        c.thread_title = Some("Old".into());
        c.unsaved = Some(Streaming {
            thread_id: 7,
            error: Some("held".into()),
            ..Default::default()
        });
        let d = c.cleared();
        assert!(d.reset && d.upsert.is_empty() && d.removed.is_empty());
        assert_eq!(d.thread_id, None);
        let v = c.view("Kai");
        assert_eq!((v.thread_id, v.thread_title), (None, None));
        assert!(c.messages.is_empty() && c.unsaved.is_none());
    }

    #[test]
    fn late_events_of_the_old_thread_stay_out_of_the_new_view() {
        let mut c = convo();
        c.cleared();
        // A still-streaming reply of the old thread, and its done.
        assert!(c.frame(Frame::Delta("late".into())).is_none());
        let (d, was_done) = c.frame(done(Some(9))).unwrap();
        assert!(was_done && d.upsert.is_empty());
        // A message.added of the old thread.
        let ev = FeedEvent::MessageAdded(lmgw_client::types::chat_feed::MessagesChanged {
            thread_id: 7,
            messages: vec![message(10, 7)],
            ..Default::default()
        });
        let asks = c.event(&ev);
        assert!(asks.delta.is_none() && c.messages.is_empty());
        // The new thread arrives whole: only its messages.
        let detail = ThreadDetail::default();
        let d = c.thread_loaded(&detail);
        assert!(d.reset && d.upsert.is_empty());
    }
}
