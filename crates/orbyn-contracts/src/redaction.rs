//! Secret redaction settings: which formats are hidden, the person's own patterns, and the
//! projects whose agents read values as they are (Settings › Secrets).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// How a person's pattern is read.
pub enum PatternKind {
    /// Plain text. `*` stands for any run of characters that are not spaces.
    Text,
    /// A regular expression.
    Regex,
}
}

wire_enum! {
/// Where a person's pattern is looked for.
pub enum PatternScope {
    Anywhere,
    /// Only the value of `NAME=value` assignments.
    EnvironmentValues,
}
}

wire! {
/// One recognised format and whether it is hidden.
pub struct RedactionFormat {
    pub id: String,
    pub label: String,
    /// What a value of this format looks like, with the secret part dotted out.
    pub example: String,
    pub enabled: bool,
}
}

wire! {
/// A pattern as the list shows it. The text of the pattern is not here: a pattern can be a secret
/// itself, so only the edit dialog the person opens reads it (`RedactionPatternDetail`).
pub struct RedactionPattern {
    pub id: String,
    pub name: String,
    pub kind: PatternKind,
    pub applies_to: PatternScope,
}
}

wire! {
pub struct RedactionPatternDetail {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub kind: PatternKind,
    pub applies_to: PatternScope,
}
}

wire! {
/// A project whose agents read values as they are.
pub struct RedactionProject {
    pub id: String,
    pub name: String,
}
}

wire! {
/// The limits a pattern is held to, so the screen can say them before the service does.
pub struct RedactionLimits {
    pub max_patterns: u32,
    pub max_name_chars: u32,
    pub max_pattern_chars: u32,
    pub max_sample_chars: u32,
    pub max_context_lines: u32,
}
}

wire! {
/// Settings › Secrets. Nothing here is synced between a person's devices.
pub struct RedactionSettings {
    /// Values are masked in terminal blocks.
    pub terminal: bool,
    /// Values are replaced by a label before an agent reads them. A project can switch this off alone.
    pub agents: bool,
    /// The Crew ledger and every Crew object hold no secret, whatever this page says. Always true.
    pub crew_locked: bool,
    pub formats: Vec<RedactionFormat>,
    pub patterns: Vec<RedactionPattern>,
    pub projects_off: Vec<RedactionProject>,
    pub limits: RedactionLimits,
}
}

wire! {
/// A change to the two places values are hidden. A field left out stays as it is.
pub struct RedactionSurfaceChange {
    #[serde(default)]
    #[ts(optional)]
    pub terminal: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub agents: Option<bool>,
}
}

wire! {
pub struct RedactionFormatChange {
    pub enabled: bool,
}
}

wire! {
/// `redaction: false` makes agents in the project read values as they are.
pub struct RedactionProjectChange {
    pub redaction: bool,
}
}

wire! {
/// A pattern to save. A pattern that would hide nearly everything is saved only with `accept_broad`.
pub struct RedactionPatternInput {
    pub name: String,
    pub pattern: String,
    pub kind: PatternKind,
    pub applies_to: PatternScope,
    #[serde(default)]
    #[ts(optional)]
    pub accept_broad: Option<bool>,
}
}

wire! {
/// A pattern to try before it is saved. `lines` are recent terminal lines to measure it against.
pub struct RedactionPatternTestInput {
    pub pattern: String,
    pub kind: PatternKind,
    pub applies_to: PatternScope,
    pub sample: String,
    #[serde(default)]
    #[ts(optional)]
    pub lines: Option<Vec<String>>,
}
}

wire! {
/// Why a pattern is refused, in plain words. `position` counts characters from 0.
pub struct RedactionPatternProblem {
    pub message: String,
    pub position: Option<u32>,
}
}

wire! {
/// A matched stretch of the sample, in characters.
pub struct RedactionSpan {
    pub start: u32,
    pub end: u32,
}
}

wire! {
pub struct RedactionPatternTest {
    /// Set when the pattern cannot be saved; the other fields are then empty.
    pub problem: Option<RedactionPatternProblem>,
    pub matches: Vec<RedactionSpan>,
    /// The sample with each matched character shown as a dot.
    pub masked: String,
    /// How many characters of the text it was tried on the pattern hides, and how many there were.
    pub hidden_chars: u32,
    pub total_chars: u32,
    /// The pattern would hide nearly everything it was tried on.
    pub broad: bool,
}
}
