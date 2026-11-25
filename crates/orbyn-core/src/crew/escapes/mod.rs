//! The escape log (slice 4k; docs/crew/escapes.md): merges that got past review and checks and were later found
//! defective. A revert is a **possible** escape at once and **confirmed** only when a person confirms; possible, confirmed and
//! dismissed are always separate numbers. The log is **display only**: nothing here feeds the circuit breaker, the Router or any
//! gate. No GET writes; the scan (PR 3, [`scan`]) writes possible escapes only, from git read-only, and runs on the service's own tick. Names come back through the same scrub the trust record's do. The writes (mark, confirm, dismiss, the gap)
//! are a person's alone ([`person`], PR 2); the `revert` source is written by the schema itself (migration 51). A person's check
//! (PR 4, [`check`]) is proposed, accepted, retried and withdrawn by a person alone; only accept starts a worker, which may add test
//! files only ([`check_paths`]), and the proof and the merge queue follow.
//!
//! The stable read API for other modules (the reviewer replay's `EscapeSource`, slice 4n) is on `Store`, see `store::crew_escapes`.
//!
//! The report's own read is still not built, so `escape-dashboard` stays drawn; one [per-area, per-engine counts](counts) read is,
//! and it decides nothing (the Router reads no escape count in v1).

mod check;
mod check_brief;
pub mod check_paths;
mod check_proof;
mod counts;
mod person;
mod possible;
mod scan;
mod thresholds;
mod view;

pub use check::escape_check_blocks_queue;
#[cfg(test)]
pub(crate) use check::escape_check_change;
#[cfg(all(test, target_os = "macos"))]
pub(crate) use check_proof::confined as check_proof_confined_for_test;
pub use counts::ESCAPE_COUNT_ENGINES_MAX;
pub use scan::{
    COMMITS_PER_MERGE, CURSOR_KEY, GIT_CALLS_PER_SCAN, Limits, MERGES_PER_SCAN, SCAN_SECONDS,
    ScanOutcome,
};
pub use thresholds::{
    DEFAULTS, THRESHOLDS_KEY, escape_thresholds, stored_thresholds, validate_thresholds,
};
