//! The compiled detectors and what they do to text. Crew's set is fixed; the other surfaces
//! build theirs from the settings (see `settings`).

use std::collections::BTreeSet;
use std::sync::OnceLock;

use orbyn_contracts::PatternScope;
use regex::Regex;

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

    fn scan(&self, text: &str, base: usize, found: &mut Vec<Span>) {
        for (index, detector) in self.detectors.iter().enumerate() {
            let mut keep = |start: usize, end: usize| {
                if end > start {
                    found.push(Span {
                        start: base + start,
                        end: base + end,
                        detector: index,
                    });
                }
            };
            if detector.env_only {
                for values in assignments().captures_iter(text) {
                    let Some(value) = values.get(1) else { continue };
                    for hit in detector.regex.find_iter(value.as_str()) {
                        keep(value.start() + hit.start(), value.start() + hit.end());
                    }
                }
            } else {
                for hit in detector.regex.find_iter(text) {
                    keep(hit.start(), hit.end());
                }
            }
        }
    }

    /// The stretches to hide: empty matches dropped, overlaps merged under the first detector that
    /// found them. Text over `MAX_APPLY_BYTES` is read in overlapping windows, so one call never
    /// hands the engine more than that and a value on a window's edge is still whole in the next.
    pub fn spans(&self, text: &str) -> Vec<Span> {
        let mut found: Vec<Span> = Vec::new();
        if text.len() <= custom::MAX_APPLY_BYTES {
            self.scan(text, 0, &mut found);
        } else {
            let mut start = 0;
            while start < text.len() {
                let mut end = (start + custom::MAX_APPLY_BYTES).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                self.scan(&text[start..end], start, &mut found);
                if end == text.len() {
                    break;
                }
                let mut next = end - custom::WINDOW_OVERLAP;
                while !text.is_char_boundary(next) {
                    next += 1;
                }
                start = next;
            }
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
