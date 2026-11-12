//! The merge queue, the after-merge Review queue and the watch on main (slice 4f, part 2). See the [README](../README.md).
//! Local only: a merge moves a local branch in a repository where no one has it checked out, and nothing is pushed.

mod api;
mod gitops;
mod guard;
mod holds;
mod queue;
mod review_queue;
mod risk;
mod watch;

pub use api::{
    Answering, COMMENT_MAX, DIGEST_HOURS_MAX, OFFSET_MAX, PAGE_DEFAULT, PAGE_MAX, Page, valid_id,
};
pub use guard::{check_words, owed_lanes, sensitive};
pub use holds::lane_ran;
pub use queue::{MergeOutcome, MergeTarget};
pub use review_queue::{ReviewAnswer, ReviewItem};
pub use risk::{DiffFacts, Rate, RecentTotals, Risk, Scored, area_of, areas_of, score};
pub use watch::WatchOutcome;

/// The review window of a merge, in seconds: high 4 hours, medium 24 hours, low batched with no deadline
/// (docs/crew.md, "Autopilot decisions").
pub fn review_window(risk: Risk) -> Option<i64> {
    match risk {
        Risk::High => Some(4 * 3600),
        Risk::Medium => Some(24 * 3600),
        Risk::Low => None,
    }
}
