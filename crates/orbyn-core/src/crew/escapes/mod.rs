//! The escape log (slice 4k; docs/crew/escapes.md): merges that got past review and checks and were later found
//! defective. A revert is a **possible** escape at once and **confirmed** only when a person confirms; possible, confirmed and
//! dismissed are always separate numbers. The log is **display only**: nothing here feeds the circuit breaker, the Router or any
//! gate. No GET writes; the scan (PR 3, [`scan`]) writes possible escapes only, from git read-only, and runs on the service's own tick. Names come back through the same scrub the trust record's do. The writes (mark, confirm, dismiss, the gap)
//! are a person's alone ([`person`], PR 2); the `revert` source is written by the schema itself (migration 51).
//!
//! The stable read API for other modules (the reviewer replay's `EscapeSource`, slice 4n) is on `Store`, see `store::crew_escapes`.

mod person;
mod possible;
mod scan;
mod thresholds;
mod view;

pub use scan::{
    COMMITS_PER_MERGE, GIT_CALLS_PER_SCAN, Limits, MERGES_PER_SCAN, SCAN_SECONDS, ScanOutcome,
};
pub use thresholds::{
    DEFAULTS, THRESHOLDS_KEY, escape_thresholds, stored_thresholds, validate_thresholds,
};
