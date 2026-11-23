//! The reviewer backtest (Preview, slice 4n) as the routes show it. Budgets are tokens per engine and are never added
//! up: there is no total anywhere, and no money field. A rate with no escape log behind it is not a zero: the lane says
//! `not_measured` and carries no score.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// What a replayed lane is. A builder lane does not exist: a backtest replays reviews and checks only.
pub enum BacktestLaneKind { Review, Check }
}
wire_enum! {
pub enum BacktestState { Estimated, Running, Stopped, Finished, Failed }
}
wire_enum! {
pub enum BacktestLaneState { Pending, Running, Stopped, Failed, Done }
}
wire_enum! {
/// Whether a lane has a score. `not_measured`: the escape log cannot say, so only the finding count shows.
/// `unscored`: the lane failed or has not finished.
pub enum BacktestLaneOutcome { Scored, NotMeasured, Unscored }
}

wire! {
/// A share with its sample size, so a screen can say "3 of 5". `permille` is rounded down.
pub struct BacktestRate { pub numerator: u32, pub denominator: u32, pub permille: u32 }
}
wire! {
/// One lane's counts, and its rates once there are enough of them: recall from 5 escapes, precision from 10 findings.
/// Below those only the counts show. A finding that matches no escape is unmatched, never wrong.
pub struct LaneScore {
    pub recall: Option<BacktestRate>,
    pub precision: Option<BacktestRate>,
    pub escapes_n: u32,
    pub findings_n: u32,
    pub unmatched_n: u32,
}
}

wire! {
/// A lane to replay, on one engine and model.
#[serde(deny_unknown_fields)]
pub struct BacktestLaneInput {
    pub lane: String,
    pub engine: String,
    pub model: String,
    pub kind: BacktestLaneKind,
}
}
wire! {
/// The token budget of one engine. Each engine has its own; they are never summed or pooled.
#[serde(deny_unknown_fields)]
pub struct BacktestEngineBudget {
    pub engine: String,
    #[ts(type = "number")]
    pub tokens: u64,
}
}
wire! {
/// A person's request to start a backtest. Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct BacktestStartInput {
    pub project: String,
    pub lanes: Vec<BacktestLaneInput>,
    pub budgets: Vec<BacktestEngineBudget>,
}
}

wire! {
/// How many replays one lane would make. Information only: it never changes which engine the router picks.
pub struct BacktestEstimateLane {
    pub lane: String,
    pub engine: String,
    pub model: String,
    pub replays: u32,
}
}
wire! {
/// The size of a backtest before a person starts it. Nothing runs and nothing is stored to make it.
pub struct BacktestEstimate {
    pub project: String,
    pub history_days: u32,
    pub outcome_days: u32,
    /// Merges the project recorded 30 to 180 days ago: the ones a replay may use.
    pub eligible_prs: u32,
    pub lanes: Vec<BacktestEstimateLane>,
    /// False when the escape log cannot say: it holds no escape, cannot be read whole (damaged or over its cap) or is
    /// absent. The report then shows "Not measured" for outcomes.
    pub outcomes_measured: bool,
    /// The run in progress, when there is one: only one runs at a time.
    pub running: Option<String>,
}
}

wire! {
pub struct BacktestLaneRun {
    pub lane: String,
    pub engine: String,
    pub model: String,
    /// The kind the lane was started as. Null only for a run recorded before a lane kept its kind: unknown, never
    /// guessed as a review.
    pub kind: Option<BacktestLaneKind>,
    pub state: BacktestLaneState,
    #[ts(type = "number")]
    pub tokens_used: u64,
    pub outcome: BacktestLaneOutcome,
    pub findings_n: u32,
    /// Confirmed escapes in the window that could not be matched because the files of their fix or revert could not be
    /// read. Counted here beside the score and never inside it; null unless `outcome` is `scored`.
    pub unresolved_n: Option<u32>,
    /// Present only when `outcome` is `scored`.
    pub score: Option<LaneScore>,
}
}
wire! {
/// A run and each of its lanes, as stored. `budgets` is one entry per engine and has no total.
pub struct BacktestRun {
    pub id: String,
    pub project: String,
    pub state: BacktestState,
    pub budgets: Vec<BacktestEngineBudget>,
    pub stop_reason: Option<String>,
    #[ts(type = "number")]
    pub created_unix: i64,
    #[ts(type = "number | null")]
    pub finished_unix: Option<i64>,
    pub lanes: Vec<BacktestLaneRun>,
}
}

wire! {
/// One lane of a start input: what it is, what it runs on, and the kind the run started it as. A field a run did not
/// record is null, never filled in with a guess.
pub struct BacktestStartLane {
    pub lane: String,
    pub engine: String,
    pub model: String,
    pub kind: Option<BacktestLaneKind>,
}
}
wire! {
/// The lanes and the per-engine token budgets one run was started with.
pub struct BacktestStartRun {
    pub id: String,
    pub state: BacktestState,
    pub lanes: Vec<BacktestStartLane>,
    /// One entry per engine, never summed and with no total.
    pub budgets: Vec<BacktestEngineBudget>,
}
}
wire! {
/// The input a start request needs, as far as the service can serve it: the lanes and the per-engine token budgets
/// (see `BacktestStartInput`) of a run the project already had. Reads only, and nothing is defaulted: `run` is null
/// when the project never ran a backtest, so no lane, engine or budget is invented for a first run, and a budget that
/// was never set reads unset rather than as a number.
pub struct BacktestStartView {
    pub project: String,
    /// The project's most recent backtest, when there is one.
    pub run: Option<BacktestStartRun>,
}
}
