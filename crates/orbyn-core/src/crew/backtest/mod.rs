//! Reviewer backtest (Preview, slice 4n): how well each review lane would have caught the escapes of the last
//! 180 days. This part is pure: no store, no process, no file and no model. The facts come in, a score goes out.
//!
//! A backtest starts only when a person chooses it; nothing here starts anything. Budgets are tokens per engine and
//! are never added up, and no money appears anywhere. The lane picker may show these counts but never lets them
//! change the router's choice.

// A person starts a backtest through the routes (`api`), and `api::crew_backtest_start` is the only code that reaches
// `replay::begin` and `replay::finish`; nothing calls them on a timer, at startup or from any scheduler (see
// `replay_tests`). `replay::run` (begin and finish together) is for the tests.
mod api;
mod history;
mod outcomes;
mod replay;
#[cfg(test)]
mod replay_tests;
mod score;

#[cfg(any(test, feature = "test-support"))]
pub use api::ScriptedRunner;
pub(crate) use api::Slot;
pub use history::{HISTORY_DAYS, OUTCOME_DAYS, PastPr, eligible_prs};
pub use outcomes::{Escape, EscapeSource, FixtureEscapes, NoEscapeLog, NotMeasured, Window};
#[cfg(test)]
pub(crate) use replay::run;
pub use replay::{
    BacktestReport, BacktestRequest, LaneKind, LaneReplay, LaneReport, LaneResult, LaneRunner,
    LaneSpec, ReplayFailed, ReplayedFinding, STOPPED_BY_PERSON, report,
};
pub use score::{
    LaneScore, Outcome, PRECISION_MIN_FINDINGS, PastFinding, RECALL_MIN_ESCAPES, Rate, from_counts,
    match_escape, rates, score_lanes,
};
