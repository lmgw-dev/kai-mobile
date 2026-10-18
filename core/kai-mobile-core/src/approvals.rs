//! The open approvals (design §5.4, §11, §12.3): the calls waiting for a decision that the phone
//! can see, their notifications, and the verdicts given on the phone.
//!
//! **Two sources, one card per call.** The feed's open set (every thread the phone sees) and the
//! running voice call's own `mcp_approval_request` items (api §6.4; module [`call`]), merged by
//! `approval_request_id`: the feed's facts win where both have a call, and a call the voice call
//! showed first is a card (and, while no card is seen, a notification) at once. Each id is asked
//! once, whichever source brought it.
//!
//! **Reconciled at every connection.** A `FeedReader` lives for one connection, so the core keeps,
//! across reconnects and process deaths, the ids it notified for (the system's active
//! notifications, which outlive the process: the shell hands them over at start) and the ids the
//! user dismissed (`approvals.json`, cut at each `caught_up` to the ids still open). Each reader
//! is built with `FeedReader::knowing(both)`, so `caught_up` yields `Withdrawn` for the ones shown
//! that are gone and `Added` for those not known, never `Open`; `Added` is filtered by the
//! dismissed ids, since a call that left the phone's reach and came back arrives as `Added` again.
//!
//! **Notified while no card is seen** (§11, §12.3): one notification per `approval_request_id`,
//! raised for each call added (at `caught_up` or after it), and for the calls not yet notified when
//! the cards leave the screen; withdrawn when the call waits no more (`Withdrawn`: decided, its
//! reply or thread gone, out of reach; lmgw's word on the call) or when the phone decided it. The
//! cards are seen while the app is visible or the assistant overlay shows them unlocked (§9.2).
//!
//! **Verdicts** (lmgw needs one for every call of a reply at once, api §6.3): kept per reply until
//! its last call has one, then sent together, through the running call when it showed every one
//! of them (so its resumed reply is spoken), else by the route. A held verdict is never asked again
//! (not when the cards leave the screen, not at a reconnect's `caught_up`), and while no card is
//! seen the reply's calls still waiting are raised again saying which verdicts wait for them, so
//! the one held is not invisible. A verdict lmgw did not take is asked again with lmgw's words on
//! its card.
//!
//! **What lmgw decided** ([`Approvals::feed_decided`]): a verdict sent stays sent until the feed's
//! `approval.decided` (or its withdrawal) closes the card; the phone never closes it on its own
//! inference. lmgw's decision can differ from the verdict sent — the owner's own words in the call
//! (or a typed message) reached lmgw first and declined what waits, another client decided first,
//! or the call was approved and its turn could not start — and the owner is told so then, once.

use std::collections::{BTreeMap, BTreeSet};

use lmgw_client::feed::{ApprovalDecided, ApprovalRequested, ApprovalsChange};

use crate::views::{
    ApprovalNote, ApprovalView, NotificationId, NotificationKind, NotificationView, Update,
};

pub mod call;

pub use call::{CallVerdicts, Shown, Waiting};

/// The book (module doc).
#[derive(Debug, Default)]
pub struct Approvals {
    /// The feed's calls waiting now, oldest first, as the reader holds them.
    open: Vec<ApprovalRequested>,
    /// The calls the voice calls' sessions showed, oldest first (module [`call`]).
    shown: Vec<Shown>,
    /// The voice call that runs now (its number), whose session takes the verdicts it showed.
    live_call: Option<u64>,
    /// The ids notified for (shown now).
    notified: BTreeSet<String>,
    /// The ids the user dismissed (`approvals.json`).
    dismissed: BTreeSet<String>,
    /// Verdicts given on the phone, by id, held until the reply's other calls have one.
    verdicts: BTreeMap<String, bool>,
    /// The ids whose verdicts went to lmgw and wait for its answer.
    sent: BTreeSet<String>,
    /// Why lmgw did not take the last verdict for an id (its words, on the card).
    notes: BTreeMap<String, String>,
    /// The ids lmgw said were decided on the call (`lmgw.approval.decided`) or by the route (its
    /// 409 `approval_decided`), kept out of the cards until the feed withdraws them or its next
    /// `caught_up`, so a set read before the decision does not show them again for a moment.
    gone: BTreeSet<String>,
    /// The verdicts the phone sent to lmgw, by id, kept past their cards until the feed's
    /// `approval.decided` says lmgw decided otherwise (told once), or the next `caught_up` finds
    /// the call no longer waiting.
    given: BTreeMap<String, Given>,
}

/// A verdict the phone sent (module doc: what lmgw decided).
#[derive(Debug, Clone)]
struct Given {
    approve: bool,
    /// `<label>__<name>`, for the words.
    tool: String,
}

/// What the book asks of the actor besides updates.
#[derive(Debug, Default, PartialEq)]
pub struct Out {
    pub updates: Vec<Update>,
    /// `approvals.json` changed.
    pub save: bool,
    /// The open set changed (the cards).
    pub cards: bool,
}

impl Out {
    fn merge(&mut self, o: Out) {
        self.updates.extend(o.updates);
        self.save |= o.save;
        self.cards |= o.cards;
    }
}

impl Approvals {
    /// A book with the dismissed ids `approvals.json` holds.
    pub fn new(dismissed: Vec<String>) -> Self {
        Self {
            dismissed: dismissed.into_iter().collect(),
            ..Default::default()
        }
    }

    /// The ids a new connection's reader knows: notified and dismissed.
    pub fn known(&self) -> Vec<String> {
        self.notified.union(&self.dismissed).cloned().collect()
    }

    pub fn dismissed(&self) -> Vec<String> {
        self.dismissed.iter().cloned().collect()
    }

    /// The shell's active approval notifications at start.
    pub fn set_notified(&mut self, ids: Vec<String>) {
        self.notified = ids.into_iter().collect();
    }

    /// Every call waiting now, by id: the feed's (its facts win), then those only a call showed;
    /// none lmgw said was decided.
    fn all(&self) -> Vec<ApprovalRequested> {
        let mut all: Vec<ApprovalRequested> = self
            .open
            .iter()
            .filter(|a| !self.gone.contains(&a.approval_request_id))
            .cloned()
            .collect();
        for s in &self.shown {
            let id = &s.request.approval_request_id;
            if !self.gone.contains(id) && !all.iter().any(|a| &a.approval_request_id == id) {
                all.push(s.request.clone());
            }
        }
        all
    }

    fn find(&self, id: &str) -> Option<ApprovalRequested> {
        self.all().into_iter().find(|a| a.approval_request_id == id)
    }

    /// The calls of `a`'s reply (one thread's last reply), `a` among them: the thread's calls with
    /// `a`'s message, and those the same voice call's session showed in the thread, whose items
    /// name no message (lmgw keeps at most one reply waiting per thread), each joining the reply
    /// by either until none more does.
    fn reply_of(&self, a: &ApprovalRequested) -> Vec<ApprovalRequested> {
        let thread: Vec<ApprovalRequested> = self
            .all()
            .into_iter()
            .filter(|o| o.thread_id == a.thread_id)
            .collect();
        let session = |id: &str| {
            self.shown
                .iter()
                .find(|s| s.request.approval_request_id == id)
                .map(|s| s.call)
        };
        let mut ids = BTreeSet::new();
        let mut messages = BTreeSet::new();
        let mut sessions = BTreeSet::new();
        let mut joining = vec![a.clone()];
        while let Some(o) = joining.pop() {
            if !ids.insert(o.approval_request_id.clone()) {
                continue;
            }
            if o.message_id != 0 {
                messages.insert(o.message_id);
            }
            if let Some(n) = session(&o.approval_request_id) {
                sessions.insert(n);
            }
            joining.extend(
                thread
                    .iter()
                    .filter(|t| !ids.contains(&t.approval_request_id))
                    .filter(|t| {
                        (t.message_id != 0 && messages.contains(&t.message_id))
                            || session(&t.approval_request_id)
                                .is_some_and(|n| sessions.contains(&n))
                    })
                    .cloned(),
            );
        }
        thread
            .into_iter()
            .filter(|o| ids.contains(&o.approval_request_id))
            .collect()
    }

    /// The running call whose session showed every call of `reply`: its verdicts go through it,
    /// and Kai speaks the resumed reply there (api §6.4: the session answers what it showed).
    fn reply_call(&self, reply: &[ApprovalRequested]) -> Option<u64> {
        let calls: BTreeSet<Option<u64>> = reply
            .iter()
            .map(|a| self.via_call(&a.approval_request_id))
            .collect();
        match calls.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(n)] => Some(*n),
            _ => None,
        }
    }

    /// Drops what is held for ids that wait no more.
    fn prune(&mut self) {
        let ids: BTreeSet<String> = self
            .all()
            .into_iter()
            .map(|a| a.approval_request_id)
            .collect();
        self.verdicts.retain(|id, _| ids.contains(id));
        self.sent.retain(|id| ids.contains(id));
        self.notes.retain(|id, _| ids.contains(id));
    }

    /// The reader's set now (each chunk's).
    pub fn set_open(&mut self, open: Vec<ApprovalRequested>) -> bool {
        if self.open == open {
            return false;
        }
        self.open = open;
        self.prune();
        true
    }

    /// The cards (§11).
    pub fn views(&self) -> Vec<ApprovalView> {
        self.all()
            .iter()
            .map(|a| {
                let id = &a.approval_request_id;
                let reply = self.reply_of(a);
                let waiting = reply
                    .iter()
                    .filter(|o| {
                        o.approval_request_id != *id
                            && !self.verdicts.contains_key(&o.approval_request_id)
                    })
                    .count();
                ApprovalView {
                    approval_request_id: id.clone(),
                    thread_id: a.thread_id,
                    thread_title: a.thread_title.clone(),
                    message_id: a.message_id,
                    server_label: a.server_label.clone(),
                    name: a.name.clone(),
                    arguments: compact_arguments(&a.arguments),
                    verdict: self.verdicts.get(id).copied(),
                    sent: self.sent.contains(id),
                    waiting: u32::try_from(waiting).unwrap_or(u32::MAX),
                    in_call: self.reply_call(&reply).is_some(),
                    note: self.notes.get(id).cloned(),
                }
            })
            .collect()
    }

    /// One change the reader yielded; `seen`: a card is on screen.
    pub fn change(&mut self, change: ApprovalsChange, seen: bool) -> Out {
        let mut out = Out::default();
        match change {
            // A reader made with `knowing` never yields it; a whole set is reconciled as one.
            ApprovalsChange::Open(set) => {
                let now: BTreeSet<String> =
                    set.iter().map(|a| a.approval_request_id.clone()).collect();
                let gone: Vec<String> = self
                    .notified
                    .iter()
                    .filter(|id| !now.contains(*id) && !self.shown_by_live_call(id))
                    .cloned()
                    .collect();
                out.merge(self.withdrawn(gone));
                for a in set {
                    out.merge(self.added(a, seen));
                }
            }
            ApprovalsChange::Added(a) => out.merge(self.added(a, seen)),
            ApprovalsChange::Withdrawn(ids) => {
                for id in &ids {
                    // The feed says it waits no more: whatever a call showed of it goes too.
                    self.gone.remove(id);
                    self.shown.retain(|s| &s.request.approval_request_id != id);
                }
                out.merge(self.withdrawn(ids));
            }
            _ => {}
        }
        out.cards = true;
        out
    }

    fn added(&mut self, a: ApprovalRequested, seen: bool) -> Out {
        let id = &a.approval_request_id;
        if seen
            || self.gone.contains(id)
            || self.dismissed.contains(id)
            || self.notified.contains(id)
            || self.verdicts.contains_key(id)
        {
            return Out::default();
        }
        self.notified.insert(id.clone());
        Out {
            updates: vec![Update::Notify {
                notification: self.raise(&a),
            }],
            ..Default::default()
        }
    }

    /// The notification of `a`, naming the verdicts of its reply that wait for it.
    fn raise(&self, a: &ApprovalRequested) -> NotificationView {
        let waiting: Vec<String> = self
            .reply_of(a)
            .iter()
            .filter(|o| self.verdicts.contains_key(&o.approval_request_id))
            .map(|o| format!("{}__{}", o.server_label, o.name))
            .collect();
        notification(a, &waiting)
    }

    fn withdrawn(&mut self, ids: Vec<String>) -> Out {
        let mut out = Out::default();
        for id in ids {
            if self.notified.remove(&id) {
                out.updates.push(Update::Withdraw {
                    id: notification_id(&id),
                });
            }
            // A dismissed id stays until `caught_up` cuts the file to the open set: a call that
            // left the reach and comes back within one connection stays dismissed (§5.4).
            self.verdicts.remove(&id);
            self.sent.remove(&id);
            self.notes.remove(&id);
        }
        out
    }

    /// `caught_up` came: the dismissed ids are cut to the ones still open, what lmgw said was
    /// decided on a call is the feed's to say from here, and a verdict sent for a call that waits
    /// no more has had its `approval.decided` (in the catch-up, before this) or never gets one.
    pub fn caught_up(&mut self) -> Out {
        let open: BTreeSet<&str> = self
            .open
            .iter()
            .map(|a| a.approval_request_id.as_str())
            .collect();
        let before = self.dismissed.len();
        self.dismissed.retain(|id| open.contains(id.as_str()));
        self.given.retain(|id, _| open.contains(id.as_str()));
        let had_gone = !self.gone.is_empty();
        self.gone.clear();
        Out {
            save: self.dismissed.len() != before,
            cards: had_gone,
            ..Default::default()
        }
    }

    /// The cards left the screen: every open call not notified nor dismissed is notified.
    pub fn backgrounded(&mut self) -> Out {
        let mut out = Out::default();
        for a in self.all() {
            out.merge(self.added(a, false));
        }
        out
    }

    /// The user dismissed the notification of `id`.
    pub fn dismiss(&mut self, id: &str) -> Out {
        self.notified.remove(id);
        Out {
            save: self.dismissed.insert(id.to_owned()),
            ..Default::default()
        }
    }

    /// The notification of `id` is gone by another way (the shell says so).
    pub fn gone(&mut self, id: &str) {
        self.notified.remove(id);
    }

    /// A verdict for `id` given on the phone. `send`: every call of the reply has one now, send
    /// them (thread, `(id, approve)` each), through the running call when `via_call` names it;
    /// else held, with how many remain. While no card is seen (the verdict came from a
    /// notification), the reply's calls still waiting are raised again, dismissed or not, saying
    /// which verdicts wait for them.
    pub fn decide(&mut self, id: &str, approve: bool, seen: bool) -> Result<Decision, String> {
        let Some(call) = self.find(id) else {
            return Err(format!(
                "the call {id} waits for no decision any more (decided elsewhere, or its reply \
                 moved on)"
            ));
        };
        if self.sent.contains(id) {
            return Err(format!(
                "the verdict on {}__{} went to lmgw already and waits for its answer",
                call.server_label, call.name
            ));
        }
        self.verdicts.insert(id.to_owned(), approve);
        self.notes.remove(id);
        let reply = self.reply_of(&call);
        let waiting: Vec<ApprovalRequested> = reply
            .iter()
            .filter(|a| !self.verdicts.contains_key(&a.approval_request_id))
            .cloned()
            .collect();
        let mut updates = Vec::new();
        if self.notified.remove(id) {
            updates.push(Update::Withdraw {
                id: notification_id(id),
            });
        }
        if !waiting.is_empty() {
            let mut save = false;
            if !seen {
                for a in &waiting {
                    // The verdict given waits for this call: asked again even if it was swiped.
                    save |= self.dismissed.remove(&a.approval_request_id);
                    self.notified.insert(a.approval_request_id.clone());
                    updates.push(Update::Notify {
                        notification: self.raise(a),
                    });
                }
            }
            return Ok(Decision {
                thread_id: call.thread_id,
                send: None,
                via_call: None,
                remaining: u32::try_from(waiting.len()).unwrap_or(u32::MAX),
                updates,
                save,
            });
        }
        let send: Vec<(String, bool)> = reply
            .iter()
            .map(|a| {
                (
                    a.approval_request_id.clone(),
                    self.verdicts[&a.approval_request_id],
                )
            })
            .collect();
        // Through the call only when its session showed every call of the reply; the route takes
        // any.
        let via_call = self.reply_call(&reply);
        for a in &reply {
            let id = &a.approval_request_id;
            self.sent.insert(id.clone());
            self.notes.remove(id);
            self.given.insert(
                id.clone(),
                Given {
                    approve: self.verdicts[id],
                    tool: format!("{}__{}", a.server_label, a.name),
                },
            );
        }
        Ok(Decision {
            thread_id: call.thread_id,
            send: Some(send),
            via_call,
            remaining: 0,
            updates,
            save: false,
        })
    }

    /// The verdicts for `ids` were refused or failed: they are no longer held, `note` (lmgw's
    /// words) goes on each card, and each call still open is asked again (notified while no card
    /// is seen). `refused`: lmgw refused them before deciding anything; else it may have decided
    /// (the answer broke off, or an error that does not say), and the feed's `approval.decided`
    /// is still compared with them.
    pub fn not_taken(
        &mut self,
        ids: &[String],
        note: Option<String>,
        refused: bool,
        seen: bool,
    ) -> Out {
        let mut out = Out {
            cards: true,
            ..Default::default()
        };
        for id in ids {
            self.verdicts.remove(id);
            self.sent.remove(id);
            if refused {
                self.given.remove(id);
            }
            if let Some(a) = self.find(id) {
                if let Some(n) = &note {
                    self.notes.insert(id.clone(), n.clone());
                }
                out.merge(self.added(a, seen));
            }
        }
        out
    }

    /// Every notification shown, withdrawn (Forget gateway, another gateway).
    pub fn clear(&mut self) -> Vec<Update> {
        let ups = std::mem::take(&mut self.notified)
            .into_iter()
            .map(|id| Update::Withdraw {
                id: notification_id(&id),
            })
            .collect();
        self.open.clear();
        self.shown.clear();
        self.verdicts.clear();
        self.sent.clear();
        self.notes.clear();
        self.gone.clear();
        self.given.clear();
        self.dismissed.clear();
        ups
    }

    /// The feed's `approval.decided` for a call: when lmgw decided otherwise than the verdict the
    /// phone sent (module doc), the words that tell the owner, once. `own_by`: how the feed names
    /// this phone (`device '<name>'`), whose own new message declines what waits.
    pub fn feed_decided(&mut self, d: &ApprovalDecided, own_by: Option<&str>) -> Option<String> {
        let g = self.given.get(&d.approval_request_id)?;
        let words = decided_words(g, d, own_by)?;
        self.given.remove(&d.approval_request_id);
        Some(words)
    }

    /// lmgw said already how `ids` were decided (the route's 409 `approval_decided`, its words in
    /// a notice): the feed's `approval.decided` tells nothing more.
    pub fn told(&mut self, ids: &[String]) {
        for id in ids {
            self.given.remove(id);
        }
    }
}

/// What the owner is told when lmgw decided otherwise than the verdict sent (`g`); `None` when it
/// decided as sent.
fn decided_words(g: &Given, d: &ApprovalDecided, own_by: Option<&str>) -> Option<String> {
    let tool = &g.tool;
    if d.not_run {
        return Some(format!(
            "{tool} was approved, but it never ran: lmgw could not start the turn that was to run it."
        ));
    }
    if d.approve == g.approve {
        return None;
    }
    let mine = d.by.is_some() && d.by.as_deref() == own_by;
    Some(match (g.approve, mine, d.by.as_deref()) {
        (true, true, _) => format!(
            "{tool} did not run: what you said or typed reached lmgw before your approval, and a \
             new message declines what waits. Ask Kai again if you still want it."
        ),
        (true, false, Some(by)) => {
            format!("{tool} did not run: {by} declined it before your approval reached lmgw.")
        }
        (true, false, None) => {
            format!("{tool} did not run: it was declined before your approval reached lmgw.")
        }
        (false, _, Some(by)) => {
            format!("{tool} was approved by {by} before your decline reached lmgw.")
        }
        (false, _, None) => format!("{tool} was approved before your decline reached lmgw."),
    })
}

/// Whether lmgw's refusal code (an `error`'s on the call, the route's) says it refused the
/// verdicts before deciding anything: an `approval_*` code but `approval_decided` (decided
/// already, by someone), or the key's rate. Any other code may come after the decision was written
/// (the resumed turn's failure: the model, an upstream timeout, `gpu_hold`).
pub fn refused_before_deciding(code: Option<&str>) -> bool {
    code.is_some_and(|c| (c.starts_with("approval_") && c != "approval_decided") || c == "key_rate")
}

/// [`Approvals::decide`]'s answer.
#[derive(Debug)]
pub struct Decision {
    pub thread_id: i64,
    /// The reply's verdicts, when every call has one.
    pub send: Option<Vec<(String, bool)>>,
    /// The running call whose session showed every call of the reply: the verdicts go through it.
    pub via_call: Option<u64>,
    pub remaining: u32,
    pub updates: Vec<Update>,
    /// `approvals.json` changed (a waiting call raised again was dismissed).
    pub save: bool,
}

pub fn notification_id(approval_request_id: &str) -> NotificationId {
    NotificationId {
        kind: NotificationKind::Approval,
        key: approval_request_id.to_owned(),
    }
}

/// The notification of a call, built from the event alone (§11): "Kai wants to run
/// desktop__open_file (path: …) in '<thread_title>'"; `waiting` names the tools of the reply's
/// calls whose verdicts the phone holds until this one has one.
pub fn notification(a: &ApprovalRequested, waiting: &[String]) -> NotificationView {
    let args = compact_arguments(&a.arguments);
    let tool = format!("{}__{}", a.server_label, a.name);
    let mut text = match waiting {
        [] => String::new(),
        [one] => format!("Your verdict on {one} waits for this one.\n"),
        many => format!("Your verdicts on {} wait for this one.\n", many.join(", ")),
    };
    if !args.is_empty() {
        text.push_str(&format!("{args}\n"));
    }
    text.push_str(&format!("in '{}'", a.thread_title));
    NotificationView {
        id: notification_id(&a.approval_request_id),
        title: format!("Kai wants to run {tool}"),
        text,
        thread_id: Some(a.thread_id),
        approval: Some(ApprovalNote {
            thread_id: a.thread_id,
            approval_request_id: a.approval_request_id.clone(),
        }),
        count: 1,
    }
}

/// A call's arguments (a JSON string) in compact lines (kai K22's form): `key: value` per
/// top-level key, strings as they are, anything else as compact JSON; arguments that are no JSON
/// object, as written.
pub fn compact_arguments(arguments: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(arguments) {
        Ok(serde_json::Value::Object(map)) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) => format!("{k}: {s}"),
                other => format!("{k}: {other}"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => arguments.trim().to_owned(),
    }
}

/// The words a card shows when lmgw did not take its verdict (`code`, lmgw's `message`): what the
/// owner can do, then lmgw's sentence. `approval_out_of_scope` is a reach the phone lacks (its
/// device policy's tool scope or admin level): declining stays open to it. A code that does not
/// say lmgw refused before deciding ([`refused_before_deciding`]) says that it may have decided.
pub fn refusal_note(code: Option<&str>, message: &str) -> String {
    match code {
        Some("approval_out_of_scope") => format!(
            "lmgw does not let this phone approve it ({message}). Decline it here, or approve it \
             on lmgw's dashboard."
        ),
        Some(c) if refused_before_deciding(code) => {
            format!("lmgw did not take the verdict: {message} ({c})")
        }
        Some(c) => format!("The decision may not have reached lmgw: {message} ({c})"),
        None => format!("The decision may not have reached lmgw: {message}"),
    }
}

#[cfg(test)]
mod tests;
