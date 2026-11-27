//! Command blocks over a terminal: the OSC 133/633 parser, the text cleaner and
//! the bounded in-memory store the terminal routes read (docs/command-blocks.md).

mod parser;
mod store;
mod text;

pub use store::{
    BlockStore, ClearOutcome, DEFAULT_OUTPUT_BYTES, DEFAULT_PAGE, MAX_BLOCK_BYTES, MAX_BLOCKS,
    MAX_OUTPUT_BYTES, MAX_PAGE, MAX_TOTAL_BYTES, TerminalBlocks, limits,
};
