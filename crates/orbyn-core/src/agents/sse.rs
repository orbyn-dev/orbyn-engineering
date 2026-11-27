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
    /// How far into `buffer` a newline has been looked for and none found: the next push resumes the
    /// search there instead of looking at the bytes it already knows about again.
    scanned: usize,
    data: Vec<String>,
    /// The bytes the frame's `data:` lines hold, kept as they arrive so the cap check stays linear.
    data_len: usize,
    /// Bytes the newline search looked at. Counted so the tests can bound the work the parser does
    /// instead of the clock it takes, which is what makes a rescan visible.
    #[cfg(test)]
    searched: usize,
    /// Bytes a completed line made the buffer move. Counted the same way, so a drain per line shows.
    #[cfg(test)]
    shifted: usize,
}

impl SseParser {
    /// Feeds a network chunk and returns the `data` of each completed frame, or refuses a frame that grows
    /// past [`MAX_FRAME_BYTES`] instead of buffering it.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, FrameTooLarge> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        // The lines this push completes are cut out of the buffer once, at the end, so one line does
        // not move the bytes behind it. `consumed` is where the first one starts.
        let mut consumed = 0;
        let mut search = self.scanned;
        loop {
            let from = search;
            let found = self.buffer[from..].iter().position(|byte| *byte == b'\n');
            #[cfg(test)]
            {
                self.searched += found.map_or(self.buffer.len() - from, |offset| offset + 1);
            }
            let Some(end) = found.map(|offset| from + offset) else {
                // No complete line yet: a line that grows past the cap is refused instead of kept.
                if self.buffer.len() - consumed > MAX_FRAME_BYTES {
                    return Err(FrameTooLarge);
                }
                search = self.buffer.len();
                break;
            };
            if end + 1 - consumed > MAX_FRAME_BYTES {
                return Err(FrameTooLarge);
            }
            let text = String::from_utf8_lossy(&self.buffer[consumed..=end]);
            let text = text.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                if !self.data.is_empty() {
                    frames.push(self.data.join("\n"));
                    self.data.clear();
                    self.data_len = 0;
                }
            } else if let Some(value) = text.strip_prefix("data:") {
                let value = value.strip_prefix(' ').unwrap_or(value);
                // One byte per line stands for the newline the join adds; counting as the lines arrive keeps
                // the check linear, so a frame split into very many lines cannot make the parser quadratic.
                self.data_len += value.len() + 1;
                if self.data_len > MAX_FRAME_BYTES {
                    return Err(FrameTooLarge);
                }
                self.data.push(value.to_owned());
            }
            consumed = end + 1;
            search = consumed;
        }
        if consumed > 0 {
            self.buffer.drain(..consumed);
            #[cfg(test)]
            {
                self.shifted += self.buffer.len();
            }
        }
        self.scanned = search - consumed;
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
        // Two lines pass the cap and keep the test linear: a cap check that re-summed the frame per line would
        // itself be a denial of service, so this case must stay cheap.
        let mut body = b"data: ".to_vec();
        body.extend(std::iter::repeat_n(b'a', MAX_FRAME_BYTES / 2));
        body.push(b'\n');
        let mut lines = body.clone();
        lines.extend_from_slice(&body);
        assert_eq!(parser.push(&lines), Err(FrameTooLarge));
    }

    /// An 8 MiB line arrives the way a network delivers one: in small chunks, with no newline until the
    /// last byte. A search that starts from zero on every push looks at the bytes it already knows about
    /// again for each chunk, which is one pass per chunk over the line's length; the work grows with the
    /// square of the line. This bounds that work instead of timing it: the line may be looked at once.
    #[test]
    fn an_unbroken_8_mib_line_is_searched_once_not_once_per_chunk() {
        let mut parser = SseParser::default();
        let mut line = vec![b'a'; MAX_FRAME_BYTES - 1];
        line.push(b'\n');
        for part in line.chunks(8 * 1024) {
            assert!(
                parser.push(part).unwrap().is_empty(),
                "the line holds no frame"
            );
        }
        assert!(
            parser.searched <= MAX_FRAME_BYTES,
            "one pass over an 8 MiB line looks at {MAX_FRAME_BYTES} bytes; this push looked at {} \
             (about {} for a search that restarts each chunk)",
            parser.searched,
            line.len() * line.len() / (2 * 8 * 1024)
        );
    }

    /// Lines that complete in one push are cut from the buffer once, at the end of the push; cutting each
    /// one out as it completes moves everything behind it, which is the tail's length per line.
    #[test]
    fn lines_that_complete_in_one_push_shift_the_tail_once() {
        let mut parser = SseParser::default();
        let mut chunk = Vec::new();
        for _ in 0..512 {
            chunk.extend_from_slice(b"data: x\n\n");
        }
        let tail = 1024 * 1024;
        chunk.extend(std::iter::repeat_n(b'y', tail));
        let frames = parser.push(&chunk).unwrap();
        assert_eq!(frames.len(), 512);
        assert_eq!(parser.buffer.len(), tail, "the partial line stays buffered");
        assert!(
            parser.shifted <= tail,
            "the tail moves once per push, not once per line: {} bytes moved",
            parser.shifted
        );
    }
}
