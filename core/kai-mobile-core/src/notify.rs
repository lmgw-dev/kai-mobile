//! Kai's replies as notifications (design §12.3): an assistant message added in the Kai folder's
//! threads while the app is not visible (and no call of the phone's runs: the owner hears that
//! one) is a notification per thread, built from the event alone: title the thread's title, text
//! the reply's start, `message.updated` replacing the text while it shows, `message.deleted` of
//! that message withdrawing it. During a catch-up replies are collected, and at `caught_up` each
//! thread gets one notification with its latest reply and the count. Opening the thread withdraws
//! it. Ids are the thread's, so a resumed feed or a restarted process never alerts twice.

use std::collections::BTreeMap;

use lmgw_client::feed::{MessagesChanged, MessagesDeleted};

use crate::views::{NotificationId, NotificationKind, NotificationView, Update};

/// One thread's reply notification.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reply {
    title: String,
    message_id: i64,
    text: String,
    count: u32,
}

/// The book (module doc).
#[derive(Debug, Default)]
pub struct Replies {
    /// Shown now, by thread.
    shown: BTreeMap<i64, Reply>,
    /// Collected during the catch-up (and until the Kai folder's threads are known), by thread.
    collecting: BTreeMap<i64, Reply>,
}

impl Replies {
    /// The shell's active reply notifications at start (by thread): their messages are unknown,
    /// so an update or delete leaves them as they are; a new reply replaces them.
    pub fn set_shown(&mut self, threads: Vec<i64>) {
        for t in threads {
            self.shown.entry(t).or_insert(Reply {
                title: String::new(),
                message_id: i64::MIN,
                text: String::new(),
                count: 1,
            });
        }
    }

    /// `message.added` (`updated`: `message.updated`). `collect`: before `caught_up`, or while the
    /// Kai folder's threads are not known; `quiet`: the app is visible; `kai`: whether the thread
    /// is in the Kai folder (only asked when not collecting).
    pub fn message(
        &mut self,
        ev: &MessagesChanged,
        updated: bool,
        collect: bool,
        quiet: bool,
        kai: impl Fn(i64) -> bool,
    ) -> Vec<Update> {
        let Some(latest) = ev.messages.iter().rev().find(|m| m.role == "assistant") else {
            return Vec::new();
        };
        let replies = ev.messages.iter().filter(|m| m.role == "assistant").count() as u32;
        let thread = ev.thread_id;
        if collect {
            let entry = self.collecting.entry(thread).or_insert(Reply {
                title: ev.thread_title.clone(),
                message_id: latest.id,
                text: String::new(),
                count: 0,
            });
            if updated {
                if entry.message_id == latest.id {
                    entry.text = latest.content.clone();
                }
                return Vec::new();
            }
            entry.title = ev.thread_title.clone();
            entry.message_id = latest.id;
            entry.text = latest.content.clone();
            entry.count += replies;
            return Vec::new();
        }
        if updated {
            // Only a notification still shown is replaced (setOnlyAlertOnce: no second alert).
            return match self.shown.get_mut(&thread) {
                Some(r) if r.message_id == latest.id => {
                    r.text = latest.content.clone();
                    r.title = ev.thread_title.clone();
                    vec![notify(thread, r)]
                }
                _ => Vec::new(),
            };
        }
        if quiet || !kai(thread) {
            return Vec::new();
        }
        let count = self.shown.get(&thread).map_or(0, |r| r.count) + replies;
        let r = Reply {
            title: ev.thread_title.clone(),
            message_id: latest.id,
            text: latest.content.clone(),
            count,
        };
        let up = notify(thread, &r);
        self.shown.insert(thread, r);
        vec![up]
    }

    /// `message.deleted`: a shown reply's deletion withdraws its notification.
    pub fn deleted(&mut self, ev: &MessagesDeleted) -> Vec<Update> {
        if let Some(r) = self.collecting.get(&ev.thread_id)
            && ev.message_ids.contains(&r.message_id)
        {
            self.collecting.remove(&ev.thread_id);
        }
        match self.shown.get(&ev.thread_id) {
            Some(r) if ev.message_ids.contains(&r.message_id) => {
                self.shown.remove(&ev.thread_id);
                vec![withdraw(ev.thread_id)]
            }
            _ => Vec::new(),
        }
    }

    /// The catch-up is over and the Kai folder's threads are known: one notification per thread
    /// collected (unless `quiet`), with its latest reply and the count.
    pub fn flush(&mut self, quiet: bool, kai: impl Fn(i64) -> bool) -> Vec<Update> {
        let collected = std::mem::take(&mut self.collecting);
        if quiet {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (thread, mut r) in collected {
            if !kai(thread) || r.count == 0 {
                continue;
            }
            if let Some(old) = self.shown.get(&thread) {
                r.count += old.count;
            }
            out.push(notify(thread, &r));
            self.shown.insert(thread, r);
        }
        out
    }

    /// The thread was opened (the app shows it): its notification goes.
    pub fn opened(&mut self, thread: i64) -> Vec<Update> {
        if self.shown.remove(&thread).is_some() {
            vec![withdraw(thread)]
        } else {
            Vec::new()
        }
    }

    /// The thread's notification is gone by another way (tapped, swiped: the shell says so).
    pub fn gone(&mut self, thread: i64) {
        self.shown.remove(&thread);
    }

    /// Every notification shown, withdrawn.
    pub fn clear(&mut self) -> Vec<Update> {
        self.collecting.clear();
        std::mem::take(&mut self.shown)
            .into_keys()
            .map(withdraw)
            .collect()
    }
}

fn id(thread: i64) -> NotificationId {
    NotificationId {
        kind: NotificationKind::Reply,
        key: thread.to_string(),
    }
}

fn withdraw(thread: i64) -> Update {
    Update::Withdraw { id: id(thread) }
}

fn notify(thread: i64, r: &Reply) -> Update {
    Update::Notify {
        notification: NotificationView {
            id: id(thread),
            title: if r.title.trim().is_empty() {
                "Kai".to_owned()
            } else {
                r.title.clone()
            },
            text: r.text.clone(),
            thread_id: Some(thread),
            approval: None,
            count: r.count.max(1),
        },
    }
}

#[cfg(test)]
mod tests {
    use lmgw_client::requests::Message;

    use super::*;

    fn added(thread: i64, id: i64, text: &str) -> MessagesChanged {
        MessagesChanged {
            thread_id: thread,
            thread_title: "Kai".into(),
            messages: vec![Message {
                id,
                thread_id: thread,
                role: "assistant".into(),
                content: text.into(),
                ..Default::default()
            }],
            by: None,
        }
    }

    #[test]
    fn a_catch_up_collapses_per_thread_and_a_live_reply_notifies() {
        let mut r = Replies::default();
        assert!(
            r.message(&added(7, 1, "one"), false, true, false, |_| true)
                .is_empty()
        );
        assert!(
            r.message(&added(7, 2, "two"), false, true, false, |_| true)
                .is_empty()
        );
        assert!(
            r.message(&added(9, 3, "other"), false, true, false, |_| true)
                .is_empty()
        );
        let out = r.flush(false, |t| t == 7);
        let [Update::Notify { notification: n }] = out.as_slice() else {
            panic!("{out:?}")
        };
        assert_eq!((n.text.as_str(), n.count), ("two", 2));
        let out = r.message(&added(7, 4, "three"), true, false, false, |_| true);
        assert!(
            out.is_empty(),
            "an update of another message than the shown one"
        );
        let out = r.message(&added(7, 2, "two, edited"), true, false, false, |_| true);
        assert_eq!(out.len(), 1);
        assert!(
            r.message(&added(7, 5, "x"), false, false, true, |_| true)
                .is_empty(),
            "visible"
        );
        assert_eq!(r.opened(7).len(), 1);
    }

    #[test]
    fn a_deleted_reply_withdraws_its_notification() {
        let mut r = Replies::default();
        r.message(&added(7, 1, "one"), false, false, false, |_| true);
        let out = r.deleted(&MessagesDeleted {
            thread_id: 7,
            message_ids: vec![1],
            by: None,
        });
        assert_eq!(out, [withdraw(7)]);
    }
}
