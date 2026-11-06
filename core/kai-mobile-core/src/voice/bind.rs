//! What a call binds (design §7.3, §7.12): the Kai folder's current thread (`chat_folder`), which
//! every entry point but one uses, or the thread an opened thread's Talk names (`chat_thread`).
//! Sans-IO: the link's actor asks [`choose`] with the Kai conversation's thread, and the voice
//! task's POST builds its query with [`call_bind`].

use lmgw_client::requests::calls::CallBind;

use crate::CoreError;

/// The thread a call binds by `chat_thread`, or `None` for the Kai folder, for a start that named
/// `asked` while the Kai conversation is on `kai_thread`.
///
/// - No thread named: the Kai folder (the overlay, the tile, the headset, the voice screen's Talk).
/// - The Kai conversation's own thread (it may have become the current one after the owner opened
///   it): the Kai folder too, so the folder's rollover and the rebind after `folder.current` keep
///   applying (§7.7).
/// - A temporary thread (a negative id): refused before any request (§7.12, rule 6). lmgw would
///   bind its temporary thread, which it keeps out of the history and the feed, so the opened view
///   would not follow the call; the negative bind is the lock screen's (§9.6, Later).
pub fn choose(asked: Option<i64>, kai_thread: Option<i64>) -> Result<Option<i64>, CoreError> {
    match asked {
        None => Ok(None),
        Some(t) if t < 0 => Err(CoreError::NotReady {
            reason: "Kai has no voice for a temporary thread: lmgw keeps it out of the feed, so \
                     this view would not follow the call."
                .to_owned(),
        }),
        Some(t) if Some(t) == kai_thread => Ok(None),
        Some(t) => Ok(Some(t)),
    }
}

/// The POST's binding: `chat_thread=<thread>` for a thread's call, else the Kai folder
/// `chat_folder=<folder>` with `new` (§6.1's New conversation).
pub fn call_bind(thread: Option<i64>, folder: i64, new: bool) -> CallBind {
    match thread {
        Some(t) => CallBind::Thread(t),
        None => CallBind::Folder { id: folder, new },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_point_but_an_opened_threads_talk_binds_the_kai_folder() {
        assert_eq!(choose(None, Some(7)).unwrap(), None);
        assert_eq!(choose(None, None).unwrap(), None);
        assert_eq!(choose(Some(12), Some(7)).unwrap(), Some(12));
        assert_eq!(
            choose(Some(12), None).unwrap(),
            Some(12),
            "no Kai thread known yet"
        );
    }

    #[test]
    fn the_kai_conversations_own_thread_binds_by_the_folder() {
        assert_eq!(choose(Some(7), Some(7)).unwrap(), None);
    }

    #[test]
    fn a_temporary_thread_is_refused_before_any_request() {
        let err = choose(Some(-3), Some(7)).unwrap_err();
        assert!(
            matches!(&err, CoreError::NotReady { reason } if reason.contains("temporary")),
            "{err:?}"
        );
    }

    #[test]
    fn the_post_binds_the_thread_or_the_folder() {
        assert_eq!(call_bind(Some(12), 5, false), CallBind::Thread(12));
        assert_eq!(
            call_bind(Some(12), 5, true),
            CallBind::Thread(12),
            "new is the folder's"
        );
        assert_eq!(
            call_bind(None, 5, true),
            CallBind::Folder { id: 5, new: true }
        );
    }
}
