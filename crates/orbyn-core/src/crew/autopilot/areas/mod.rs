//! Autonomy by area (slice 4j). A circuit breaker per area: an area (the first two folders of the paths merges touched,
//! [`area_of`](super::area_of)) of a project whose recent merges were reverted too often is recorded as `demoted`, and returns when
//! it has stayed clean. **Enforced at the merge gate** (`effective`): a lowered area holds a Full crew to `On` for the diffs that
//! touch it, and a person can keep the ceiling or set the area to `Off` (`person`). The approval chain and the risk tier do not read
//! an area. Earning a higher mode and the screens are not built.
//!
//! The thresholds below are Engineering's proposal and are served as such (`proposed: true`). The 60-day revert rate in
//! `merge/risk` is separate and keeps raising the risk tier. A demotion lowers an area from `Full` to `On` only; a ceiling of `On` or
//! `Off` is never changed, and the record is still shown. Escapes (a defect found after a merge that no revert shows) are not
//! measured: there is no source for them, and no count includes them.
//!
//! The record is counted from `crew_merges` each time, never stored. Each breaker write adds its `crew_area_events` row and then
//! changes `crew_area_modes`, in one transaction. The breaker acts when a revert passes through `revert_merge` and checks the way back
//! when a merge is recorded or queued, so a return waits for at most one merge.

mod breaker;
mod decide;
mod effective;
mod person;
mod retry;
mod view;

/// The window the breaker reads, in days. Proposed.
pub const WINDOW_DAYS: i64 = 14;
/// Merges the window needs before the breaker decides anything. Proposed.
pub const MIN_MERGES: u32 = 5;
/// Reverted merges in the window that lower an area. Proposed.
pub const TRIP_REVERTS: u32 = 2;
/// Days without a revert before a lowered area can return. Proposed.
pub const RECOVERY_DAYS: i64 = 14;
/// Merges since the lowering, none reverted, before it can return. Proposed.
pub const RECOVERY_MERGES: u32 = 5;
