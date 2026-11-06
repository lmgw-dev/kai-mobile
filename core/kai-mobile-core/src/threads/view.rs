//! What Kotlin sees of the threads browser and the opened thread (design §6.4): snapshots, whole,
//! as the conversation's views are; the opened thread's messages come as `MessagesDelta`s like the
//! Kai thread's.

/// Whether the browser's lists are there (design §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BrowserStatus {
    /// The first read is under way (or the link is not up yet: [`BrowserView::note`] says why).
    Loading,
    /// The lists came.
    Ready,
    /// The last read failed ([`BrowserView::note`]); the lists shown, if any, are the last good
    /// ones, and the next event or visibility reads again.
    Failed,
}

/// The threads browser (design §6.4), whole.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowserView {
    pub status: BrowserStatus,
    /// Why the lists are not current: the connection's words, a failed read's.
    pub note: Option<String>,
    /// The folders in lmgw's order, each with its threads (pinned first, then the most recently
    /// active, as lmgw lists them), then the threads in no folder (a section without a folder id),
    /// when there are any.
    pub sections: Vec<FolderSection>,
    /// Temporary threads (open only: lmgw refuses every action on them).
    pub temporary: Vec<ThreadItem>,
    /// How many threads are archived (as the device may see them).
    pub archived_count: i64,
    /// The archived threads, most recently archived first, while the archived section is open.
    pub archived: Option<Vec<ThreadItem>>,
    /// The folders a thread can be moved to (the device sees them), in lmgw's order.
    pub folders: Vec<FolderChoice>,
}

/// One folder of the browser and its threads.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FolderSection {
    /// `None` for the threads in no folder (or in a folder hidden from the device).
    pub folder_id: Option<i64>,
    pub name: String,
    /// The Kai conversation's folder: its New thread is New conversation (§6.1).
    pub kai: bool,
    /// One ongoing conversation (a rolling current thread).
    pub ongoing: bool,
    /// New thread is offered: the folder's defaults name a model.
    pub can_create: bool,
    /// Why not, in the dashboard's words, when the folder names no model.
    pub steps: Option<String>,
    pub threads: Vec<ThreadItem>,
}

/// One thread in a list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ThreadItem {
    /// Negative for a temporary thread.
    pub id: i64,
    pub title: String,
    pub folder_id: Option<i64>,
    pub pinned: bool,
    pub archived: bool,
    pub temporary: bool,
    /// The Kai conversation's thread: it opens as the Kai conversation (§6.4).
    pub kai: bool,
    /// The day of its last change (`YYYY-MM-DD`, UTC, as lmgw has it).
    pub day: String,
}

/// A folder a thread can be moved to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FolderChoice {
    pub folder_id: i64,
    pub name: String,
}

/// Whether the opened thread can be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OpenedStatus {
    /// Being read.
    Loading,
    Ready,
    /// Deleted, or out of this device's reach ([`OpenedView::note`]).
    Gone,
    /// The read failed ([`OpenedView::note`]); read again at the next event or visibility.
    Failed,
}

/// The opened thread (design §6.4), whole.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenedView {
    pub status: OpenedStatus,
    pub thread_id: i64,
    /// Empty until its first read.
    pub title: String,
    pub folder_id: Option<i64>,
    pub pinned: bool,
    pub archived: bool,
    pub temporary: bool,
    /// It is the Kai conversation's thread now.
    pub kai: bool,
    pub note: Option<String>,
    /// Who else is answering in it now.
    pub answering: Vec<String>,
    /// Who holds its voice, when another client does.
    pub voice_by: Option<String>,
    /// A text send of the phone's is streaming into it.
    pub sending: bool,
}

/// Where a search looks (design §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SearchIn {
    Active,
    Archived,
    All,
}

/// One page of search results.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SearchPageView {
    /// How many threads match in all.
    pub total: i64,
    /// This page's threads, best match first.
    pub threads: Vec<SearchThreadView>,
    /// The next page's offset; `None` on the last.
    pub next_offset: Option<i64>,
}

/// A thread with matches.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SearchThreadView {
    pub thread_id: i64,
    pub title: String,
    pub folder_id: Option<i64>,
    pub archived: bool,
    pub kai: bool,
    pub match_count: i64,
    /// Its best matches (up to three).
    pub hits: Vec<SearchHitView>,
}

/// Where a match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HitKind {
    Title,
    Message,
    Attachment,
    /// A kind this build does not know.
    Other,
}

/// One match.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SearchHitView {
    pub kind: HitKind,
    /// The message, for a message's match: the opened thread scrolls to it.
    pub message_id: Option<i64>,
    /// The text around the match, in parts; `hit` marks the matching words.
    pub parts: Vec<SnippetPart>,
}

/// A part of a snippet.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SnippetPart {
    pub text: String,
    pub hit: bool,
}
