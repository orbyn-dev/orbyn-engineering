//! The plain text of what a terminal printed, cut for the attention event.
//!
//! The excerpt is untrusted: a program can print escape sequences, control
//! characters, markup or a link. Everything here produces text a `pre` can draw;
//! the caps are applied after redaction, and never wrap a line.

use std::sync::LazyLock;

use regex::Regex;

use crate::redaction::cut_safely;

/// The last lines of the screen kept in an excerpt.
pub(crate) const MAX_LINES: usize = 6;
/// The longest one excerpt line may be, in characters.
pub(crate) const MAX_LINE_CHARS: usize = 200;
/// The longest whole excerpt may be, in characters.
pub(crate) const MAX_EXCERPT_CHARS: usize = 1000;
/// A byte ceiling under the event's 4 KiB budget, even when every character is
/// four bytes or needs a JSON escape.
pub(crate) const MAX_EXCERPT_BYTES: usize = 3000;
/// A secret prompt keeps only its label.
pub(crate) const MAX_LABEL_CHARS: usize = 80;
/// A terminal title is a label only.
pub(crate) const MAX_TITLE_CHARS: usize = 80;
/// An engine family id is drawn as text.
pub(crate) const MAX_ENGINE_CHARS: usize = 40;
/// How much recent raw output a quiet evaluation reads.
pub(crate) const TAIL_BYTES: usize = 64 * 1024;

// The same sequences as the web's `stripTerminalText`: OSC (to BEL or ST),
// DCS/PM/APC/SOS (to ST), CSI, then any other escape. A fixed pattern cannot fail.
static OSC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)?").expect("a fixed pattern"));
static STRING_SEQUENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b[PX^_][^\x1b]*(?:\x1b\\)?").expect("a fixed pattern"));
static CSI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("a fixed pattern"));
static OTHER_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b[ -/]*[0-~]?").expect("a fixed pattern"));

/// Terminal text with escapes gone, a carriage return showing only what it last
/// redrew, and every other control character but tab and newline removed.
pub(crate) fn plain(text: &str) -> String {
    let stripped = OSC.replace_all(text, "");
    let stripped = STRING_SEQUENCE.replace_all(&stripped, "");
    let stripped = CSI.replace_all(&stripped, "");
    let stripped = OTHER_ESCAPE.replace_all(&stripped, "");
    let mut out = String::with_capacity(stripped.len());
    for (index, line) in stripped.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        // A carriage return redraws the line: the last non-empty part is what the
        // person saw. A trailing carriage return keeps what came before it.
        let visible = if line.contains('\r') {
            line.split('\r')
                .rfind(|part| !part.is_empty())
                .unwrap_or("")
        } else {
            line
        };
        out.extend(
            visible
                .chars()
                .filter(|char| *char == '\t' || !char.is_control()),
        );
    }
    out
}

/// The last `max` bytes of a terminal's raw output, starting at a line boundary so
/// an escape sequence is not cut in half. Bytes that are not text read as U+FFFD.
pub(crate) fn tail_text(bytes: &[u8], max: usize) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut start = bytes.len().saturating_sub(max);
    if start > 0
        && let Some(newline) = bytes[start..].iter().position(|byte| *byte == b'\n')
    {
        start += newline + 1;
    }
    while start < bytes.len() && (bytes[start] & 0b1100_0000) == 0b1000_0000 {
        start += 1;
    }
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// One excerpt line cut at the 200-character cap, never wrapped.
pub(crate) fn cut_line(line: &str) -> String {
    cut_safely(line, MAX_LINE_CHARS)
}

/// The excerpt's lines, newest last: escapes and controls gone, empty lines dropped,
/// each line cut at 200 characters, at most 6 lines and 1,000 characters in all.
pub(crate) fn lines(text: &str) -> Vec<String> {
    let plain = plain(text);
    let mut kept: Vec<String> = plain
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if kept.len() > MAX_LINES {
        kept.drain(..kept.len() - MAX_LINES);
    }
    for line in &mut kept {
        *line = cut_line(line);
    }
    while kept.len() > 1
        && kept.iter().map(String::len).sum::<usize>() + kept.len() - 1 > MAX_EXCERPT_BYTES
    {
        kept.remove(0);
    }
    let mut total = 0;
    let mut out: Vec<String> = Vec::new();
    for line in kept.into_iter().rev() {
        let chars = line.chars().count();
        let added = chars + usize::from(!out.is_empty());
        if total + added > MAX_EXCERPT_CHARS {
            break;
        }
        total += added;
        out.push(line);
    }
    out.reverse();
    out
}

/// A no-echo prompt's label: the newest line that reads like a prompt, so a line a
/// program echoed after it is never taken for the label. None when no line reads
/// like one, and then the excerpt stays empty: a secret event carries no typed text.
pub(crate) fn secret_label(text: &str) -> Option<String> {
    plain(text)
        .lines()
        .rev()
        .find(|line| prompt_label_like(line))
        .map(str::to_owned)
}

fn prompt_label_like(line: &str) -> bool {
    let line = line.trim_end();
    if line.is_empty() {
        return false;
    }
    if line.ends_with(':') || line.ends_with('?') {
        return true;
    }
    let lower = line.to_ascii_lowercase();
    [
        "password",
        "passphrase",
        "passcode",
        "secret",
        "token",
        "username",
        "login",
        "passkey",
        "pin",
    ]
    .iter()
    .any(|word| {
        lower
            .split(|char: char| !char.is_ascii_alphanumeric())
            .any(|part| part == *word)
    })
}

/// One plain line: whitespace collapsed and the text cut at `max` characters.
pub(crate) fn one_line(text: &str, max: usize) -> String {
    let plain = plain(text);
    let joined = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    cut_safely(&joined, max)
}
