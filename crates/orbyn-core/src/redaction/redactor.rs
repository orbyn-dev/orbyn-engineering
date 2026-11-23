//! The compiled detectors and what they do to text. Crew's set is fixed; the other surfaces
//! build theirs from the settings (see `settings`).

use std::collections::BTreeSet;
use std::sync::OnceLock;

use orbyn_contracts::PatternScope;
use regex::Regex;
use serde_json::Value;

use super::builtin::BUILTINS;
use super::custom;

/// The class a person's own pattern reports.
const CUSTOM_CLASS: &str = "custom pattern";

struct Detector {
    label: String,
    class: &'static str,
    regex: Regex,
    /// Looks only at the value of `NAME=value` assignments.
    env_only: bool,
}

/// A stretch of text to hide, in bytes, and the detector that found it (its label is `label`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    detector: usize,
}

/// One person's pattern, ready to compile.
pub struct Custom<'a> {
    pub name: &'a str,
    pub regex: Regex,
    pub applies_to: PatternScope,
}

#[derive(Default)]
pub struct Redactor {
    detectors: Vec<Detector>,
}

/// The start of what `Redactor::labelled` writes in place of a hidden value.
const MARKER: &str = "[hidden: ";

/// `text` cut to at most `max` characters, without leaving a piece of a value behind. A cut that
/// lands inside a `[hidden: ...]` marker drops the marker; one that lands inside a run of
/// non-space characters drops that run's head, because a pattern that no longer matches the
/// shortened run would show the front of a secret. Hide values first, then cut.
pub fn cut_safely(text: &str, max: usize) -> String {
    let Some((at, _)) = text.char_indices().nth(max) else {
        return text.to_owned();
    };
    let mut head = &text[..at];
    if let Some(open) = head
        .rfind(MARKER)
        .filter(|open| !head[*open..].contains(']'))
    {
        head = &head[..open];
    }
    let inside_run = !head.ends_with(char::is_whitespace)
        && text[head.len()..].starts_with(|c: char| !c.is_whitespace());
    if inside_run {
        head = &head[..head.rfind(char::is_whitespace).map_or(0, |at| {
            at + head[at..].chars().next().map_or(0, char::len_utf8)
        })];
    }
    head.trim_end().to_owned()
}

fn assignments() -> &'static Regex {
    static VALUES: OnceLock<Regex> = OnceLock::new();
    VALUES.get_or_init(|| {
        Regex::new(r#"\b[A-Za-z_][A-Za-z0-9_]*=("[^"\n]*"|'[^'\n]*'|\S+)"#)
            .expect("a fixed pattern")
    })
}

impl Redactor {
    /// Crew's set: every recognised format, whatever the settings say. The ledger and every Crew
    /// object use this and nothing else, so no setting can let a secret into them.
    pub fn locked() -> &'static Redactor {
        static LOCKED: OnceLock<Redactor> = OnceLock::new();
        LOCKED.get_or_init(|| Redactor::with(&BTreeSet::new(), Vec::new()))
    }

    /// The recognised formats not in `off`, then the person's patterns.
    pub fn with(off: &BTreeSet<String>, patterns: Vec<Custom<'_>>) -> Redactor {
        let mut detectors = Vec::new();
        for builtin in BUILTINS.iter().filter(|b| !off.contains(b.id)) {
            for source in builtin.patterns {
                detectors.push(Detector {
                    label: builtin.label.to_owned(),
                    class: builtin.class,
                    regex: Regex::new(source).expect("a fixed pattern"),
                    env_only: false,
                });
            }
        }
        for pattern in patterns {
            detectors.push(Detector {
                label: pattern.name.to_owned(),
                class: CUSTOM_CLASS,
                regex: pattern.regex,
                env_only: pattern.applies_to == PatternScope::EnvironmentValues,
            });
        }
        Redactor { detectors }
    }

    /// Only the given patterns, no recognised format: to try one pattern on its own.
    pub fn only(patterns: Vec<Custom<'_>>) -> Redactor {
        let every: BTreeSet<String> = BUILTINS.iter().map(|b| b.id.to_owned()).collect();
        Redactor::with(&every, patterns)
    }

    /// Nothing is hidden by this set.
    pub fn is_empty(&self) -> bool {
        self.detectors.is_empty()
    }

    /// The class of the first detector that matches `text`, if any. Never the match.
    pub fn class_of(&self, text: &str) -> Option<&'static str> {
        self.detectors
            .iter()
            .filter(|detector| !detector.env_only)
            .find(|detector| detector.regex.is_match(text))
            .map(|detector| detector.class)
    }

    /// The name of what matched.
    pub fn label(&self, span: &Span) -> &str {
        &self.detectors[span.detector].label
    }

    /// Looks for matches that start in `start..end` of `text`, reading `text[..end]` so what lies
    /// before the window is seen (a `\b` at its first byte is judged against the byte before it).
    /// `touch` takes the earliest start of a match that runs to `end`: it may go on in the next window.
    fn scan(
        &self,
        text: &str,
        start: usize,
        end: usize,
        more: bool,
        found: &mut Vec<Span>,
        touch: &mut Option<usize>,
    ) {
        let hay = &text[..end];
        let mut keep = |index: usize, from: usize, to: usize| {
            if to > from {
                found.push(Span {
                    start: from,
                    end: to,
                    detector: index,
                });
                if more && to == end {
                    *touch = Some(touch.map_or(from, |at| at.min(from)));
                }
            }
        };
        for (index, detector) in self.detectors.iter().enumerate() {
            if detector.env_only {
                // An assignment sits on one line: read from the line that holds `start`.
                let line = hay[..start].rfind('\n').map_or(0, |at| at + 1);
                for values in assignments().captures_iter(&hay[line..]) {
                    let Some(value) = values.get(1) else { continue };
                    let base = line + value.start();
                    for hit in detector.regex.find_iter(value.as_str()) {
                        keep(index, base + hit.start(), base + hit.end());
                    }
                }
            } else {
                let mut at = start;
                while at <= hay.len() {
                    let Some(hit) = detector.regex.find_at(hay, at) else {
                        break;
                    };
                    keep(index, hit.start(), hit.end());
                    at = if hit.end() > hit.start() {
                        hit.end()
                    } else {
                        hay[hit.end()..]
                            .chars()
                            .next()
                            .map_or(hay.len() + 1, |c| hit.end() + c.len_utf8())
                    };
                }
            }
        }
    }

    /// The stretches to hide: empty matches dropped, overlaps merged under the first detector that
    /// found them.
    ///
    /// Text over `MAX_APPLY_BYTES` is read in windows of that size, each seeing the text before it.
    /// The next window starts `WINDOW_OVERLAP` (64 KiB) before the end of the last, or at the start
    /// of a match that ran to the end, whichever is earlier. So a match is found whole when it is at
    /// most 64 KiB long, or when it is one run (`{20,}`, `\S*`) that carries on from where it began.
    /// A match that needs both ends of a block longer than 64 KiB is not found: the built-in private
    /// key pattern reads at most 8 KiB between its markers for that reason.
    pub fn spans(&self, text: &str) -> Vec<Span> {
        let mut found: Vec<Span> = Vec::new();
        let mut start = 0;
        loop {
            let mut end = (start + custom::MAX_APPLY_BYTES).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let more = end < text.len();
            let mut touch = None;
            self.scan(text, start, end, more, &mut found, &mut touch);
            if !more {
                break;
            }
            let mut next = touch.unwrap_or(end).min(end - custom::WINDOW_OVERLAP);
            while !text.is_char_boundary(next) {
                next += 1;
            }
            // A match as long as the window itself is cut here; its rest is found from this point on.
            start = if next > start { next } else { end };
        }
        found.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
        let mut merged: Vec<Span> = Vec::new();
        for span in found {
            match merged.last_mut() {
                Some(last) if span.start < last.end => last.end = last.end.max(span.end),
                _ => merged.push(span),
            }
        }
        merged
    }

    fn replace_spans(
        &self,
        text: &str,
        mut mask: impl FnMut(&Self, &Span, &str) -> String,
    ) -> String {
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for span in self.spans(text) {
            out.push_str(&text[at..span.start]);
            out.push_str(&mask(self, &span, &text[span.start..span.end]));
            at = span.end;
        }
        out.push_str(&text[at..]);
        out
    }

    /// What an agent reads: each hidden value becomes `[hidden: <label>]`.
    pub fn labelled(&self, text: &str) -> String {
        self.replace_spans(text, |me, span, _| format!("[hidden: {}]", me.label(span)))
    }

    /// What a terminal block shows: one dot per hidden character.
    pub fn dotted(&self, text: &str) -> String {
        self.replace_spans(text, |_, _, hidden| "•".repeat(hidden.chars().count()))
    }

    /// A stored JSON value as a person reads it: every string value and every object key has the
    /// set's patterns hidden, so a secret carried in either is not shown. A key has no room for a
    /// marker of its own, so it becomes the same `[hidden: <label>]` text a value does. Other
    /// scalars (numbers, booleans, null) are shown as they are.
    pub fn mask_json(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.labelled(text)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.mask_json(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, item)| (self.labelled(key), self.mask_json(item)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// `text` with each format replaced by `mask`, one format after the other, as Crew has always
    /// done it. Only for the fixed set: it reads every detector as matching anywhere.
    pub fn replace_each(&self, text: &str, mask: &str) -> String {
        self.detectors
            .iter()
            .filter(|detector| !detector.env_only)
            .fold(text.to_owned(), |text, detector| {
                detector.regex.replace_all(&text, mask).into_owned()
            })
    }

    /// How many characters of `lines` this set hides, and how many there are. Lines and line
    /// lengths beyond the limits are cut, not skipped.
    pub fn chars_hidden(&self, lines: &[String]) -> (usize, usize) {
        let mut hidden = 0;
        let mut total = 0;
        for line in lines.iter().take(custom::MAX_CONTEXT_LINES) {
            let line: String = line.chars().take(custom::MAX_LINE_CHARS).collect();
            total += line.chars().count();
            hidden += self
                .spans(&line)
                .iter()
                .map(|span| line[span.start..span.end].chars().count())
                .sum::<usize>();
        }
        (hidden, total)
    }
}
