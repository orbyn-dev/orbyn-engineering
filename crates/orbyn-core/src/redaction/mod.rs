//! Secret redaction: one list of recognised formats, the person's own patterns, and the
//! settings that decide where each applies. See [README](README.md).

mod api;
mod builtin;
pub mod changes;
mod custom;
mod redactor;
mod settings;

pub use builtin::{BUILTINS, Builtin};
pub use custom::{
    MAX_CONTEXT_LINES, MAX_LINE_CHARS, MAX_NAME_CHARS, MAX_PATTERN_CHARS, MAX_PATTERNS,
    MAX_SAMPLE_CHARS, Problem, compile,
};
pub use redactor::{Custom, Redactor, Span};
pub use settings::{Stored, StoredPattern, Surface};

#[cfg(test)]
mod tests;
