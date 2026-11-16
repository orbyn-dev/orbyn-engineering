//! Review capacity (Preview): how many merges wait for a person, when a new low-priority start from Crew
//! pauses, and which low-risk merges are sampled for a closer look. Everything here is pure: no store, no
//! process, no file and no model. The facts come in, a decision goes out. `provenance` is the exception: it runs git
//! for a merged range and writes a local note.

// The store (`store/crew_capacity.rs`, `store/crew_provenance.rs`) uses the settings, `Priority`, `Sampling` and the
// provenance types. The start check, the sampling run and the merge hook (`provenance::record_at_merge`) still call
// nothing here; the PRs that wire them remove this allow.
#![allow(dead_code, unused_imports)]

mod backpressure;
mod provenance;
mod sampling;

pub use backpressure::{StartDecision, start_allowed, waiting_count};
pub use provenance::{
    Author, AuthoredHunk, Hunk, NOTE_REF, Recorded, attribute, hunks_of, parse_diff,
    record_at_merge, trace_note,
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

/// A person's capacity settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacitySettings {
    /// Waiting High and Medium merges allowed before a new low-priority Crew start pauses.
    pub limit: u32,
    pub sampling: Sampling,
}

impl Default for CapacitySettings {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            sampling: Sampling::Off,
        }
    }
}
