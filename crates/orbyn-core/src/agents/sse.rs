//! Incremental `text/event-stream` parser for engines that speak SSE.

/// The largest line or frame the parser keeps. A remote agent must not grow the buffer without bound: a line
/// with no newline, a single over-long line, or many `data:` lines adding up past this are refused.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Why a stream was refused: one of its lines or frames grew past [`MAX_FRAME_BYTES`] before it completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The agent's stream sent a frame over 8 MiB.")]
pub struct FrameTooLarge;

#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    /// Feeds a network chunk and returns the `data` of each completed frame, or refuses a frame that grows
    /// past [`MAX_FRAME_BYTES`] instead of buffering it.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, FrameTooLarge> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') else {
                // No complete line yet: a line that grows past the cap is refused instead of kept.
                if self.buffer.len() > MAX_FRAME_BYTES {
                    return Err(FrameTooLarge);
                }
                break;
            };
            if end + 1 > MAX_FRAME_BYTES {
                return Err(FrameTooLarge);
            }
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                if !self.data.is_empty() {
                    frames.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(value) = text.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).to_owned());
                let kept: usize =
                    self.data.iter().map(String::len).sum::<usize>() + self.data.len();
                if kept > MAX_FRAME_BYTES {
                    return Err(FrameTooLarge);
                }
            }
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_span_chunks_and_multibyte_splits() {
        let mut parser = SseParser::default();
        let body = "id: 1\ndata: {\"a\":\"é\"}\n\n: keep\n\ndata: x\r\ndata: y\r\n\r\n".as_bytes();
        let (left, right) = body.split_at(14);
        let mut frames = parser.push(left).unwrap();
        frames.extend(parser.push(right).unwrap());
        assert_eq!(frames, vec!["{\"a\":\"é\"}", "x\ny"]);
    }

    #[test]
    fn a_stream_with_no_newline_is_refused_once_it_passes_the_cap() {
        let mut parser = SseParser::default();
        let chunk = vec![b'a'; 1024 * 1024];
        for _ in 0..8 {
            parser.push(&chunk).unwrap();
        }
        // Exactly at the cap a chunk that carries no newline is still accepted; one more byte refuses it.
        assert_eq!(parser.push(b"a"), Err(FrameTooLarge));
    }

    #[test]
    fn a_single_line_over_the_cap_is_refused() {
        let mut parser = SseParser::default();
        let mut line = b"data: ".to_vec();
        line.extend(std::iter::repeat_n(b'a', MAX_FRAME_BYTES));
        line.push(b'\n');
        assert_eq!(parser.push(&line), Err(FrameTooLarge));
    }

    #[test]
    fn a_frame_whose_data_lines_add_up_past_the_cap_is_refused() {
        let mut parser = SseParser::default();
        let line = b"data: aaaaaaaa\n".repeat(MAX_FRAME_BYTES / 8);
        assert_eq!(parser.push(&line), Err(FrameTooLarge));
    }
}
