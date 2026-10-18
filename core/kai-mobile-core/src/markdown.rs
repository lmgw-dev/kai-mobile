//! A message's Markdown (design M14, §6.2), parsed in the core with pulldown-cmark into a flat list
//! of blocks of styled spans, which Compose renders with composables of its own (and iOS later
//! with its own look). Flat, because UniFFI carries no recursive types: a block names its list
//! depth and its quote depth instead of holding children. Paragraphs, headings, list items, quotes,
//! code blocks and rules; a table as its source text (Later: rendered as a table); inline text,
//! emphasis, strong, strikethrough, code and links. Images show as their alternative text; raw
//! HTML as its text.
//!
//! **Links are model output, so untrusted.** Only `http`, `https` and `mailto` targets become
//! links (opened by the browser or the mail app on a tap); any other (`file:`, `content:`,
//! `intent:`, `javascript:`, `lmgw-pair:`, a relative path) is plain text with its target shown
//! after it, so a tap can neither reach another app's content or the pairing screen nor crash the
//! process that holds the feed.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// One block (module doc).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MdBlock {
    pub kind: MdBlockKind,
    /// How deep in lists the block sits (0: in none).
    pub depth: u32,
    /// How deep in block quotes (0: in none).
    pub quote: u32,
    /// The block's text as styled spans (empty for a rule and a code block).
    pub spans: Vec<MdSpan>,
    /// A code block's or a table's text, as written.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MdBlockKind {
    Paragraph,
    Heading {
        level: u8,
    },
    /// A list item's first paragraph; `marker` is `•` or the item's number with its dot.
    ListItem {
        marker: String,
    },
    CodeBlock {
        lang: Option<String>,
    },
    Rule,
    /// A table, kept as its source text.
    Table,
}

/// A run of text with one style.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct MdSpan {
    pub text: String,
    pub emphasis: bool,
    pub strong: bool,
    pub strike: bool,
    pub code: bool,
    /// The link's target, for a link's text: an `http`, `https` or `mailto` URL only (module doc).
    pub link: Option<String>,
}

/// An open link: kept, or refused for its scheme (shown as text, its target after it).
enum Link {
    Kept(String),
    Refused { target: String, text: String },
}

#[derive(Default)]
struct Style {
    emphasis: u32,
    strong: u32,
    strike: u32,
    link: Vec<Link>,
}

/// Whether `target` may be a link (module doc): `http://`, `https://` or `mailto:`, by scheme,
/// case-insensitively.
pub fn link_allowed(target: &str) -> bool {
    let Some((scheme, rest)) = target.split_once(':') else {
        return false;
    };
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => rest.starts_with("//") && rest.len() > 2,
        "mailto" => !rest.is_empty(),
        _ => false,
    }
}

struct Builder {
    blocks: Vec<MdBlock>,
    current: Option<MdBlock>,
    style: Style,
    depth: u32,
    quote: u32,
    /// Each open list's next number (`None`: a bullet list).
    lists: Vec<Option<u64>>,
    /// The marker the next paragraph of an item takes (its first only).
    pending_marker: Option<String>,
}

impl Builder {
    fn open(&mut self, kind: MdBlockKind) {
        self.close();
        let kind = match (kind, self.pending_marker.take()) {
            (MdBlockKind::Paragraph, Some(marker)) => MdBlockKind::ListItem { marker },
            (k, _) => k,
        };
        self.current = Some(MdBlock {
            kind,
            depth: self.depth,
            quote: self.quote,
            spans: Vec::new(),
            text: String::new(),
        });
    }

    fn close(&mut self) {
        if let Some(b) = self.current.take() {
            let empty = b.spans.iter().all(|s| s.text.is_empty()) && b.text.is_empty();
            if !empty || matches!(b.kind, MdBlockKind::Rule | MdBlockKind::CodeBlock { .. }) {
                self.blocks.push(b);
            }
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        if self.current.is_none() {
            // Text outside a paragraph (a tight list item's): an item's paragraph of its own.
            self.open(MdBlockKind::Paragraph);
        }
        let link = match self.style.link.last_mut() {
            Some(Link::Kept(target)) => Some(target.clone()),
            Some(Link::Refused { text: shown, .. }) => {
                shown.push_str(text);
                None
            }
            None => None,
        };
        let span = MdSpan {
            text: text.to_owned(),
            emphasis: self.style.emphasis > 0,
            strong: self.style.strong > 0,
            strike: self.style.strike > 0,
            code,
            link,
        };
        let b = self.current.as_mut().expect("opened above");
        match b.spans.last_mut() {
            Some(last)
                if last.emphasis == span.emphasis
                    && last.strong == span.strong
                    && last.strike == span.strike
                    && last.code == span.code
                    && last.link == span.link =>
            {
                last.text.push_str(&span.text)
            }
            _ => b.spans.push(span),
        }
    }
}

/// Parses `source` (module doc).
pub fn parse(source: &str) -> Vec<MdBlock> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut b = Builder {
        blocks: Vec::new(),
        current: None,
        style: Style::default(),
        depth: 0,
        quote: 0,
        lists: Vec::new(),
        pending_marker: None,
    };
    let mut table_depth = 0u32;
    for (event, range) in Parser::new_ext(source, opts).into_offset_iter() {
        if table_depth > 0 {
            match event {
                Event::Start(Tag::Table(_)) => table_depth += 1,
                Event::End(TagEnd::Table) => table_depth -= 1,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(Tag::Paragraph) => b.open(MdBlockKind::Paragraph),
            Event::End(TagEnd::Paragraph) => b.close(),
            Event::Start(Tag::Heading { level, .. }) => b.open(MdBlockKind::Heading {
                level: heading_level(level),
            }),
            Event::End(TagEnd::Heading(_)) => b.close(),
            Event::Start(Tag::BlockQuote(_)) => {
                b.close();
                b.quote += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                b.close();
                b.quote = b.quote.saturating_sub(1);
            }
            Event::Start(Tag::List(start)) => {
                b.close();
                b.lists.push(start);
                b.depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                b.close();
                b.lists.pop();
                b.depth = b.depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                b.close();
                let marker = match b.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}.");
                        *n += 1;
                        m
                    }
                    _ => "•".to_owned(),
                };
                b.pending_marker = Some(marker);
            }
            Event::End(TagEnd::Item) => {
                b.close();
                b.pending_marker = None;
            }
            Event::TaskListMarker(done) => {
                b.pending_marker = Some(if done { "☑" } else { "☐" }.to_owned());
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) if !l.trim().is_empty() => Some(l.trim().to_owned()),
                    _ => None,
                };
                b.pending_marker = None;
                b.open(MdBlockKind::CodeBlock { lang });
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(cur) = b.current.as_mut()
                    && cur.text.ends_with('\n')
                {
                    cur.text.pop();
                }
                b.close();
            }
            Event::Start(Tag::Table(_)) => {
                b.close();
                table_depth = 1;
                b.blocks.push(MdBlock {
                    kind: MdBlockKind::Table,
                    depth: b.depth,
                    quote: b.quote,
                    spans: Vec::new(),
                    text: source[range].trim_end().to_owned(),
                });
            }
            Event::Rule => {
                b.open(MdBlockKind::Rule);
                b.close();
            }
            Event::Start(Tag::Emphasis) => b.style.emphasis += 1,
            Event::End(TagEnd::Emphasis) => b.style.emphasis = b.style.emphasis.saturating_sub(1),
            Event::Start(Tag::Strong) => b.style.strong += 1,
            Event::End(TagEnd::Strong) => b.style.strong = b.style.strong.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => b.style.strike += 1,
            Event::End(TagEnd::Strikethrough) => b.style.strike = b.style.strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => {
                let target = dest_url.to_string();
                b.style.link.push(if link_allowed(&target) {
                    Link::Kept(target)
                } else {
                    Link::Refused {
                        target,
                        text: String::new(),
                    }
                });
            }
            Event::End(TagEnd::Link) => {
                if let Some(Link::Refused { target, text }) = b.style.link.pop()
                    && text.trim() != target
                    && !target.is_empty()
                {
                    // Not a link: its target shown as text after it (an autolink shows it already).
                    b.text(&format!(" ({target})"), false);
                }
            }
            Event::Text(t) => {
                if let Some(cur) = b.current.as_mut()
                    && matches!(cur.kind, MdBlockKind::CodeBlock { .. })
                {
                    cur.text.push_str(&t);
                } else {
                    b.text(&t, false);
                }
            }
            Event::Code(t) => b.text(&t, true),
            Event::SoftBreak => b.text(" ", false),
            Event::HardBreak => b.text("\n", false),
            Event::Html(t) | Event::InlineHtml(t) => b.text(&t, false),
            Event::InlineMath(t) | Event::DisplayMath(t) => b.text(&t, true),
            Event::FootnoteReference(t) => b.text(&format!("[{t}]"), false),
            // An image's text is its alternative text, which arrives as Text inside it.
            _ => {}
        }
    }
    b.close();
    b.blocks
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(blocks: &[MdBlock]) -> Vec<&MdBlockKind> {
        blocks.iter().map(|b| &b.kind).collect()
    }

    #[test]
    fn paragraphs_headings_and_inline_styles() {
        let b = parse("# Title\n\nSome *em* and **strong** and `code` and [a link](https://x.y).");
        assert_eq!(
            kinds(&b),
            [&MdBlockKind::Heading { level: 1 }, &MdBlockKind::Paragraph]
        );
        let spans = &b[1].spans;
        assert!(spans.iter().any(|s| s.text == "em" && s.emphasis));
        assert!(spans.iter().any(|s| s.text == "strong" && s.strong));
        assert!(spans.iter().any(|s| s.text == "code" && s.code));
        assert!(
            spans
                .iter()
                .any(|s| s.text == "a link" && s.link.as_deref() == Some("https://x.y"))
        );
    }

    #[test]
    fn lists_nest_by_depth_and_number_their_items() {
        let b = parse("1. one\n2. two\n   - inner\n\n> quoted\n");
        assert_eq!(
            b[0].kind,
            MdBlockKind::ListItem {
                marker: "1.".into()
            }
        );
        assert_eq!(
            b[1].kind,
            MdBlockKind::ListItem {
                marker: "2.".into()
            }
        );
        assert_eq!(
            b[2].kind,
            MdBlockKind::ListItem {
                marker: "•".into()
            }
        );
        assert_eq!((b[0].depth, b[2].depth), (1, 2));
        assert_eq!(b[3].quote, 1);
        assert_eq!(b[3].spans[0].text, "quoted");
    }

    #[test]
    fn code_blocks_rules_and_tables_keep_their_text() {
        let b = parse("```rust\nfn main() {}\n```\n\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert_eq!(
            b[0].kind,
            MdBlockKind::CodeBlock {
                lang: Some("rust".into())
            }
        );
        assert_eq!(b[0].text, "fn main() {}");
        assert_eq!(b[1].kind, MdBlockKind::Rule);
        assert_eq!(b[2].kind, MdBlockKind::Table);
        assert!(b[2].text.starts_with("| a | b |") && b[2].text.ends_with("| 1 | 2 |"));
    }

    #[test]
    fn only_http_https_and_mailto_targets_are_links() {
        let b = parse(
            "[web](https://x.y) [MAIL](MAILTO:a@b.c) [f](file:///data/data/x) \
             [pair](lmgw-pair:?v=2&token=t) [c](content://sms/inbox) [js](javascript:alert(1)) \
             [rel](/etc) <file:///etc/hosts>",
        );
        let spans = &b[0].spans;
        let links: Vec<&str> = spans.iter().filter_map(|s| s.link.as_deref()).collect();
        assert_eq!(links, ["https://x.y", "MAILTO:a@b.c"]);
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        for shown in [
            "f (file:///data/data/x)",
            "pair (lmgw-pair:?v=2&token=t)",
            "c (content://sms/inbox)",
            "js (javascript:alert(1))",
            "rel (/etc)",
        ] {
            assert!(text.contains(shown), "{shown} in {text}");
        }
        assert!(
            text.ends_with(" file:///etc/hosts"),
            "an autolink once: {text}"
        );
        assert!(!link_allowed("https:evil"));
        assert!(!link_allowed("intent://x#Intent;end"));
        assert!(link_allowed("http://198.51.100.20:8001/"));
    }

    #[test]
    fn plain_text_is_one_paragraph() {
        let b = parse("It is late.");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].spans[0].text, "It is late.");
        assert!(parse("").is_empty());
    }
}
