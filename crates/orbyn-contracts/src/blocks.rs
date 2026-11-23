//! Command blocks over a terminal: one entry per command a shell's OSC 133 marks
//! delimit. The service keeps them in memory, redacted, and the drawer reads them
//! a page at a time (docs/command-blocks.md).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// One command in a terminal's block list. The web keeps `id` as a map key, never as an object key or a path.
pub struct CommandBlock {
    /// Service-made, unique within the terminal and never reused.
    pub id: String,
    /// The command line, redacted and at most 4096 characters, or null when the shell sent no line.
    pub command: Option<String>,
    /// RFC 3339 UTC, when output started (the `C` mark).
    pub started_at: String,
    /// When the `D` mark arrived; null while the command runs.
    pub ended_at: Option<String>,
    /// From OSC 133 `D;<code>`; null while running or when the shell sent no code.
    pub exit_code: Option<i32>,
    /// The working directory at the `C` mark when the shell reports it.
    pub cwd: Option<String>,
    /// Bytes of output kept for the block, after redaction and the caps.
    pub output_bytes: usize,
    /// Bytes were dropped from the top of this block's output.
    pub truncated: bool,
    /// Lines dropped from the top; 0 unless `truncated`.
    pub dropped_lines: usize,
    /// A program that used the alternate screen: its output is not kept.
    pub fullscreen: bool,
}
}

wire! {
/// The block limits the service enforces, read by the drawer instead of hard-coded.
pub struct BlockLimits {
    pub max_blocks: usize,
    pub max_block_bytes: usize,
    pub max_total_bytes: usize,
}
}

wire! {
/// One page of a terminal's blocks, oldest first.
pub struct BlockList {
    /// Oldest first.
    pub blocks: Vec<CommandBlock>,
    /// Opaque cursor for the page before this one, or null at the start.
    pub next_before: Option<String>,
    pub limits: BlockLimits,
}
}

wire! {
/// One page of a block's kept output, oldest first.
pub struct BlockOutputPage {
    /// Plain text, oldest first.
    pub text: String,
    /// Opaque cursor for the next page, or null on the last.
    pub next: Option<String>,
    /// All kept lines, or null while the command still runs.
    pub total_lines: Option<usize>,
    /// The block's dropped lines, repeated so one answer says what is missing.
    pub dropped_lines: usize,
}
}
