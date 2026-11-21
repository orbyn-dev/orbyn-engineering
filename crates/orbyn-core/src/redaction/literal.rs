//! Exact-match masking of known values in a byte stream, for a workflow run's terminal output
//! (docs/execution-profiles-and-library.md, "Redaction rules for environment values"). The values are known exactly, so each is
//! registered as a literal pattern of the existing [`Redactor`]; a match shows as `[hidden: env <KEY>]`.
//!
//! Output arrives in chunks, so a value can be split across two or more. The masker keeps back only the tail of what it has seen that
//! could still turn into a value (a proper prefix of one), emits everything before it masked, and joins the next chunk to what it kept.
//! A value is therefore never in what [`LiteralMasker::push`] returns, at any split. [`LiteralMasker::finish`] flushes the rest.
//!
//! Limits, stated plainly: only the exact bytes are caught. A command that transforms a value (base64, reversed, sliced, a hash)
//! prints something this does not match. Bytes that are not UTF-8 read as U+FFFD, as the terminal's saved output already reads them.

use regex::RegexBuilder;

use super::{Custom, Redactor};
use crate::error::{CoreError, Result};
use orbyn_contracts::PatternScope;

/// The most a compiled literal may take. A value is at most 8 KiB, so this is far above any real one; over it the run is refused.
const PROGRAM_LIMIT: usize = 16 * 1024 * 1024;

/// Masks known values in output that arrives in chunks.
pub struct LiteralMasker {
    redactor: Redactor,
    /// Every byte string that is masked: each value, and the same value with its line breaks as a terminal writes them.
    needles: Vec<Vec<u8>>,
    /// Text read but not yet emitted: what might still begin a value.
    held: String,
    /// The start of a character that the last chunk cut in two.
    partial: Vec<u8>,
}

impl LiteralMasker {
    /// A masker for these `(key, value)` pairs. An empty value is skipped. When a pattern cannot be compiled the call fails, and the
    /// caller must not start the run: output is never let through unmasked.
    pub fn new(values: &[(&str, &str)]) -> Result<Self> {
        let mut entries: Vec<(String, String)> = Vec::new();
        for (key, value) in values {
            if value.is_empty() {
                continue;
            }
            entries.push(((*key).to_owned(), (*value).to_owned()));
            // A terminal writes a line break as CR LF, so a multi-line value that a command prints reads that way.
            if value.contains('\n') && !value.contains("\r\n") {
                entries.push(((*key).to_owned(), value.replace('\n', "\r\n")));
            }
        }
        // Longest first, so when two values overlap the longer one is the one found.
        entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        let mut seen = std::collections::HashSet::new();
        entries.retain(|(_, value)| seen.insert(value.clone()));
        let names: Vec<String> = entries
            .iter()
            .map(|(key, _)| format!("env {key}"))
            .collect();
        let mut patterns = Vec::with_capacity(entries.len());
        for ((_, value), name) in entries.iter().zip(&names) {
            let regex = RegexBuilder::new(&regex::escape(value))
                .size_limit(PROGRAM_LIMIT)
                .build()
                .map_err(|_| {
                    CoreError::Invalid("env: a value could not be prepared for masking.".into())
                })?;
            patterns.push(Custom {
                name,
                regex,
                applies_to: PatternScope::Anywhere,
            });
        }
        Ok(Self {
            redactor: Redactor::only(patterns),
            needles: entries
                .into_iter()
                .map(|(_, value)| value.into_bytes())
                .collect(),
            held: String::new(),
            partial: Vec::new(),
        })
    }

    /// No value is registered: nothing is ever hidden or held.
    pub fn is_empty(&self) -> bool {
        self.needles.is_empty()
    }

    /// The bytes of a chunk that are safe to show. What could still become a value stays inside until the next chunk or `finish`.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.partial.extend_from_slice(chunk);
        self.held.push_str(&decode(&mut self.partial));
        self.release(false)
    }

    /// Everything still held, masked: the output has ended.
    pub fn finish(&mut self) -> Vec<u8> {
        self.held
            .push_str(&String::from_utf8_lossy(&std::mem::take(&mut self.partial)));
        self.release(true)
    }

    fn release(&mut self, last: bool) -> Vec<u8> {
        let mut cut = self.held.len();
        if !last {
            cut -= self.undecided();
            // A value that straddles the cut is kept whole for the next chunk.
            for span in self.redactor.spans(&self.held) {
                if span.start < cut && cut < span.end {
                    cut = span.start;
                }
            }
            while !self.held.is_char_boundary(cut) {
                cut -= 1;
            }
        }
        let rest = self.held.split_off(cut);
        let shown = self.redactor.labelled(&self.held);
        self.held = rest;
        shown.into_bytes()
    }

    /// How many bytes at the end of `held` are the start of some value, so a later chunk may complete it.
    fn undecided(&self) -> usize {
        let bytes = self.held.as_bytes();
        let mut longest = 0;
        for needle in &self.needles {
            let window = bytes.len().saturating_sub(needle.len() - 1);
            for at in window..bytes.len() {
                let tail = &bytes[at..];
                if tail[0] == needle[0] && needle.len() > tail.len() && needle.starts_with(tail) {
                    longest = longest.max(tail.len());
                    break;
                }
            }
        }
        longest
    }
}

/// The complete characters of `bytes` as text, leaving an unfinished character at the end in `bytes`.
fn decode(bytes: &mut Vec<u8>) -> String {
    let mut text = String::new();
    loop {
        match std::str::from_utf8(bytes) {
            Ok(valid) => {
                text.push_str(valid);
                bytes.clear();
                return text;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                text.push_str(&String::from_utf8_lossy(&bytes[..valid]));
                match error.error_len() {
                    Some(len) => {
                        text.push(char::REPLACEMENT_CHARACTER);
                        bytes.drain(..valid + len);
                    }
                    None => {
                        bytes.drain(..valid);
                        return text;
                    }
                }
            }
        }
    }
}
