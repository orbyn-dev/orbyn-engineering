//! The streaming state machine for terminal marks: OSC 133 and 633 command
//! boundaries and exit codes, OSC 7 and 633 `P;Cwd=` working directories, and
//! the alternate-screen switch. Marks may be split anywhere across PTY reads.

use super::text::{clean, percent_decode};

/// What one chunk of PTY output says to the block store.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockEvent<'a> {
    /// Plain output bytes between escape sequences. Control characters are still
    /// here; the text cleaner removes them.
    Text(&'a [u8]),
    /// A command's output starts (OSC 133/633 `C`).
    Start {
        command: Option<String>,
        cwd: Option<String>,
    },
    /// A command's output ends. The code is `None` when the shell sent no code,
    /// the end arrived with `A` or the next `C`, or the terminal closed.
    End(Option<i32>),
    /// The alternate screen was entered (`true`) or left (`false`).
    Fullscreen(bool),
}

/// A sequence longer than this is not a mark or a working directory anyone
/// reads, so its body is dropped instead of growing without a bound.
const MAX_SEQUENCE: usize = 64 * 1024;
/// A working directory is a label, not a document.
const MAX_CWD_CHARS: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    Osc,
    OscEscape,
    Csi,
    Dcs,
    DcsEscape,
}

#[derive(Default)]
pub struct BlockParser {
    state: State,
    payload: Vec<u8>,
    overflow: bool,
    alternate_screen: bool,
    command: Option<String>,
    cwd: Option<String>,
    running: bool,
}

impl BlockParser {
    /// Reads one PTY chunk. A mark split at the end is carried into the next call.
    pub fn push(&mut self, chunk: &[u8], emit: &mut dyn FnMut(BlockEvent<'_>)) {
        let mut at = 0;
        while at < chunk.len() {
            match self.state {
                State::Ground => {
                    let start = at;
                    while at < chunk.len() && chunk[at] != 0x1b {
                        at += 1;
                    }
                    if at > start {
                        emit(BlockEvent::Text(&chunk[start..at]));
                    }
                    if at < chunk.len() {
                        self.state = State::Escape;
                        at += 1;
                    }
                }
                State::Escape => {
                    let kind = chunk[at];
                    at += 1;
                    match kind {
                        b']' => {
                            self.payload.clear();
                            self.overflow = false;
                            self.state = State::Osc;
                        }
                        b'[' => {
                            self.payload.clear();
                            self.overflow = false;
                            self.state = State::Csi;
                        }
                        b'P' | b'X' | b'^' | b'_' => {
                            self.state = State::Dcs;
                        }
                        byte if (0x20..=0x2f).contains(&byte) => {
                            self.state = State::EscapeIntermediate;
                        }
                        _ => {
                            self.state = State::Ground;
                        }
                    }
                }
                State::EscapeIntermediate => {
                    // The last intermediate byte ended; one final byte closes
                    // the sequence, as in `ESC ( B` and `ESC # 8`.
                    if !(0x20..=0x2f).contains(&chunk[at]) {
                        self.state = State::Ground;
                    }
                    at += 1;
                }
                State::Osc => match chunk[at] {
                    0x07 => {
                        self.finish_osc(emit);
                        self.state = State::Ground;
                        at += 1;
                    }
                    0x1b => {
                        self.state = State::OscEscape;
                        at += 1;
                    }
                    byte => {
                        self.keep(byte);
                        at += 1;
                    }
                },
                State::OscEscape => {
                    if chunk[at] == b'\\' {
                        self.finish_osc(emit);
                        self.state = State::Ground;
                        at += 1;
                    } else {
                        self.state = State::Osc;
                    }
                }
                State::Csi => {
                    let byte = chunk[at];
                    at += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        if !self.overflow {
                            self.finish_csi(byte, emit);
                        }
                        self.state = State::Ground;
                    } else {
                        self.keep(byte);
                    }
                }
                State::Dcs => {
                    if chunk[at] == 0x1b {
                        self.state = State::DcsEscape;
                    }
                    at += 1;
                }
                State::DcsEscape => {
                    if chunk[at] == b'\\' {
                        self.state = State::Ground;
                        at += 1;
                    } else {
                        self.state = State::Dcs;
                    }
                }
            }
        }
    }

    /// The output ended (or the terminal closed). A command still running ends
    /// there with no exit code.
    pub fn finish(&mut self, emit: &mut dyn FnMut(BlockEvent<'_>)) {
        self.state = State::Ground;
        self.payload.clear();
        self.overflow = false;
        self.end(None, emit);
    }

    fn keep(&mut self, byte: u8) {
        if self.payload.len() < MAX_SEQUENCE {
            self.payload.push(byte);
        } else {
            self.overflow = true;
        }
    }

    fn finish_osc(&mut self, emit: &mut dyn FnMut(BlockEvent<'_>)) {
        if self.overflow {
            self.overflow = false;
            return;
        }
        let payload = String::from_utf8_lossy(&self.payload).into_owned();
        let Some((ident, data)) = payload.split_once(';') else {
            return;
        };
        match ident {
            "133" | "633" => self.mark(ident, data, emit),
            "7" => {
                if let Some(cwd) = osc7_cwd(data) {
                    self.cwd = Some(cwd);
                }
            }
            _ => {}
        }
    }

    fn mark(&mut self, ident: &str, data: &str, emit: &mut dyn FnMut(BlockEvent<'_>)) {
        let (kind, rest) = match data.split_once(';') {
            Some((kind, rest)) => (kind, rest),
            None => (data, ""),
        };
        match kind {
            "A" => {
                self.end(None, emit);
                self.command = None;
            }
            "B" => {}
            "C" => {
                self.end(None, emit);
                let command = self.command.take();
                emit(BlockEvent::Start {
                    command,
                    cwd: self.cwd.clone(),
                });
                self.running = true;
            }
            "D" => {
                let code = rest
                    .split(';')
                    .next()
                    .and_then(|code| code.parse::<i32>().ok());
                self.end(code, emit);
            }
            "E" if ident == "633" => {
                let command = rest.split(';').next().unwrap_or("");
                self.command = Some(decode_command_line(command));
            }
            "P" if ident == "633" => {
                if let Some(cwd) = rest.strip_prefix("Cwd=") {
                    let cwd = bounded_cwd(cwd);
                    self.cwd = (!cwd.is_empty()).then_some(cwd);
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, code: Option<i32>, emit: &mut dyn FnMut(BlockEvent<'_>)) {
        if self.running {
            self.running = false;
            emit(BlockEvent::End(code));
        }
    }

    fn finish_csi(&mut self, final_byte: u8, emit: &mut dyn FnMut(BlockEvent<'_>)) {
        if final_byte != b'h' && final_byte != b'l' {
            return;
        }
        let Some(body) = self.payload.strip_prefix(b"?".as_slice()) else {
            return;
        };
        let alternate = body.split(|byte| *byte == b';').any(|part| {
            part == b"1049".as_slice() || part == b"1047".as_slice() || part == b"47".as_slice()
        });
        if !alternate {
            return;
        }
        let on = final_byte == b'h';
        if on != self.alternate_screen {
            self.alternate_screen = on;
            emit(BlockEvent::Fullscreen(on));
        }
    }
}

/// `file://host/path` as the directory the shell reported.
fn osc7_cwd(data: &str) -> Option<String> {
    let rest = data.strip_prefix("file://")?;
    let path = rest.find('/').map_or("", |at| &rest[at..]);
    if path.is_empty() {
        return None;
    }
    let decoded = percent_decode(path);
    if decoded.is_empty() {
        return None;
    }
    // A Windows shell reports `file:///C:/work`: drop the URI's empty-host slash.
    let cwd = if decoded.len() > 2
        && decoded.starts_with('/')
        && decoded.as_bytes()[1].is_ascii_alphabetic()
        && decoded.as_bytes()[2] == b':'
    {
        decoded[1..].to_owned()
    } else {
        decoded
    };
    let cwd = bounded_cwd(&cwd);
    if cwd.is_empty() { None } else { Some(cwd) }
}

fn bounded_cwd(cwd: &str) -> String {
    clean(cwd).chars().take(MAX_CWD_CHARS).collect()
}

/// OSC 633 `E` escapes `;` and other bytes as `\xHH`, and a backslash as `\\`.
fn decode_command_line(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut bytes = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\\' && at + 1 < chars.len() {
            if chars[at + 1] == '\\' {
                bytes.push(b'\\');
                at += 2;
                continue;
            }
            if chars[at + 1] == 'x'
                && at + 3 < chars.len()
                && let (Some(high), Some(low)) =
                    (chars[at + 2].to_digit(16), chars[at + 3].to_digit(16))
            {
                bytes.push((high * 16 + low) as u8);
                at += 4;
                continue;
            }
        }
        let mut buf = [0u8; 4];
        bytes.extend_from_slice(chars[at].encode_utf8(&mut buf).as_bytes());
        at += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Text(String),
        Start {
            command: Option<String>,
            cwd: Option<String>,
        },
        End(Option<i32>),
        Fullscreen(bool),
    }

    fn collect(events: &mut Vec<Event>, event: BlockEvent<'_>) {
        match event {
            BlockEvent::Text(bytes) => {
                let text = String::from_utf8_lossy(bytes).into_owned();
                if let Some(Event::Text(last)) = events.last_mut() {
                    last.push_str(&text);
                } else {
                    events.push(Event::Text(text));
                }
            }
            BlockEvent::Start { command, cwd } => events.push(Event::Start { command, cwd }),
            BlockEvent::End(code) => events.push(Event::End(code)),
            BlockEvent::Fullscreen(on) => events.push(Event::Fullscreen(on)),
        }
    }

    fn run(chunks: &[&[u8]]) -> Vec<Event> {
        let mut parser = BlockParser::default();
        let mut events = Vec::new();
        for chunk in chunks {
            parser.push(chunk, &mut |event| collect(&mut events, event));
        }
        parser.finish(&mut |event| collect(&mut events, event));
        events
    }

    fn start(command: Option<&str>, cwd: Option<&str>) -> Event {
        Event::Start {
            command: command.map(str::to_owned),
            cwd: cwd.map(str::to_owned),
        }
    }

    #[test]
    fn marks_split_across_chunks_start_and_end_a_block() {
        let events = run(&[b"\x1b]13", b"3;C\x07hel", b"lo\n\x1b]133;D;", b"7\x07"]);
        assert_eq!(
            events,
            vec![
                start(None, None),
                Event::Text("hello\n".into()),
                Event::End(Some(7))
            ]
        );
    }

    #[test]
    fn a_finish_mark_without_a_code_ends_with_none() {
        assert_eq!(
            run(&[b"\x1b]133;C\x07out\x1b]133;D\x07"]),
            vec![
                start(None, None),
                Event::Text("out".into()),
                Event::End(None)
            ]
        );
        assert_eq!(
            run(&[b"\x1b]133;C\x07out\x1b]133;D;-1\x07"]),
            vec![
                start(None, None),
                Event::Text("out".into()),
                Event::End(Some(-1))
            ]
        );
        assert_eq!(
            run(&[b"\x1b]133;C\x07out\x1b]133;D;not-a-number\x07"]),
            vec![
                start(None, None),
                Event::Text("out".into()),
                Event::End(None)
            ]
        );
    }

    #[test]
    fn a_next_prompt_or_command_ends_a_block_without_a_code() {
        assert_eq!(
            run(&[b"\x1b]133;C\x07one\x1b]133;C\x07two\x1b]133;A\x07"]),
            vec![
                start(None, None),
                Event::Text("one".into()),
                Event::End(None),
                start(None, None),
                Event::Text("two".into()),
                Event::End(None),
            ]
        );
        assert_eq!(run(&[b"\x1b]133;B\x07"]), Vec::new());
    }

    #[test]
    fn nested_and_garbled_osc_sequences_are_ignored() {
        let events = run(&[
            b"\x1b]133;X;weird\x07",
            b"\x1b]999;whatever\x1b\\",
            b"\x1b]133;C\x1b]633;E;echo hi\x07outside",
            b"\x1b]133;D;5\x07",
        ]);
        assert_eq!(events, vec![Event::Text("outside".into())]);
    }

    #[test]
    fn other_escape_sequences_are_removed_whole() {
        assert_eq!(
            run(&[b"a\x1b(Bb\x1b#8c\x1b7d"]),
            vec![Event::Text("abcd".into())]
        );
    }

    #[test]
    fn alternate_screen_switches_are_read_across_chunks() {
        assert_eq!(
            run(&[
                b"\x1b[?10",
                b"49h\x1b]133;C\x07out\x1b]133;D;0\x07\x1b[?104",
                b"9l",
            ]),
            vec![
                Event::Fullscreen(true),
                start(None, None),
                Event::Text("out".into()),
                Event::End(Some(0)),
                Event::Fullscreen(false),
            ]
        );
        assert_eq!(
            run(&[b"\x1b[?47h\x1b[?47h\x1b[?1047l"]),
            vec![Event::Fullscreen(true), Event::Fullscreen(false)]
        );
    }

    #[test]
    fn the_shells_own_command_line_and_cwd_are_used() {
        assert_eq!(
            run(&[
                b"\x1b]633;E;echo\\x20hi\\x3b;nonce\x07",
                b"\x1b]7;file://host/tmp/work\x07\x1b]133;C\x07",
                b"\x1b]133;D;0\x07",
            ]),
            vec![
                start(Some("echo hi;"), Some("/tmp/work")),
                Event::End(Some(0)),
            ]
        );
        assert_eq!(
            run(&[b"\x1b]633;P;Cwd=/tmp/other\x07\x1b]133;C\x07\x1b]133;D\x07",]),
            vec![start(None, Some("/tmp/other")), Event::End(None)]
        );
        assert_eq!(
            run(&[b"\x1b]7;file:///C:/work\x07\x1b]133;C\x07\x1b]133;D\x07",]),
            vec![start(None, Some("C:/work")), Event::End(None)]
        );
    }

    #[test]
    fn every_byte_of_a_mark_can_arrive_in_its_own_chunk() {
        let full = b"\x1b]133;C\x07hi\x1b]133;D;3\x07";
        let chunks: Vec<&[u8]> = full.chunks(1).collect();
        assert_eq!(
            run(&chunks),
            vec![
                start(None, None),
                Event::Text("hi".into()),
                Event::End(Some(3))
            ]
        );
    }

    #[test]
    fn finish_ends_a_running_command_without_a_code() {
        assert_eq!(
            run(&[b"\x1b]133;C\x07still here"]),
            vec![
                start(None, None),
                Event::Text("still here".into()),
                Event::End(None)
            ]
        );
    }
}
