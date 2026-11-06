//! The browser's lists (design §6.4), sans-IO: read when the browser shows, dropped when it closes,
//! read again after the events that change them, one read in flight and one more after it when
//! something came meanwhile. lmgw's order is kept as it lists it (pinned first, then the most
//! recently active); the phone never sorts.

use lmgw_client::{
    feed::FeedEvent,
    requests::{Folder, SearchPage, ThreadList, ThreadRow},
};

use super::view::{
    BrowserStatus, BrowserView, FolderChoice, FolderSection, HitKind, SearchHitView,
    SearchPageView, SearchThreadView, SnippetPart, ThreadItem,
};

/// The lists one read brought: the active list (with the folders, the temporary threads and the
/// archived count) and, while the archived section is open, the archived threads.
#[derive(Debug, Clone, Default)]
pub struct Lists {
    pub active: ThreadList,
    pub archived: Option<Vec<ThreadRow>>,
}

/// The browser's state (module doc).
#[derive(Debug, Default)]
pub struct Browser {
    pub shown: bool,
    pub archived_shown: bool,
    lists: Option<Lists>,
    /// A read is in flight (its generation is [`Self::generation`]).
    reading: bool,
    /// Something changed while it was: read once more after it.
    dirty: bool,
    generation: u64,
    status: Option<BrowserStatus>,
    note: Option<String>,
}

impl Browser {
    /// The browser shows (`true`) or closed: closed, its lists are dropped. Whether to read now.
    pub fn show(&mut self, shown: bool) -> bool {
        if shown == self.shown {
            return false;
        }
        self.shown = shown;
        if !shown {
            *self = Self {
                generation: self.generation + 1,
                ..Default::default()
            };
            return false;
        }
        true
    }

    /// The archived section opened or closed: whether to read now.
    pub fn show_archived(&mut self, shown: bool) -> bool {
        if shown == self.archived_shown {
            return false;
        }
        self.archived_shown = shown;
        if !shown && let Some(l) = self.lists.as_mut() {
            l.archived = None;
        }
        shown && self.shown
    }

    /// Whether `ev` changes what the browser lists.
    pub fn touches(ev: &FeedEvent) -> bool {
        matches!(
            ev,
            FeedEvent::ThreadCreated(_)
                | FeedEvent::ThreadUpdated(_)
                | FeedEvent::ThreadDeleted(_)
                | FeedEvent::FolderCreated(_)
                | FeedEvent::FolderUpdated(_)
                | FeedEvent::FolderDeleted(_)
                | FeedEvent::MessageAdded(_)
                | FeedEvent::Resync(_)
        )
    }

    /// A read is wanted: the generation to read under, or `None` (the browser does not show, or a
    /// read is in flight: it reads once more after it).
    pub fn want(&mut self) -> Option<u64> {
        if !self.shown {
            return None;
        }
        if self.reading {
            self.dirty = true;
            return None;
        }
        self.reading = true;
        self.dirty = false;
        self.generation += 1;
        Some(self.generation)
    }

    /// A read under `generation` ended without an answer to show (lmgw refused it, and the link's
    /// state says why): it is no longer in flight, so the next one can start. An older
    /// generation's is dropped.
    pub fn abandon(&mut self, generation: u64) {
        if generation == self.generation {
            self.reading = false;
            self.dirty = false;
        }
    }

    /// A read is wanted while the app is not on screen: it waits for the app to be visible.
    pub fn defer(&mut self) {
        if self.shown {
            self.dirty = true;
        }
    }

    /// A read under `generation` ended: whether the view changed, and whether to read again (an
    /// event came while it ran). A read of a closed browser's, or of an older generation, is
    /// dropped.
    pub fn read(&mut self, generation: u64, result: Result<Lists, String>) -> (bool, bool) {
        if generation != self.generation || !self.shown {
            return (false, false);
        }
        self.reading = false;
        match result {
            Ok(mut lists) => {
                if !self.archived_shown {
                    lists.archived = None;
                }
                self.lists = Some(lists);
                self.status = Some(BrowserStatus::Ready);
                self.note = None;
            }
            Err(why) => {
                self.status = Some(BrowserStatus::Failed);
                self.note = Some(why);
            }
        }
        (true, std::mem::take(&mut self.dirty))
    }

    /// The link is not up: the lists stay as they were, with why.
    pub fn waiting(&mut self, words: &str) {
        self.reading = false;
        self.dirty = false;
        if self.status != Some(BrowserStatus::Ready) {
            self.status = Some(BrowserStatus::Loading);
        }
        self.note = Some(words.to_owned());
    }

    /// The folders as last read (a create's or a move's lookups use lmgw's own answer instead).
    pub fn folders(&self) -> &[Folder] {
        self.lists
            .as_ref()
            .map_or(&[], |l| l.active.folders.as_slice())
    }

    /// The view, with `kai_folder` and `kai_thread` the Kai conversation's.
    pub fn view(&self, kai_folder: Option<i64>, kai_thread: Option<i64>) -> BrowserView {
        let item = |t: &ThreadRow| item(t, kai_thread);
        let Some(lists) = self.lists.as_ref() else {
            return BrowserView {
                status: self.status.unwrap_or(BrowserStatus::Loading),
                note: self.note.clone(),
                sections: Vec::new(),
                temporary: Vec::new(),
                archived_count: 0,
                archived: None,
                folders: Vec::new(),
            };
        };
        let active = &lists.active;
        let mut sections: Vec<FolderSection> = active
            .folders
            .iter()
            .map(|f| {
                let model = folder_model(f).is_ok();
                FolderSection {
                    folder_id: Some(f.id),
                    name: f.name.clone(),
                    kai: Some(f.id) == kai_folder,
                    ongoing: f.ongoing.is_some(),
                    can_create: model,
                    steps: (!model).then(|| no_model_steps(&f.name)),
                    threads: active
                        .threads
                        .iter()
                        .filter(|t| t.folder_id == Some(f.id))
                        .map(item)
                        .collect(),
                }
            })
            .collect();
        let known =
            |id: Option<i64>| id.is_some_and(|id| active.folders.iter().any(|f| f.id == id));
        let loose: Vec<ThreadItem> = active
            .threads
            .iter()
            .filter(|t| !known(t.folder_id))
            .map(item)
            .collect();
        if !loose.is_empty() {
            sections.push(FolderSection {
                folder_id: None,
                name: String::new(),
                kai: false,
                ongoing: false,
                can_create: false,
                steps: None,
                threads: loose,
            });
        }
        BrowserView {
            status: self.status.unwrap_or(BrowserStatus::Loading),
            note: self.note.clone(),
            sections,
            temporary: active.temporary.iter().map(item).collect(),
            archived_count: active.archived_count,
            archived: lists
                .archived
                .as_ref()
                .map(|a| a.iter().map(item).collect()),
            folders: active
                .folders
                .iter()
                .map(|f| FolderChoice {
                    folder_id: f.id,
                    name: f.name.clone(),
                })
                .collect(),
        }
    }
}

fn item(t: &ThreadRow, kai_thread: Option<i64>) -> ThreadItem {
    ThreadItem {
        id: t.id,
        title: t.title.clone(),
        folder_id: t.folder_id,
        pinned: t.pinned,
        archived: t.archived_at.is_some(),
        temporary: t.temporary || t.id < 0,
        kai: Some(t.id) == kai_thread,
        day: t.updated_at.chars().take(10).collect(),
    }
}

/// The dashboard's steps for a folder whose defaults name no model (§6.1's `folder_no_model`).
pub fn no_model_steps(name: &str) -> String {
    format!(
        "The folder '{name}' has no model, so the phone cannot start a thread in it. On lmgw: \
         Chat, the folder's menu → Folder settings, pick its Model."
    )
}

/// The model a folder's new thread starts with, or the dashboard's steps.
pub fn folder_model(f: &Folder) -> Result<String, String> {
    match f.defaults.model_alias.as_deref().map(str::trim) {
        Some(m) if !m.is_empty() => Ok(m.to_owned()),
        _ => Err(no_model_steps(&f.name)),
    }
}

/// lmgw's minimum for a search (`400 bad_request` below it).
pub const SEARCH_MIN_CHARS: usize = 2;

/// Whether `q` is long enough to search for.
pub fn searchable(q: &str) -> bool {
    q.trim().chars().count() >= SEARCH_MIN_CHARS
}

/// A search page as the browser shows it.
pub fn search_view(page: SearchPage, kai_thread: Option<i64>) -> SearchPageView {
    SearchPageView {
        total: page.total_threads,
        next_offset: page.next_offset,
        threads: page
            .threads
            .into_iter()
            .map(|t| SearchThreadView {
                kai: Some(t.thread_id) == kai_thread,
                thread_id: t.thread_id,
                title: t.title,
                folder_id: t.folder_id,
                archived: t.archived,
                match_count: t.match_count,
                hits: t
                    .hits
                    .into_iter()
                    .map(|h| SearchHitView {
                        kind: match h.kind.as_str() {
                            "t" => HitKind::Title,
                            "m" => HitKind::Message,
                            "a" => HitKind::Attachment,
                            _ => HitKind::Other,
                        },
                        message_id: h.message_id,
                        parts: snippet(&h.snippet),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// A snippet split at lmgw's match marks (U+E000 opens a match, U+E001 closes it): plain text with
/// a `hit` flag, never markup. An unclosed mark runs to the end; empty parts are left out.
pub fn snippet(s: &str) -> Vec<SnippetPart> {
    let mut parts = Vec::new();
    let mut hit = false;
    let mut text = String::new();
    let flush = |text: &mut String, hit: bool, parts: &mut Vec<SnippetPart>| {
        if !text.is_empty() {
            parts.push(SnippetPart {
                text: std::mem::take(text),
                hit,
            });
        }
    };
    for c in s.chars() {
        match c {
            '\u{E000}' => {
                flush(&mut text, hit, &mut parts);
                hit = true;
            }
            '\u{E001}' => {
                flush(&mut text, hit, &mut parts);
                hit = false;
            }
            c => text.push(c),
        }
    }
    flush(&mut text, hit, &mut parts);
    parts
}
