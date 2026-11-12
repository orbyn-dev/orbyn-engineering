//! The person's own patterns: how one is read, how it is checked before it is saved, and the
//! limits that keep one from slowing every screen down. The regex engine never backtracks, so a
//! pattern cannot run away; the limits bound its size, its compiled program and the text tried.

use regex::{Regex, RegexBuilder};
use regex_syntax::ast::ErrorKind as AstKind;

use orbyn_contracts::PatternKind;

pub const MAX_PATTERNS: usize = 50;
pub const MAX_NAME_CHARS: usize = 60;
pub const MAX_PATTERN_CHARS: usize = 200;
pub const MAX_SAMPLE_CHARS: usize = 4_000;
pub const MAX_CONTEXT_LINES: usize = 200;
pub const MAX_LINE_CHARS: usize = 2_000;
/// The least a text pattern must spell out besides its `*`, so it names something.
const MIN_TEXT_CHARS: usize = 3;
/// A compiled program over this, in bytes, is too complex to keep (the engine's default is 10 MB).
const PROGRAM_BYTES: usize = 200_000;
const LAZY_DFA_BYTES: usize = 1_000_000;
const NEST_LIMIT: u32 = 16;
/// The most text one scan reads at once; longer text is read in windows (see `Redactor::spans`).
pub const MAX_APPLY_BYTES: usize = 1 << 20;
/// What two windows share. A match up to this long is found whole wherever it falls.
pub const WINDOW_OVERLAP: usize = 64 * 1024;

/// Why a pattern is refused, in plain words. `position` counts characters from 0.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub message: String,
    pub position: Option<usize>,
}

impl Problem {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            position: None,
        }
    }
}

fn char_at(pattern: &str, byte: usize) -> usize {
    pattern
        .char_indices()
        .take_while(|(at, _)| *at < byte)
        .count()
}

fn words(kind: &AstKind, pattern: &str, byte: usize) -> String {
    let at = char_at(pattern, byte);
    match kind {
        AstKind::GroupUnclosed => format!("the group opened at position {at} is not closed"),
        AstKind::GroupUnopened => {
            format!("the closing bracket at position {at} has no group to close")
        }
        AstKind::ClassUnclosed => format!("the [ at position {at} is not closed"),
        AstKind::RepetitionMissing => format!("there is nothing to repeat at position {at}"),
        AstKind::EscapeUnexpectedEof => "the pattern ends with a lone backslash".to_owned(),
        AstKind::UnsupportedLookAround => {
            format!("look-around at position {at} is not supported; use a plain pattern")
        }
        AstKind::UnsupportedBackreference => {
            format!("a back-reference at position {at} is not supported")
        }
        other => format!("{other} at position {at}"),
    }
}

fn syntax_problem(error: &regex_syntax::Error, pattern: &str) -> Problem {
    let (message, span) = match error {
        regex_syntax::Error::Parse(parse) => (
            words(parse.kind(), pattern, parse.span().start.offset),
            Some(parse.span().start.offset),
        ),
        regex_syntax::Error::Translate(translate) => (
            format!(
                "{} at position {}",
                translate.kind(),
                char_at(pattern, translate.span().start.offset)
            ),
            Some(translate.span().start.offset),
        ),
        _ => ("the pattern is not understood".to_owned(), None),
    };
    Problem {
        message: format!("This isn’t a valid pattern: {message}."),
        position: span.map(|byte| char_at(pattern, byte)),
    }
}

/// A text pattern as a regular expression: every character literal, `*` any run of non-spaces.
fn text_source(pattern: &str) -> Result<String, Problem> {
    let spelled = pattern.chars().filter(|c| *c != '*').count();
    if spelled < MIN_TEXT_CHARS {
        return Err(Problem::plain(format!(
            "Spell out at least {MIN_TEXT_CHARS} characters besides *, so the pattern names something."
        )));
    }
    Ok(pattern
        .split('*')
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\S*"))
}

/// Checks a pattern and compiles it. The checks run in the order a person would fix them.
pub fn compile(kind: PatternKind, pattern: &str) -> Result<Regex, Problem> {
    if pattern.trim().is_empty() {
        return Err(Problem::plain("Enter a pattern."));
    }
    if pattern.chars().count() > MAX_PATTERN_CHARS {
        return Err(Problem::plain(format!(
            "A pattern holds at most {MAX_PATTERN_CHARS} characters."
        )));
    }
    let source = match kind {
        PatternKind::Text => text_source(pattern)?,
        PatternKind::Regex => {
            regex_syntax::ParserBuilder::new()
                .nest_limit(NEST_LIMIT)
                .build()
                .parse(pattern)
                .map_err(|error| syntax_problem(&error, pattern))?;
            pattern.to_owned()
        }
    };
    RegexBuilder::new(&source)
        .size_limit(PROGRAM_BYTES)
        .dfa_size_limit(LAZY_DFA_BYTES)
        .nest_limit(NEST_LIMIT)
        .build()
        .map_err(|error| match error {
            regex::Error::CompiledTooBig(_) => {
                Problem::plain("This pattern is too complex. Make it shorter or less repeated.")
            }
            // The engine's own text quotes the whole pattern, and a pattern can be a secret itself.
            _ => Problem::plain("This isn’t a valid pattern."),
        })
}

/// A name a person can read in the list: trimmed, not empty, one line.
pub fn clean_name(name: &str) -> Result<String, Problem> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Problem::plain("Give the pattern a name."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(Problem::plain(format!(
            "A name holds at most {MAX_NAME_CHARS} characters."
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(Problem::plain("A name is one line of text."));
    }
    Ok(name.to_owned())
}
