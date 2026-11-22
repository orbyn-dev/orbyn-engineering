//! The in-memory store behind a terminal's command blocks: limits, cursors,
//! redacted text and the reader-thread entry points.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use orbyn_contracts::{BlockLimits, BlockList, BlockOutputPage, CommandBlock};

use crate::error::{CoreError, Result};
use crate::host::blocks::parser::{BlockEvent, BlockParser};
use crate::host::blocks::text::{TextCleaner, clean_command};
use crate::redaction::{Redactor, Surface, redactor_from};
use crate::store::Store;
use crate::util;

/// Blocks one terminal keeps; the oldest is dropped whole.
pub const MAX_BLOCKS: usize = 500;
/// Text one block keeps; its top is dropped and counted.
pub const MAX_BLOCK_BYTES: usize = 1024 * 1024;
/// Text one terminal keeps across its blocks; the oldest blocks are dropped whole.
pub const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;
/// One list page.
pub const DEFAULT_PAGE: usize = 50;
/// The most blocks one list answer carries.
pub const MAX_PAGE: usize = 200;
/// One output page.
pub const DEFAULT_OUTPUT_BYTES: usize = 64 * 1024;
/// The most output bytes one answer carries.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

const LIST_CURSOR: &str = "v1:";
const OUTPUT_CURSOR: &str = "v1:";

/// The block limits as the wire type reports them.
pub fn limits() -> BlockLimits {
    BlockLimits {
        max_blocks: MAX_BLOCKS,
        max_block_bytes: MAX_BLOCK_BYTES,
        max_total_bytes: MAX_TOTAL_BYTES,
    }
}

/// The store a terminal manager hands to each new terminal. It reads the terminal
/// redaction set once when the terminal opens; no command text is kept when the
/// set cannot be read.
pub struct BlockStore {
    store: Arc<Store>,
}

impl BlockStore {
    pub(crate) fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// One terminal's blocks. When the redaction rules cannot be read it keeps
    /// nothing, and the routes answer an empty list rather than trusting
    /// unredacted text.
    pub(crate) fn terminal(&self, thread_id: &str, terminal_id: &str) -> Arc<TerminalBlocks> {
        let redactor = match redactor_from(&self.store, Surface::Terminal, None) {
            Ok(redactor) => Some(redactor),
            Err(error) => {
                tracing::warn!(
                    %error,
                    thread_id,
                    terminal = terminal_id,
                    "terminal command blocks are off: the redaction rules could not be read"
                );
                None
            }
        };
        let mut blocks = TerminalBlocks::new(redactor, thread_id, terminal_id);
        blocks.store = Some(self.store.clone());
        Arc::new(blocks)
    }
}

/// One terminal's blocks. The PTY reader pushes output here; the routes read it.
pub struct TerminalBlocks {
    inner: Mutex<Inner>,
    redactor: Option<Arc<Redactor>>,
    /// Where the rule set is read again when an answer is built; `None` in tests.
    store: Option<Arc<Store>>,
}

struct Inner {
    thread_id: String,
    terminal_id: String,
    parser: BlockParser,
    cleaner: TextCleaner,
    blocks: VecDeque<Block>,
    total_bytes: usize,
    next_seq: u64,
    alternate_screen: bool,
    closed: bool,
}

struct Block {
    seq: u64,
    id: String,
    command: Option<String>,
    started_at: String,
    ended_at: Option<String>,
    exit_code: Option<i32>,
    cwd: Option<String>,
    text: String,
    pending: String,
    dropped_bytes: usize,
    dropped_lines: usize,
    truncated: bool,
    fullscreen: bool,
}

impl Block {
    fn ended(&self) -> bool {
        self.ended_at.is_some()
    }

    fn view(&self, redactor: &Redactor) -> CommandBlock {
        CommandBlock {
            id: self.id.clone(),
            command: self.command.as_deref().map(|text| redact(redactor, text)),
            started_at: self.started_at.clone(),
            ended_at: self.ended_at.clone(),
            exit_code: self.exit_code,
            cwd: self.cwd.clone(),
            output_bytes: self.text.len(),
            truncated: self.truncated,
            dropped_lines: self.dropped_lines,
            fullscreen: self.fullscreen,
        }
    }
}

impl TerminalBlocks {
    pub(crate) fn new(redactor: Option<Arc<Redactor>>, thread_id: &str, terminal_id: &str) -> Self {
        Self {
            inner: Mutex::new(Inner {
                thread_id: thread_id.to_owned(),
                terminal_id: terminal_id.to_owned(),
                parser: BlockParser::default(),
                cleaner: TextCleaner::default(),
                blocks: VecDeque::new(),
                total_bytes: 0,
                next_seq: 0,
                alternate_screen: false,
                closed: false,
            }),
            redactor,
            store: None,
        }
    }

    /// The rule set as it is now, for an answer. Tests keep the captured one.
    fn answer_redactor(&self) -> Option<Arc<Redactor>> {
        match &self.store {
            Some(store) => redactor_from(store, Surface::Terminal, None).ok(),
            None => self.redactor.clone(),
        }
    }

    /// One PTY read, before the existing output mask sees it. The hook that hands
    /// a chunk here is in `host::terminal`'s reader thread.
    pub fn push(&self, chunk: &[u8]) {
        let Some(redactor) = self.redactor.clone() else {
            return;
        };
        let mut inner = lock(&self.inner);
        if inner.closed {
            return;
        }
        let mut parser = std::mem::take(&mut inner.parser);
        parser.push(chunk, &mut |event| inner.event(event, &redactor));
        inner.parser = parser;
    }

    /// The terminal's output ended, or it closed: a running block ends with no
    /// exit code and the recorder keeps nothing more.
    pub fn finish(&self) {
        let Some(redactor) = self.redactor.clone() else {
            return;
        };
        let mut inner = lock(&self.inner);
        if inner.closed {
            return;
        }
        let mut parser = std::mem::take(&mut inner.parser);
        parser.finish(&mut |event| inner.event(event, &redactor));
        inner.parser = parser;
        inner.closed = true;
        // The parser emits the end when it saw a C; this covers a running block
        // whose mark arrived in an incomplete sequence.
        inner.end_running(None, &redactor);
    }

    /// The newest page, or the page before `before`, oldest first.
    pub fn list(&self, before: Option<&str>, limit: usize) -> Result<BlockList> {
        let Some(redactor) = self.answer_redactor() else {
            return Ok(BlockList {
                blocks: Vec::new(),
                next_before: None,
                limits: limits(),
            });
        };
        let inner = lock(&self.inner);
        let limit = limit.clamp(1, MAX_PAGE);
        let mut end = inner.blocks.len();
        if let Some(cursor) = before {
            let seq = parse_list_cursor(cursor)?;
            match inner.blocks.iter().position(|block| block.seq == seq) {
                Some(at) => end = at,
                None if inner.blocks.front().is_none_or(|block| seq < block.seq) => {
                    return Err(cursor_expired());
                }
                None => return Err(bad_cursor()),
            }
        }
        let start = end.saturating_sub(limit);
        let blocks = inner
            .blocks
            .iter()
            .skip(start)
            .take(end - start)
            .map(|block| block.view(&redactor))
            .collect();
        let next_before = if start > 0 {
            inner.blocks.get(start).map(|block| list_cursor(block.seq))
        } else {
            None
        };
        Ok(BlockList {
            blocks,
            next_before,
            limits: limits(),
        })
    }

    /// One page of a block's kept output, oldest first.
    pub fn output(
        &self,
        block_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<BlockOutputPage> {
        let Some(redactor) = self.answer_redactor() else {
            return Err(CoreError::NotFound("block".into()));
        };
        let inner = lock(&self.inner);
        let block = inner
            .blocks
            .iter()
            .find(|block| block.id == block_id)
            .ok_or_else(|| CoreError::NotFound("block".into()))?;
        if block.fullscreen {
            return Ok(BlockOutputPage {
                text: String::new(),
                next: None,
                total_lines: if block.ended() { Some(0) } else { None },
                dropped_lines: 0,
            });
        }
        let limit = limit.clamp(1, MAX_OUTPUT_BYTES);
        let start = match after {
            Some(cursor) => parse_output_cursor(cursor, &block.id)?,
            None => block.dropped_bytes,
        };
        if start < block.dropped_bytes {
            return Err(cursor_expired());
        }
        let relative = start - block.dropped_bytes;
        if relative > block.text.len() || !block.text.is_char_boundary(relative) {
            return Err(bad_cursor());
        }
        let mut end = (relative + limit).min(block.text.len());
        while end > relative && !block.text.is_char_boundary(end) {
            end -= 1;
        }
        if end == relative && relative < block.text.len() {
            // A byte limit smaller than the next character still moves: one
            // character is the smallest page, and it is never cut.
            end = relative
                + block.text[relative..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
        }
        if end < block.text.len()
            && let Some(newline) = block.text[relative..end].rfind('\n')
        {
            end = relative + newline + 1;
        }
        let text = redact(&redactor, &block.text[relative..end]);
        let next = if end < block.text.len() {
            Some(output_cursor(&block.id, block.dropped_bytes + end))
        } else {
            None
        };
        Ok(BlockOutputPage {
            text,
            next,
            total_lines: block.ended().then(|| line_count(&block.text)),
            dropped_lines: block.dropped_lines,
        })
    }

    #[cfg(test)]
    fn count(&self) -> usize {
        lock(&self.inner).blocks.len()
    }
}

impl Inner {
    fn event(&mut self, event: BlockEvent<'_>, redactor: &Redactor) {
        match event {
            BlockEvent::Text(bytes) => self.text(bytes, redactor),
            BlockEvent::Start { command, cwd } => self.start(command, cwd, redactor),
            BlockEvent::End(code) => self.end_running(code, redactor),
            BlockEvent::Fullscreen(on) => self.fullscreen(on),
        }
    }

    fn text(&mut self, bytes: &[u8], redactor: &Redactor) {
        if !self
            .blocks
            .back()
            .is_some_and(|block| !block.ended() && !block.fullscreen)
        {
            return;
        }
        let clean = self.cleaner.push(bytes);
        let dropped = self.cleaner.take_dropped_lines();
        if clean.is_empty() && dropped == 0 {
            return;
        }
        let Some(block) = self.blocks.back_mut() else {
            return;
        };
        if dropped > 0 {
            block.dropped_lines += dropped;
            block.truncated = true;
        }
        block.pending.push_str(&clean);
        let added = flush_pending(block, redactor, false);
        self.total_bytes = self.total_bytes.saturating_add(added);
        let removed = trim_block(block);
        self.total_bytes = self.total_bytes.saturating_sub(removed);
        enforce_total(&mut self.blocks, &mut self.total_bytes);
    }

    fn start(&mut self, command: Option<String>, cwd: Option<String>, redactor: &Redactor) {
        self.end_running(None, redactor);
        self.cleaner = TextCleaner::default();
        let command = command
            .and_then(|raw| clean_command(&raw))
            .map(|line| redact(redactor, &line));
        let block = Block {
            seq: self.next_seq,
            id: util::new_id(),
            command,
            started_at: util::now(),
            ended_at: None,
            exit_code: None,
            cwd,
            text: String::new(),
            pending: String::new(),
            dropped_bytes: 0,
            dropped_lines: 0,
            truncated: false,
            fullscreen: self.alternate_screen,
        };
        self.next_seq += 1;
        self.blocks.push_back(block);
        while self.blocks.len() > MAX_BLOCKS {
            let Some(at) = self.blocks.iter().position(Block::ended) else {
                break;
            };
            if let Some(removed) = self.blocks.remove(at) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.text.len());
            }
        }
    }

    fn end_running(&mut self, code: Option<i32>, redactor: &Redactor) {
        let tail = self.cleaner.finish();
        let dropped = self.cleaner.take_dropped_lines();
        let Some(block) = self.blocks.back_mut() else {
            return;
        };
        if block.ended() {
            return;
        }
        if dropped > 0 {
            block.dropped_lines += dropped;
            block.truncated = true;
        }
        if !tail.is_empty() && !block.fullscreen {
            block.pending.push_str(&tail);
        }
        let added = flush_pending(block, redactor, true);
        let removed = trim_block(block);
        self.total_bytes = self
            .total_bytes
            .saturating_add(added)
            .saturating_sub(removed);
        block.ended_at = Some(util::now());
        block.exit_code = code;
        let id = block.id.clone();
        let bytes = block.text.len();
        tracing::debug!(
            thread_id = %self.thread_id,
            terminal_id = %self.terminal_id,
            block_id = %id,
            bytes,
            exit_code = ?code,
            "terminal command block ended"
        );
        enforce_total(&mut self.blocks, &mut self.total_bytes);
    }

    fn fullscreen(&mut self, on: bool) {
        self.alternate_screen = on;
        if !on {
            return;
        }
        self.cleaner = TextCleaner::default();
        let Some(block) = self.blocks.back_mut() else {
            return;
        };
        if block.ended() {
            return;
        }
        let cleared = block.text.len();
        block.fullscreen = true;
        block.text.clear();
        block.pending.clear();
        block.dropped_bytes = block.dropped_bytes.saturating_add(cleared);
        block.dropped_lines = 0;
        block.truncated = false;
        self.total_bytes = self.total_bytes.saturating_sub(cleared);
    }
}

/// Appends the redacted part of a block's pending text that is safe to keep. The
/// last line stays pending until it is complete or the command ends, so a secret
/// printed across two reads is seen whole; spans that touch the end wait too.
fn flush_pending(block: &mut Block, redactor: &Redactor, force: bool) -> usize {
    if block.pending.is_empty() {
        return 0;
    }
    let commit = if force {
        block.pending.len()
    } else {
        let mut candidate = last_line_start(&block.pending);
        for span in redactor.spans(&block.pending) {
            if span.end == block.pending.len() {
                candidate = candidate.min(span.start);
            } else if span.start < candidate && span.end > candidate {
                candidate = span.start;
            }
        }
        while candidate > 0 && !block.pending.is_char_boundary(candidate) {
            candidate -= 1;
        }
        candidate
    };
    if commit == 0 {
        return 0;
    }
    let head = block.pending[..commit].to_owned();
    let redacted = redact(redactor, &head);
    block.pending.drain(..commit);
    block.text.push_str(&redacted);
    redacted.len()
}

/// Where the last line starts, so it can stay pending for a cross-chunk secret.
/// A trailing newline still belongs to the line before it.
fn last_line_start(text: &str) -> usize {
    let end = if text.ends_with('\n') {
        text.len() - 1
    } else {
        text.len()
    };
    text[..end].rfind('\n').map_or(0, |at| at + 1)
}

/// Drops the top of a block's text at a line boundary when it is over the cap.
/// Returns the bytes removed.
fn trim_block(block: &mut Block) -> usize {
    if block.text.len() <= MAX_BLOCK_BYTES {
        return 0;
    }
    let mut cut = block.text.len() - MAX_BLOCK_BYTES;
    while cut < block.text.len() && !block.text.is_char_boundary(cut) {
        cut += 1;
    }
    if let Some(newline) = block.text[cut..].find('\n') {
        cut += newline + 1;
    }
    let dropped = block.text[..cut].matches('\n').count();
    block.dropped_lines += dropped;
    block.dropped_bytes += cut;
    block.truncated = true;
    block.text.drain(..cut);
    cut
}

/// Drops the oldest finished blocks until the terminal fits. A running block is
/// never dropped.
fn enforce_total(blocks: &mut VecDeque<Block>, total: &mut usize) {
    while *total > MAX_TOTAL_BYTES {
        let Some(at) = blocks.iter().position(Block::ended) else {
            break;
        };
        let Some(removed) = blocks.remove(at) else {
            break;
        };
        *total = total.saturating_sub(removed.text.len());
    }
}

fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.matches('\n').count() + usize::from(!text.ends_with('\n'))
}

/// The one place the rule set is applied to block text. The command and every
/// output page go through here; nothing else calls the Redactor for blocks.
fn redact(redactor: &Redactor, text: &str) -> String {
    redactor.dotted(text)
}

fn list_cursor(seq: u64) -> String {
    format!("{LIST_CURSOR}{seq}")
}

fn parse_list_cursor(cursor: &str) -> Result<u64> {
    cursor
        .strip_prefix(LIST_CURSOR)
        .and_then(|rest| rest.parse::<u64>().ok())
        .ok_or_else(bad_cursor)
}

fn output_cursor(block: &str, offset: usize) -> String {
    format!("{OUTPUT_CURSOR}{block}:{offset}")
}

fn parse_output_cursor(cursor: &str, block: &str) -> Result<usize> {
    let rest = cursor.strip_prefix(OUTPUT_CURSOR).ok_or_else(bad_cursor)?;
    let (id, offset) = rest.rsplit_once(':').ok_or_else(bad_cursor)?;
    if id != block {
        return Err(bad_cursor());
    }
    offset.parse::<usize>().map_err(|_| bad_cursor())
}

fn bad_cursor() -> CoreError {
    CoreError::Refused {
        status: 422,
        code: "bad_cursor",
        message: "That cursor is not one the service made.".into(),
        detail: serde_json::Value::Null,
    }
}

fn cursor_expired() -> CoreError {
    CoreError::Refused {
        status: 410,
        code: "cursor_expired",
        message: "That cursor points at output the service dropped. Start again from the top."
            .into(),
        detail: serde_json::Value::Null,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn plain() -> TerminalBlocks {
        TerminalBlocks::new(Some(Arc::new(Redactor::default())), "thread", "main")
    }

    fn redacting() -> TerminalBlocks {
        TerminalBlocks::new(
            Some(Arc::new(Redactor::with(&BTreeSet::new(), Vec::new()))),
            "thread",
            "main",
        )
    }

    fn push_command(
        blocks: &TerminalBlocks,
        command: Option<&str>,
        output: &str,
        code: Option<i32>,
    ) {
        let mut chunk = Vec::new();
        if let Some(command) = command {
            chunk.extend_from_slice(b"\x1b]633;E;");
            chunk.extend_from_slice(command.as_bytes());
            chunk.push(0x07);
        }
        chunk.extend_from_slice(b"\x1b]133;C\x07");
        chunk.extend_from_slice(output.as_bytes());
        match code {
            Some(code) => chunk.extend_from_slice(format!("\x1b]133;D;{code}\x07").as_bytes()),
            None => chunk.extend_from_slice(b"\x1b]133;D\x07"),
        }
        blocks.push(&chunk);
    }

    fn ids(blocks: &TerminalBlocks) -> Vec<String> {
        lock(&blocks.inner)
            .blocks
            .iter()
            .map(|block| block.id.clone())
            .collect()
    }

    fn text_of(blocks: &TerminalBlocks, id: &str) -> String {
        blocks.output(id, None, MAX_OUTPUT_BYTES).unwrap().text
    }

    #[test]
    fn five_hundred_blocks_is_the_cap_and_the_oldest_goes_whole() {
        let blocks = plain();
        for index in 0..(MAX_BLOCKS + 1) {
            push_command(&blocks, None, &format!("out-{index}\n"), Some(index as i32));
        }
        assert_eq!(blocks.count(), MAX_BLOCKS);
        let inner = lock(&blocks.inner);
        let first = inner.blocks.front().unwrap();
        assert_eq!(first.exit_code, Some(1), "block 0 was dropped");
        assert_eq!(first.text, "out-1\n");
        assert_eq!(inner.blocks.back().unwrap().exit_code, Some(500));
    }

    #[test]
    fn a_blocks_top_is_dropped_at_the_byte_cap_and_counted() {
        let blocks = plain();
        let line = format!("{}\n", "x".repeat(99));
        let body = line.repeat((MAX_BLOCK_BYTES / line.len()) + 5_000);
        push_command(&blocks, None, &format!("FIRSTLINE\n{body}"), Some(0));
        let inner = lock(&blocks.inner);
        let block = inner.blocks.front().unwrap();
        assert!(block.truncated);
        assert!(block.dropped_lines >= 1);
        assert!(!block.text.contains("FIRSTLINE"));
        assert!(block.text.len() <= MAX_BLOCK_BYTES);
        assert!(block.pending.is_empty(), "the last line is flushed at D");
    }

    #[test]
    fn the_terminal_total_drops_oldest_blocks_but_never_a_running_one() {
        let blocks = plain();
        let line = format!("{}\n", "y".repeat(99));
        let body = line.repeat((MAX_BLOCK_BYTES / line.len()) + 5_000);
        for _ in 0..9 {
            push_command(&blocks, None, &body, Some(0));
        }
        {
            let inner = lock(&blocks.inner);
            assert!(inner.blocks.len() < 9);
            assert!(inner.total_bytes <= MAX_TOTAL_BYTES);
            assert!(inner.blocks.front().is_none_or(|block| block.seq > 0));
        }
        blocks.push(b"\x1b]133;C\x07");
        blocks.push(body.as_bytes());
        let inner = lock(&blocks.inner);
        assert!(inner.blocks.back().is_some_and(|block| !block.ended()));
        assert!(
            inner.blocks.iter().any(|block| !block.ended()),
            "a running block is never dropped"
        );
        assert!(inner.total_bytes <= MAX_TOTAL_BYTES);
    }

    #[test]
    fn the_list_pages_backwards_and_is_oldest_first_within_a_page() {
        let blocks = plain();
        for index in 0..5 {
            push_command(&blocks, None, &format!("out-{index}\n"), Some(index));
        }
        let newest = blocks.list(None, 2).unwrap();
        assert_eq!(newest.blocks.len(), 2);
        assert_eq!(newest.blocks[0].exit_code, Some(3));
        assert_eq!(newest.blocks[1].exit_code, Some(4));
        assert_eq!(newest.limits.max_blocks, MAX_BLOCKS);
        let older = blocks.list(newest.next_before.as_deref(), 2).unwrap();
        assert_eq!(
            older
                .blocks
                .iter()
                .map(|block| block.exit_code)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(2)]
        );
        let oldest = blocks.list(older.next_before.as_deref(), 2).unwrap();
        assert_eq!(oldest.blocks.len(), 1);
        assert_eq!(oldest.blocks[0].exit_code, Some(0));
        assert!(oldest.next_before.is_none());
    }

    #[test]
    fn output_pages_never_split_a_character_and_prefer_lines() {
        let blocks = plain();
        push_command(&blocks, None, "one\ntwo\n\u{e9}\u{e9}\u{e9}\n", Some(0));
        let id = ids(&blocks).remove(0);
        let first = blocks.output(&id, None, 6).unwrap();
        assert_eq!(first.text, "one\n");
        assert_eq!(first.total_lines, Some(3));
        assert_eq!(first.dropped_lines, 0);
        let second = blocks.output(&id, first.next.as_deref(), 6).unwrap();
        assert_eq!(second.text, "two\n");
        let third = blocks.output(&id, second.next.as_deref(), 6).unwrap();
        assert_eq!(third.text, "\u{e9}\u{e9}\u{e9}");
        let fourth = blocks.output(&id, third.next.as_deref(), 6).unwrap();
        assert_eq!(fourth.text, "\n");
        assert!(fourth.next.is_none());
        assert_eq!(
            format!("{}{}{}{}", first.text, second.text, third.text, fourth.text),
            "one\ntwo\n\u{e9}\u{e9}\u{e9}\n"
        );
    }

    #[test]
    fn a_cursor_into_dropped_text_expires_and_a_malformed_one_is_refused() {
        let blocks = plain();
        let line = format!("{}\n", "z".repeat(99));
        let body = line.repeat((MAX_BLOCK_BYTES / line.len()) + 5_000);
        push_command(&blocks, None, &body, Some(0));
        let id = ids(&blocks).remove(0);
        assert!(lock(&blocks.inner).blocks.front().unwrap().dropped_bytes > 0);
        let expired = blocks
            .output(&id, Some(&format!("v1:{id}:0")), 100)
            .unwrap_err();
        assert_eq!((expired.status(), expired.code()), (410, "cursor_expired"));
        let malformed = blocks.output(&id, Some("nope"), 100).unwrap_err();
        assert_eq!((malformed.status(), malformed.code()), (422, "bad_cursor"));
        let wrong_block = blocks
            .output(&id, Some("v1:someone-else:0"), 100)
            .unwrap_err();
        assert_eq!(wrong_block.code(), "bad_cursor");
        let list_bad = blocks.list(Some("v1:nope"), 10).unwrap_err();
        assert_eq!(list_bad.code(), "bad_cursor");
        let unknown = blocks.output("no-such-block", None, 10).unwrap_err();
        assert_eq!((unknown.status(), unknown.code()), (404, "not_found"));
    }

    #[test]
    fn a_list_cursor_before_the_oldest_kept_block_expires() {
        let blocks = plain();
        for _ in 0..(MAX_BLOCKS + 1) {
            push_command(&blocks, None, "x\n", Some(0));
        }
        let expired = blocks.list(Some("v1:0"), 10).unwrap_err();
        assert_eq!((expired.status(), expired.code()), (410, "cursor_expired"));
    }

    #[test]
    fn a_secret_printed_across_two_reads_is_never_stored_or_answered() {
        let blocks = redacting();
        let secret = format!("sk-{}", "a1".repeat(12));
        let (left, right) = secret.split_at(10);
        let mut first = Vec::new();
        first.extend_from_slice(b"\x1b]633;E;echo ");
        first.extend_from_slice(secret.as_bytes());
        first.extend_from_slice(b"\x07\x1b]133;C\x07leak ");
        first.extend_from_slice(left.as_bytes());
        blocks.push(&first);
        let mut second = Vec::new();
        second.extend_from_slice(right.as_bytes());
        second.extend_from_slice(b"\n\x1b]133;D;0\x07");
        blocks.push(&second);
        let list = blocks.list(None, 10).unwrap();
        assert!(!list.blocks[0].command.as_deref().unwrap().contains(&secret));
        let output = blocks
            .output(&list.blocks[0].id, None, MAX_OUTPUT_BYTES)
            .unwrap();
        assert!(!output.text.contains(&secret));
        assert!(output.text.contains('\u{2022}'));
    }

    #[test]
    fn fullscreen_output_is_not_kept_even_when_the_screen_starts_later() {
        let blocks = plain();
        blocks.push(b"\x1b]133;C\x07before\n\x1b[?1049ha full screen\n\x1b]133;D;0\x07");
        let list = blocks.list(None, 10).unwrap();
        let block = &list.blocks[0];
        assert!(block.fullscreen);
        assert_eq!(block.output_bytes, 0);
        assert_eq!(block.exit_code, Some(0));
        let output = blocks.output(&block.id, None, MAX_OUTPUT_BYTES).unwrap();
        assert_eq!(output.text, "");
        assert!(output.next.is_none());
    }

    #[test]
    fn a_prompt_or_a_new_command_ends_a_block_without_a_code() {
        let blocks = plain();
        blocks.push(b"\x1b]133;C\x07first\n\x1b]133;A\x07\x1b]133;C\x07second\n\x1b]133;D;0\x07");
        let list = blocks.list(None, 10).unwrap();
        assert_eq!(list.blocks.len(), 2);
        assert!(list.blocks[0].ended_at.is_some());
        assert_eq!(list.blocks[0].exit_code, None);
        assert_eq!(list.blocks[1].exit_code, Some(0));
    }

    #[test]
    fn no_marks_means_no_blocks_and_never_an_error() {
        let blocks = plain();
        blocks.push(b"just a prompt\nand some output\n");
        blocks.finish();
        let list = blocks.list(None, 10).unwrap();
        assert!(list.blocks.is_empty());
        assert!(list.next_before.is_none());
        assert_eq!(list.limits.max_total_bytes, MAX_TOTAL_BYTES);
    }

    #[test]
    fn closing_a_terminal_ends_its_running_block_without_a_code() {
        let blocks = plain();
        blocks.push(b"\x1b]133;C\x07partial");
        blocks.finish();
        let list = blocks.list(None, 10).unwrap();
        assert_eq!(list.blocks.len(), 1);
        assert!(list.blocks[0].ended_at.is_some());
        assert_eq!(list.blocks[0].exit_code, None);
        assert_eq!(text_of(&blocks, &list.blocks[0].id), "partial");
        blocks.push(b"\x1b]133;C\x07after close");
        assert_eq!(blocks.count(), 1, "the recorder keeps nothing after close");
    }

    #[test]
    fn without_a_redactor_nothing_is_kept_and_the_list_is_empty() {
        let blocks = TerminalBlocks::new(None, "thread", "main");
        blocks.push(b"\x1b]133;C\x07secret sk-aaaaaaaaaaaaaaaaaaaaaaaa\n\x1b]133;D;0\x07");
        let list = blocks.list(None, 10).unwrap();
        assert!(list.blocks.is_empty());
        assert!(blocks.output("anything", None, 10).is_err());
    }

    #[test]
    fn output_cuts_at_a_character_boundary() {
        let blocks = plain();
        push_command(&blocks, None, "\u{e9}\u{e9}\u{e9}\n", Some(0));
        let id = ids(&blocks).remove(0);
        let first = blocks.output(&id, None, 3).unwrap();
        assert_eq!(first.text, "\u{e9}");
        let second = blocks.output(&id, first.next.as_deref(), 3).unwrap();
        assert_eq!(second.text, "\u{e9}");
        let third = blocks.output(&id, second.next.as_deref(), 3).unwrap();
        assert_eq!(third.text, "\u{e9}\n");
        assert!(third.next.is_none());
        let tiny = blocks.output(&id, None, 1).unwrap();
        assert_eq!(tiny.text, "\u{e9}");
        assert!(tiny.next.is_some());
    }

    #[test]
    fn a_running_blocks_output_grows_and_an_earlier_cursor_stays_valid() {
        let blocks = plain();
        blocks.push(b"\x1b]133;C\x07one\ntwo\nthree\nfour\n");
        let id = ids(&blocks).remove(0);
        let first = blocks.output(&id, None, 5).unwrap();
        assert_eq!(first.text, "one\n");
        assert_eq!(first.total_lines, None);
        let cursor = first.next.expect("a page before the running end");
        blocks.push(b"five\n");
        let second = blocks.output(&id, Some(&cursor), 4).unwrap();
        assert_eq!(second.text, "two\n");
        assert_eq!(second.total_lines, None);
    }
}
