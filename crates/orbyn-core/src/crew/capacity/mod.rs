//! Review capacity (Preview): how many merges wait for a person, when a new low-priority start from Crew
//! pauses, and which low-risk merges are sampled for a closer look. Everything here is pure: no store, no
//! process, no file and no model. The facts come in, a decision goes out. `provenance` is the exception: it runs git
//! for a merged range and writes a local note.

// The start check and the merge hook (`hooks`) call the settings, `Priority`, the backpressure decision,
// `provenance::record_at_merge` and the sampling run (`batch`).
#![allow(dead_code, unused_imports)]

mod api;
mod backpressure;
mod batch;
mod coverage;
mod hooks;
mod provenance;
mod sampling;

pub use backpressure::{StartDecision, start_allowed, waiting_count};
pub(crate) use batch::{BATCH_SIZE, close_batch, sample_after_merge};
pub use coverage::{AreaCoverage, CoverageMap, EngineCoverage, MergeProvenance, coverage};
pub(crate) use hooks::{FIX_WAITING, paused_by_capacity, record_provenance, retry_provenance};
pub(crate) use provenance::attribute_by_blame;
pub use provenance::{
    Author, AuthoredHunk, Hunk, NOTE_REF, Recorded, hunks_of, parse_blame, parse_diff,
    record_at_merge, split_by_author, trace_note,
};
pub use sampling::{Seeded, choose, plan_size};

/// The waiting merges (High and Medium risk) a person may have before Crew pauses new low-priority starts.
pub const DEFAULT_LIMIT: u32 = 8;

/// How much of the low-risk work is picked for a closer look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sampling {
    /// No sampling. The default.
    #[default]
    Off,
    /// One merge in four, rounded up.
    OneInFour,
}

/// How urgent a task is. Only `Low` can be paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    High,
    Medium,
    Low,
}

/// Which Crew trigger a held start came from. A stored value, so the names never change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PausedKind {
    /// A fix for a person's merge Comment.
    CommentFix,
    /// A fix for a confirmed review finding.
    ReviewFix,
    /// The worker of a check created from a confirmed escape.
    EscapeCheck,
    /// A background crew run. Reserved: no background trigger creates a crew task yet, so nothing stores this today.
    Background,
    /// A Crew-started launch that named no trigger. A bug in the caller, kept visible rather than labelled as another kind.
    Unknown,
}

impl PausedKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommentFix => "comment_fix",
            Self::ReviewFix => "review_fix",
            Self::EscapeCheck => "escape_check",
            Self::Background => "background",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "comment_fix" => Self::CommentFix,
            "review_fix" => Self::ReviewFix,
            "escape_check" => Self::EscapeCheck,
            "background" => Self::Background,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

/// The trigger of a Crew-started start: its kind and a stable key (an id or hash, never text), so the same trigger
/// refused again is one held row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PausedTrigger {
    pub kind: PausedKind,
    pub key: String,
}

/// A person's capacity settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacitySettings {
    /// Waiting High and Medium merges allowed before a new low-priority Crew start pauses.
    pub limit: u32,
    pub sampling: Sampling,
    /// Whether a new low-priority Crew start may pause at all. Off by default; only a person turns it on.
    pub pause_enabled: bool,
}

impl Default for CapacitySettings {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            sampling: Sampling::Off,
            pause_enabled: false,
        }
    }
}
