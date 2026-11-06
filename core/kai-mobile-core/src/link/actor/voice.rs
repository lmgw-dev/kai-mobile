//! The actor's side of a thread's call (design §7.12): the ticket of a call that binds an opened
//! thread by `chat_thread`, that thread's K16 facts (who else answers there) for the voice task as
//! the feed changes them, a thread gone from the feed (a move waiting to go there is called off),
//! and which replies the owner hears in the call (no notification). The Kai call's facts come with
//! the conversation's view, as before; the rules are `crate::voice::bind`'s and the call's.

use lmgw_client::feed::{FeedEvent, ThreadNow};

use super::Actor;
use crate::voice::actor::{Cmd as VoiceCmd, Ticket};

/// The owner hears `thread`'s replies in the phone's call, so none notifies (§12.3): any
/// Kai-folder thread's during the Kai call (`call_thread` `None`), as before; only the bound
/// thread's during a thread's call, which is not the Kai conversation.
pub(super) fn hears(in_call: bool, call_thread: Option<i64>, thread: i64) -> bool {
    in_call && call_thread.is_none_or(|t| t == thread)
}

impl Actor {
    /// `ticket` (the Kai call's) made the ticket of a call bound to thread `thread`: its holder and
    /// its title as the phone knows them (the opened thread's; empty until lmgw names it).
    pub(super) fn thread_ticket(&self, thread: i64, ticket: Ticket) -> Ticket {
        let title = self
            .opened
            .as_ref()
            .filter(|o| o.id == thread)
            .map(|o| o.view(self.convo.thread_id).title)
            .unwrap_or_default();
        Ticket {
            bind_thread: Some(thread),
            voice_by: self.convo.voice_on(thread),
            thread: Some((thread, title)),
            ..ticket
        }
    }

    /// [`hears`] for the running call.
    pub(super) fn hears(&self, thread: i64) -> bool {
        hears(self.in_call, self.call_thread, thread)
    }

    /// A thread deleted or out of this device's reach, as the feed says it: the voice task calls
    /// off a move waiting to go there (§7.12).
    pub(super) fn call_thread_gone(&self, ev: &FeedEvent) {
        let thread = match ev {
            FeedEvent::ThreadDeleted(g) => g.thread_id,
            FeedEvent::ThreadCreated(ThreadNow::Gone(g))
            | FeedEvent::ThreadUpdated(ThreadNow::Gone(g)) => g.thread_id,
            _ => return,
        };
        if self.in_call {
            let _ = self.ctx.voice.send(VoiceCmd::ThreadGone { thread });
        }
    }

    /// The running call binds `thread` by `chat_thread`, or no call does.
    pub(super) fn call_thread(&mut self, thread: Option<i64>) {
        self.call_thread = thread;
        self.call_live = None;
        self.publish_call_live();
    }

    /// Who else answers in the thread the running call binds, when it changed.
    pub(super) fn publish_call_live(&mut self) {
        let Some(thread) = self.call_thread else {
            return;
        };
        let answering = self.convo.answering_in(thread);
        if self.call_live.as_ref() == Some(&answering) {
            return;
        }
        self.call_live = Some(answering.clone());
        let _ = self.ctx.voice.send(VoiceCmd::Live { thread, answering });
    }
}
