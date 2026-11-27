//! Turning terminal bytes into plain stored text: UTF-8 read lossily, no escape
//! sequences, no control characters except newline and tab, and a carriage
//! return that keeps the last text of its line (docs/command-blocks.md).

/// The longest command line a block keeps.
pub const MAX_COMMAND_CHARS: usize = 4096;
/// One line longer than this is dropped whole: keeping a tail could hide the
/// front of a secret from the redactor, and an unfinished line must stay bounded.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Streaming cleaner for the text of one command. An incomplete UTF-8 character
/// and an unfinished line wait for the next chunk; a carriage return that redraws
/// a line replaces the text before it, as the person saw.
#[derive(Default)]
pub struct TextCleaner {
    carry: Vec<u8>,
    /// The last non-empty piece before the latest carriage return.
    last: String,
    /// What has been written since the latest carriage return.
    segment: String,
    /// The current line outgrew its bound and is not kept at all.
    dropping: bool,
    /// Lines discarded whole because they outgrew [`MAX_LINE_BYTES`].
    dropped_lines: usize,
}

impl TextCleaner {
    /// The clean complete lines in `bytes`. A partial last line is kept back
    /// until a newline or [`finish`](Self::finish).
    pub fn push(&mut self, bytes: &[u8]) -> String {
        let text = self.decode(bytes);
        let mut out = String::new();
        for c in text.chars() {
            self.push_char(c, &mut out);
        }
        out
    }

    /// The unfinished last line, once the command's output ends.
    pub fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.carry);
        let text = String::from_utf8_lossy(&rest).into_owned();
        let mut out = String::new();
        for c in text.chars() {
            self.push_char(c, &mut out);
        }
        if self.dropping {
            self.dropping = false;
            self.dropped_lines += 1;
        }
        let line = if self.segment.is_empty() {
            std::mem::take(&mut self.last)
        } else {
            std::mem::take(&mut self.segment)
        };
        out.push_str(&line);
        self.last.clear();
        out
    }

    /// Lines the bound discarded whole since the last call.
    pub fn take_dropped_lines(&mut self) -> usize {
        std::mem::take(&mut self.dropped_lines)
    }

    fn push_char(&mut self, c: char, out: &mut String) {
        match c {
            '\r' => {
                self.dropping = false;
                if !self.segment.is_empty() {
                    self.last = std::mem::take(&mut self.segment);
                }
            }
            '\n' => {
                if std::mem::take(&mut self.dropping) {
                    self.segment.clear();
                    self.last.clear();
                    self.dropped_lines += 1;
                    return;
                }
                let line = if self.segment.is_empty() {
                    std::mem::take(&mut self.last)
                } else {
                    std::mem::take(&mut self.segment)
                };
                out.push_str(&line);
                out.push('\n');
                self.last.clear();
            }
            '\t' => self.keep('\t'),
            other if !other.is_control() => self.keep(other),
            _ => {}
        }
    }

    fn keep(&mut self, c: char) {
        if self.dropping {
            return;
        }
        if self.segment.len() + c.len_utf8() > MAX_LINE_BYTES {
            self.segment.clear();
            self.last.clear();
            self.dropping = true;
            return;
        }
        self.segment.push(c);
    }

    /// Decodes what is valid now and carries an incomplete character forward.
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.carry.extend_from_slice(bytes);
        let mut text = String::new();
        loop {
            let error = match std::str::from_utf8(&self.carry) {
                Ok(valid) => {
                    text.push_str(valid);
                    self.carry.clear();
                    break;
                }
                Err(error) => error,
            };
            let valid = error.valid_up_to();
            if let Ok(part) = std::str::from_utf8(&self.carry[..valid]) {
                text.push_str(part);
            }
            match error.error_len() {
                Some(len) => {
                    text.push(char::REPLACEMENT_CHARACTER);
                    self.carry.drain(..valid + len);
                }
                None => {
                    self.carry.drain(..valid);
                    break;
                }
            }
        }
        text
    }
}

/// `text` cleaned as if it were complete: escapes removed, control characters
/// dropped, carriage returns resolved, and its last line kept.
pub fn clean(text: &str) -> String {
    let plain = strip_sequences(text);
    let mut cleaner = TextCleaner::default();
    let mut out = cleaner.push(plain.as_bytes());
    out.push_str(&cleaner.finish());
    out
}

/// A shell's own command line as a block header keeps it: escapes removed, one
/// line of spaces, at most 4096 characters. Empty is null.
pub fn clean_command(command: &str) -> Option<String> {
    let cleaned = clean(command);
    let line = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(MAX_COMMAND_CHARS).collect())
}

/// Removes OSC, DCS and other string sequences, CSI sequences and every other
/// ESC sequence. Unit tests call this through [`clean`]; the parser removes the
/// same sequences from the live stream.
fn strip_sequences(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != 0x1b {
            let start = at;
            while at < bytes.len() && bytes[at] != 0x1b {
                at += 1;
            }
            out.push_str(&text[start..at]);
            continue;
        }
        at += 1;
        let Some(kind) = bytes.get(at).copied() else {
            break;
        };
        match kind {
            b']' => {
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == 0x07 {
                        at += 1;
                        break;
                    }
                    if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'\\') {
                        at += 2;
                        break;
                    }
                    at += 1;
                }
            }
            b'[' => {
                at += 1;
                while at < bytes.len() && !(0x40..=0x7e).contains(&bytes[at]) {
                    at += 1;
                }
                if at < bytes.len() {
                    at += 1;
                }
            }
            b'P' | b'X' | b'^' | b'_' => {
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'\\') {
                        at += 2;
                        break;
                    }
                    at += 1;
                }
            }
            _ => {
                // An optional run of intermediate bytes and one final byte, as
                // in `ESC ( B`, `ESC 7` and `ESC c`. `at` still names the first
                // byte after ESC.
                while at < bytes.len() && (0x20..=0x2f).contains(&bytes[at]) {
                    at += 1;
                }
                if at < bytes.len() && (0x30..=0x7e).contains(&bytes[at]) {
                    at += 1;
                }
            }
        }
    }
    out
}

/// Percent-decodes a URI path, keeping invalid escapes as written.
pub(super) fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let high = (bytes[at + 1] as char).to_digit(16);
            let low = (bytes[at + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_and_other_escape_sequences_are_removed() {
        assert_eq!(clean("\u{1b}[31mred\u{1b}[0m"), "red");
        assert_eq!(
            clean("\u{1b}]8;;http://example.invalid\u{7}link\u{1b}]8;;\u{1b}\\"),
            "link"
        );
        assert_eq!(clean("\u{1b}]0;title\u{7}tail"), "tail");
        assert_eq!(clean("a\u{1b}Ppayload\u{1b}\\b"), "ab");
        assert_eq!(clean("a\u{1b}(Bb"), "ab");
        assert_eq!(clean("a\u{1b}7b"), "ab");
    }

    #[test]
    fn a_carriage_return_keeps_the_last_text_of_the_line() {
        assert_eq!(clean("progress 10%\rprogress 100%\n"), "progress 100%\n");
        assert_eq!(clean("abc\r\n"), "abc\n");
        assert_eq!(clean("abc\r"), "abc");
        assert_eq!(clean("one\rone\rtwo\n"), "two\n");
    }

    #[test]
    fn invalid_utf8_reads_as_replacement_characters() {
        let mut cleaner = TextCleaner::default();
        let mut out = cleaner.push(&[0xff, b'a', 0xf0, 0x9f]);
        out.push_str(&cleaner.finish());
        assert_eq!(out, "\u{fffd}a\u{fffd}");
    }

    #[test]
    fn control_characters_are_dropped_except_tab_and_newline() {
        assert_eq!(
            clean("a\u{0}b\u{7}c\td\ne\u{1f}f\u{7f}g\u{9b}h"),
            "abc\td\nefgh"
        );
    }

    #[test]
    fn chunks_split_utf8_and_lines_without_losing_them() {
        let mut cleaner = TextCleaner::default();
        let mut out = cleaner.push("caf".as_bytes());
        out.push_str(&cleaner.push(&[0xc3]));
        out.push_str(&cleaner.push(&[0xa9, b'\n', b'o', b'k', b'\n', b'x']));
        out.push_str(&cleaner.finish());
        assert_eq!(out, "caf\u{e9}\nok\nx");
    }

    #[test]
    fn a_carriage_return_split_across_chunks_still_redraws() {
        let mut cleaner = TextCleaner::default();
        assert_eq!(cleaner.push(b"one\r"), "");
        assert_eq!(cleaner.push(b"two\n"), "two\n");
    }

    #[test]
    fn an_overlong_line_is_dropped_whole_and_the_next_one_kept() {
        let mut cleaner = TextCleaner::default();
        let long = "\u{e9}".repeat(MAX_LINE_BYTES / 2 + 10);
        assert_eq!(cleaner.push(long.as_bytes()), "");
        assert_eq!(cleaner.push(b"\nkept\n"), "kept\n");
        assert_eq!(cleaner.take_dropped_lines(), 1);
    }

    #[test]
    fn a_command_becomes_one_capped_line() {
        assert_eq!(
            clean_command("\u{1b}[32mgit status\u{1b}[0m"),
            Some("git status".into())
        );
        assert_eq!(
            clean_command("  echo\n\thi\tthere  "),
            Some("echo hi there".into())
        );
        assert_eq!(
            clean_command("à".repeat(MAX_COMMAND_CHARS + 10).as_str())
                .unwrap()
                .chars()
                .count(),
            MAX_COMMAND_CHARS
        );
        assert_eq!(clean_command(" \n\t"), None);
    }

    #[test]
    fn percent_escapes_are_decoded_and_bad_ones_kept() {
        assert_eq!(percent_decode("/tmp/caf%C3%A9"), "/tmp/café");
        assert_eq!(percent_decode("/tmp/%zz"), "/tmp/%zz");
    }
}
