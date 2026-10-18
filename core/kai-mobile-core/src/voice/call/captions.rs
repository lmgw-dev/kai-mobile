//! A call's captions (design §7.6): the owner's words from the transcript (`TranscriptDelta`, then
//! `Transcript`), Kai's from `SpokenDelta` per response; a `lmgw.chat.reply` replaces Kai's line with
//! what the thread kept (a cut's heard part). Every line of the call is kept, and each change goes
//! to Kotlin as the line whole, by id.

use std::collections::BTreeSet;

use crate::voice::view::{CaptionLine, Speaker};

#[derive(Debug, Clone)]
struct Line {
    view: CaptionLine,
    /// Kai's line: the response it speaks, and the message the thread keeps it as.
    response: Option<String>,
    message: Option<i64>,
    /// "answered by <alias> (fallback)".
    fallback: Option<String>,
    /// How the thread kept the reply, when not whole.
    cut: Option<&'static str>,
}

/// The lines of one call (module doc).
#[derive(Debug, Clone)]
pub struct Captions {
    call: u64,
    lines: Vec<Line>,
    /// The owner's line the transcript still writes.
    owner_open: Option<usize>,
    changed: BTreeSet<usize>,
}

impl Captions {
    pub fn new(call: u64) -> Self {
        Self {
            call,
            lines: Vec::new(),
            owner_open: None,
            changed: BTreeSet::new(),
        }
    }

    fn push(&mut self, speaker: Speaker, response: Option<String>) -> usize {
        let i = self.lines.len();
        self.lines.push(Line {
            view: CaptionLine {
                call: self.call,
                id: i as u64 + 1,
                speaker,
                text: String::new(),
                done: false,
                note: None,
            },
            response,
            message: None,
            fallback: None,
            cut: None,
        });
        i
    }

    /// The line's note from its fallback and its cut.
    fn renote(&mut self, i: usize) {
        let l = &mut self.lines[i];
        let parts: Vec<&str> = l.fallback.as_deref().into_iter().chain(l.cut).collect();
        l.view.note = (!parts.is_empty()).then(|| parts.join(" · "));
    }

    /// `conversation.item.input_audio_transcription.delta`.
    pub fn owner_delta(&mut self, delta: &str) {
        let i = match self.owner_open {
            Some(i) => i,
            None => {
                let i = self.push(Speaker::Owner, None);
                self.owner_open = Some(i);
                i
            }
        };
        self.lines[i].view.text.push_str(delta);
        self.changed.insert(i);
    }

    /// `….completed`: the owner's words, final.
    pub fn owner_final(&mut self, text: &str) {
        let i = match self.owner_open.take() {
            Some(i) => i,
            None => self.push(Speaker::Owner, None),
        };
        let line = &mut self.lines[i].view;
        line.text = text.to_owned();
        line.done = true;
        self.changed.insert(i);
    }

    /// `response.output_audio_transcript.delta`.
    pub fn kai_delta(&mut self, response: &str, delta: &str) {
        let i = match self
            .lines
            .iter()
            .rposition(|l| l.response.as_deref() == Some(response))
        {
            Some(i) => i,
            None => self.push(Speaker::Kai, Some(response.to_owned())),
        };
        self.lines[i].view.text.push_str(delta);
        self.changed.insert(i);
    }

    /// The line message `message_id` is kept as: the one it was matched with, else Kai's latest
    /// line not matched yet.
    fn kai_line_of(&mut self, message_id: i64) -> Option<usize> {
        let i = self
            .lines
            .iter()
            .position(|l| l.message == Some(message_id))
            .or_else(|| {
                self.lines
                    .iter()
                    .rposition(|l| l.view.speaker == Speaker::Kai && l.message.is_none())
            })?;
        self.lines[i].message = Some(message_id);
        Some(i)
    }

    /// `lmgw.chat.reply`: the reply as the thread keeps it.
    pub fn kai_reply(
        &mut self,
        message_id: i64,
        content: Option<&str>,
        unheard: Option<&str>,
        removed: bool,
    ) {
        let i = match self.kai_line_of(message_id) {
            Some(i) => i,
            None => {
                let i = self.push(Speaker::Kai, None);
                self.lines[i].message = Some(message_id);
                i
            }
        };
        let line = &mut self.lines[i].view;
        if let Some(c) = content {
            line.text = c.to_owned();
        }
        line.done = true;
        let cut = if removed {
            Some("not heard: gone from the thread")
        } else if unheard.is_some_and(|u| !u.trim().is_empty()) {
            Some("cut where it was heard")
        } else {
            None
        };
        self.lines[i].cut = cut;
        self.renote(i);
        self.changed.insert(i);
    }

    /// `lmgw.response.timing`: matches the response's line with its message, and names a fallback
    /// that answered (ruling 10: shown, never refused).
    pub fn timing(
        &mut self,
        response: Option<&str>,
        message: Option<i64>,
        answered_by: Option<&str>,
    ) {
        let Some(i) = response.and_then(|r| {
            self.lines
                .iter()
                .rposition(|l| l.response.as_deref() == Some(r))
        }) else {
            return;
        };
        if self.lines[i].message.is_none() {
            self.lines[i].message = message;
        }
        if let Some(a) = answered_by {
            self.lines[i].fallback = Some(format!("answered by {a} (fallback)"));
            self.renote(i);
            self.changed.insert(i);
        }
    }

    /// A line of lmgw's word on the call (an error, a broken part set).
    pub fn note(&mut self, text: String) {
        let i = self.push(Speaker::Note, None);
        self.lines[i].view.text = text;
        self.lines[i].view.done = true;
        self.changed.insert(i);
    }

    /// The lines changed since the last call, in order.
    pub fn take_changed(&mut self) -> Vec<CaptionLine> {
        std::mem::take(&mut self.changed)
            .into_iter()
            .map(|i| self.lines[i].view.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_follow_the_transcript_the_speech_and_the_reply_kept() {
        let mut c = Captions::new(3);
        c.owner_delta("what time");
        c.owner_delta(" is it");
        let ch = c.take_changed();
        assert_eq!(ch.len(), 1);
        assert_eq!((ch[0].call, ch[0].id), (3, 1));
        assert_eq!(ch[0].text, "what time is it");
        assert!(!ch[0].done);
        c.owner_final("What time is it?");
        c.kai_delta("resp_1", "It is ");
        c.kai_delta("resp_1", "ten past nine, and");
        let ch = c.take_changed();
        assert_eq!(ch.len(), 2);
        assert!(ch[0].done && ch[0].text == "What time is it?");
        assert_eq!(ch[1].speaker, Speaker::Kai);
        assert_eq!(ch[1].text, "It is ten past nine, and");
        // Cut by a barge-in: the thread keeps what was heard.
        c.timing(Some("resp_1"), Some(41), Some("cloud"));
        c.kai_reply(41, Some("It is ten past"), Some("nine, and"), false);
        let ch = c.take_changed();
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].text, "It is ten past");
        assert!(ch[0].done);
        assert_eq!(
            ch[0].note.as_deref(),
            Some("answered by cloud (fallback) · cut where it was heard")
        );
        // The next turn opens new lines.
        c.owner_delta("thanks");
        assert_eq!(c.take_changed()[0].id, 3);
        // A reply heard by nobody.
        c.kai_delta("resp_2", "You're");
        c.kai_reply(42, None, Some("You're welcome"), true);
        let ch = c.take_changed();
        let last = ch.last().unwrap();
        assert_eq!(last.text, "You're");
        assert_eq!(
            last.note.as_deref(),
            Some("not heard: gone from the thread")
        );
        c.note("the parts of event evt_1 cannot be joined".into());
        assert_eq!(c.take_changed()[0].speaker, Speaker::Note);
    }
}
