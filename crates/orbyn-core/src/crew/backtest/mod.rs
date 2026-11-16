//! Reviewer backtest (Preview, slice 4n): how well each review lane would have caught the escapes of the last
//! 180 days. This part is pure: no store, no process, no file and no model. The facts come in, a score goes out.
//!
//! A backtest starts only when a person chooses it; nothing here starts anything. Budgets are tokens per engine and
//! are never added up, and no money appears anywhere. The lane picker may show these counts but never lets them
//! change the router's choice.

// Slice 4n PR 2 (the store and the replay) and PR 3 (the routes) call these; the PR that wires them removes this allow.
#![allow(dead_code, unused_imports)]

mod history;
mod outcomes;
mod score;

pub use history::{HISTORY_DAYS, OUTCOME_DAYS, PastPr, eligible_prs};
pub use outcomes::{Escape, EscapeSource, FixtureEscapes, NoEscapeLog, NotMeasured, Window};
pub use score::{
    LaneScore, Outcome, PRECISION_MIN_FINDINGS, PastFinding, RECALL_MIN_ESCAPES, Rate,
    match_escape, rates, score_lanes,
};
