//! Replaying review lanes and checks on past pull requests (slice 4n PR 2), and scoring them against the escape log.
//!
//! Rules this module keeps:
//! - A backtest starts only when a person chooses it. `begin` and `finish` (`run` is the two together) are the only
//!   way in and nothing calls them on a timer, at startup or from any scheduler: only `api::crew_backtest_start`,
//!   which the person's route reaches (the guard test in `replay_tests` fails if any other non-test code calls them).
//! - The budget is tokens per engine. Two engines' budgets are never added, compared or pooled, and no money appears
//!   anywhere: a request that names a money field or a price-like engine is refused.
//! - Only review lanes and checks replay. A builder lane cannot be written as a `LaneKind`, and a lane named like
//!   one is refused.
//! - Nothing here runs a model. A `LaneRunner` does the replay; no live one exists yet, and tests use `FakeLaneRunner`.
//! - A lane whose engine has spent its budget stops and says so; a lane whose replay breaks fails and the others go
//!   on. The lanes of one engine share that engine's budget.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::history::{HISTORY_DAYS, OUTCOME_DAYS, PastPr, eligible_prs};
use super::outcomes::{Escape, EscapeRead, EscapeSource, Window};
use super::score::{LaneScore, Outcome, PastFinding, count, from_counts, match_escape};
use crate::error::{CoreError, Result};
use crate::store::{
    BacktestLaneRow, BacktestResultRow, BacktestRunRow, BacktestState, LaneState,
    MOST_BUDGET_TOKENS, ResultOutcome, Store,
};

const DAY_SECS: i64 = 86_400;
const MOST_NAME_LEN: usize = 128;
/// Words that make a lane, engine or budget key look like a builder or like money.
const BUILDER_WORDS: [&str; 4] = ["builder", "implementer", "coder", "author"];
const MONEY_WORDS: [&str; 12] = [
    "usd", "eur", "gbp", "dollar", "dollars", "cent", "cents", "cost", "price", "money", "spend",
    "currency",
];

/// What a replayed lane is. There is no builder kind: a backtest replays reviews and checks only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneKind {
    Review,
    Check,
}

/// One lane to replay, on one engine and model.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaneSpec {
    pub lane: String,
    pub engine: String,
    pub model: String,
    pub kind: LaneKind,
}

/// A person's request for a backtest. Budgets are tokens per engine and are never summed; there is no money field,
/// and any other field is refused when it is read from text.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BacktestRequest {
    pub project: String,
    pub lanes: Vec<LaneSpec>,
    pub budgets: BTreeMap<String, u64>,
}

fn words(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
}

fn check_name(what: &str, name: &str) -> Result<()> {
    if name.trim().is_empty()
        || name.len() > MOST_NAME_LEN
        || name.chars().any(|c| c.is_control() || c == '$')
    {
        return Err(CoreError::Invalid(format!("{what} is not a usable name")));
    }
    if words(name).any(|word| MONEY_WORDS.contains(&word.as_str())) {
        return Err(CoreError::Invalid(format!(
            "{what} looks like money: a backtest budget is tokens per engine"
        )));
    }
    Ok(())
}

impl BacktestRequest {
    /// Reads a request from JSON and validates it. An unknown field (a price, a cost cap, a builder lane) is refused.
    pub fn from_json(text: &str) -> Result<Self> {
        let request: Self = serde_json::from_str(text)
            .map_err(|_| CoreError::Invalid("the backtest request is not valid".into()))?;
        request.validate()?;
        Ok(request)
    }

    /// Whether the request can run: lanes named, every lane's engine has a token budget of at least 1, no lane is
    /// a builder, nothing looks like money and no lane is listed twice on one engine.
    pub fn validate(&self) -> Result<()> {
        check_name("the project", &self.project)?;
        if self.lanes.is_empty() {
            return Err(CoreError::Invalid(
                "a backtest needs at least one lane".into(),
            ));
        }
        let mut seen = BTreeSet::new();
        for spec in &self.lanes {
            check_name("a lane", &spec.lane)?;
            check_name("an engine", &spec.engine)?;
            check_name("a model", &spec.model)?;
            if words(&spec.lane).any(|word| BUILDER_WORDS.contains(&word.as_str())) {
                return Err(CoreError::Invalid(
                    "a backtest replays reviews and checks, never a builder".into(),
                ));
            }
            if !seen.insert((spec.lane.as_str(), spec.engine.as_str())) {
                return Err(CoreError::Invalid(
                    "a lane is listed twice on one engine".into(),
                ));
            }
            match self.budgets.get(&spec.engine) {
                Some(tokens) if *tokens >= 1 => {}
                _ => {
                    return Err(CoreError::Invalid(
                        "every engine of a lane needs a token budget of at least 1".into(),
                    ));
                }
            }
        }
        for (engine, tokens) in &self.budgets {
            check_name("a budget", engine)?;
            if !(1..=MOST_BUDGET_TOKENS).contains(tokens) {
                return Err(CoreError::Invalid(format!(
                    "a token budget must be from 1 to {MOST_BUDGET_TOKENS}"
                )));
            }
        }
        Ok(())
    }
}

/// A finding a replay raised: its fingerprint and the files it is about. The files score it and are not stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayedFinding {
    pub fingerprint: String,
    pub files: Vec<String>,
}

/// What one lane raised on one past pull request, and the tokens that cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneReplay {
    pub findings: Vec<ReplayedFinding>,
    pub tokens_used: u64,
}

/// A replay that broke. It carries no text: the run records only that the lane failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayFailed;

/// Replays one review lane or check on one past pull request at the head it was reviewed at.
pub trait LaneRunner {
    fn replay(&self, spec: &LaneSpec, pr: &PastPr)
    -> std::result::Result<LaneReplay, ReplayFailed>;
}

/// A lane's score, or why it has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaneResult {
    /// Scored against a measured escape log (a rate still shows only from its minimum, see `LaneScore`).
    Scored(LaneScore),
    /// The escape log could not say: only the finding count shows, and no escape count of zero is implied.
    NotMeasured { findings_n: u32 },
    /// The lane failed or has not finished: its findings so far are counted and nothing is scored.
    Unscored { findings_n: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneReport {
    pub lane: String,
    pub engine: String,
    pub model: String,
    pub state: LaneState,
    pub tokens_used: u64,
    pub result: LaneResult,
    /// Confirmed escapes in the window that could not be matched because their fix or revert files could not be read.
    /// Reported beside the score and never counted in it; 0 when the lane was not scored.
    pub unresolved_n: u32,
}

/// A run as a person reads it: the run and each lane. Built only from what the store holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestReport {
    pub run: BacktestRunRow,
    pub lanes: Vec<LaneReport>,
}

/// Reads a run's report from the store. It is a pure function of the stored rows, so reading twice gives the same.
pub fn report(store: &Store, run_id: &str) -> Result<BacktestReport> {
    let run = store.crew_backtest_run(run_id)?;
    let results = store.crew_backtest_results(run_id)?;
    let lanes = store
        .crew_backtest_lanes(run_id)?
        .into_iter()
        .map(|lane| {
            let own = results
                .iter()
                .filter(|r| r.lane == lane.lane && r.engine == lane.engine);
            let findings_n = count(own.clone().count());
            let matched = count(own.filter(|r| r.outcome == ResultOutcome::Caught).count());
            let result = match (lane.state, lane.escapes_n, lane.caught_n) {
                (LaneState::Stopped | LaneState::Done, Some(escapes), Some(caught)) => {
                    LaneResult::Scored(from_counts(escapes, caught, findings_n, matched))
                }
                (LaneState::Stopped | LaneState::Done, _, _) => {
                    LaneResult::NotMeasured { findings_n }
                }
                _ => LaneResult::Unscored { findings_n },
            };
            LaneReport {
                lane: lane.lane,
                engine: lane.engine,
                model: lane.model,
                state: lane.state,
                tokens_used: lane.tokens_used,
                result,
                unresolved_n: lane.unresolved_n.unwrap_or(0),
            }
        })
        .collect();
    Ok(BacktestReport { run, lanes })
}

/// How many of `escapes` the findings caught.
fn rates_caught(findings: &[PastFinding], escapes: &[Escape]) -> u32 {
    count(
        escapes
            .iter()
            .filter(|e| match_escape(findings, e) == Outcome::Caught)
            .count(),
    )
}

fn outcome_of(finding: &PastFinding, escapes: &[Escape]) -> ResultOutcome {
    let on_pr: Vec<&Escape> = escapes.iter().filter(|e| e.pr == finding.pr).collect();
    if on_pr.is_empty() {
        return ResultOutcome::NoEscape;
    }
    let one = std::slice::from_ref(finding);
    if on_pr
        .iter()
        .any(|e| match_escape(one, e) == Outcome::Caught)
    {
        ResultOutcome::Caught
    } else {
        ResultOutcome::Unmatched
    }
}

/// Why a run a person stopped says it stopped.
pub const STOPPED_BY_PERSON: &str = "stopped by the person";

/// Records a run a person chose to start, with each lane pending. With `alone` set, a run already in progress refuses
/// it (`CrewTaskBusy`) and nothing is written: one backtest at a time. Nothing is replayed here.
pub(crate) fn begin(
    store: &Store,
    run_id: &str,
    request: &BacktestRequest,
    now_unix: i64,
    alone: bool,
) -> Result<()> {
    request.validate()?;
    let lane_keys: Vec<(String, String, String)> = request
        .lanes
        .iter()
        .map(|s| (s.lane.clone(), s.engine.clone(), s.model.clone()))
        .collect();
    if alone {
        store.crew_backtest_create_alone(
            run_id,
            &request.project,
            &request.budgets,
            &lane_keys,
            now_unix,
        )
    } else {
        store.crew_backtest_create(
            run_id,
            &request.project,
            BacktestState::Running,
            &request.budgets,
            &lane_keys,
            now_unix,
        )
    }
}

/// Runs a backtest a person chose to start. `begin` and `finish` together. For the tests: the person's start (`api`)
/// calls the two itself, because it records the run before it goes on in the background.
#[cfg(test)]
pub(crate) fn run(
    store: &Store,
    run_id: &str,
    request: &BacktestRequest,
    prs: &[PastPr],
    now_unix: i64,
    runner: &dyn LaneRunner,
    escapes: &dyn EscapeSource,
) -> Result<BacktestReport> {
    begin(store, run_id, request, now_unix, false)?;
    finish(
        store,
        run_id,
        request,
        prs,
        now_unix,
        runner,
        escapes,
        &|| false,
    )
}

/// Replays each lane on each eligible pull request of `prs` (merged 30 to 180 days before `now_unix`) for a run
/// `begin` recorded, stops a lane once its engine's token budget is spent, records a lane whose replay broke as failed
/// without stopping the others, stores every finding and returns the report read back from the store.
/// `stop` is asked before every replay: once it says yes the run ends as stopped, scoring what was replayed. A stop that
/// arrives after the last replay has begun (or when no pull request is left to replay) is never seen: the run ends
/// `finished` with no stop reason, because it did everything it was asked to.
///
/// When `escapes` says Not measured the run still finishes: findings are counted and nothing is scored, because
/// no recall or precision can be read from an escape log that cannot say.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish(
    store: &Store,
    run_id: &str,
    request: &BacktestRequest,
    prs: &[PastPr],
    now_unix: i64,
    runner: &dyn LaneRunner,
    escapes: &dyn EscapeSource,
    stop: &dyn Fn() -> bool,
) -> Result<BacktestReport> {
    let eligible = eligible_prs(prs, now_unix);
    // From here a run exists. Whatever goes wrong (an error, or in a test or dev build a panic in the runner) ends it
    // as failed, so it does not stay `running`; the final write is best effort. A release build aborts on a panic
    // (`panic = "abort"`), which no code can catch: the startup recovery marks such a run failed as "interrupted".
    let ended = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(
            store, run_id, request, &eligible, now_unix, runner, escapes, stop,
        )
    }));
    let error = match ended {
        Ok(Ok(())) => return report(store, run_id),
        Ok(Err(error)) => error,
        Err(_) => CoreError::Internal("a backtest lane runner panicked".into()),
    };
    let _ = store.crew_backtest_fail(run_id, "failed: the backtest could not finish", now_unix);
    Err(error)
}

#[allow(clippy::too_many_arguments)]
fn execute(
    store: &Store,
    run_id: &str,
    request: &BacktestRequest,
    eligible: &[&PastPr],
    now_unix: i64,
    runner: &dyn LaneRunner,
    escapes: &dyn EscapeSource,
    stop: &dyn Fn() -> bool,
) -> Result<()> {
    let window = Window {
        from_unix: now_unix - HISTORY_DAYS * DAY_SECS,
        until_unix: now_unix - OUTCOME_DAYS * DAY_SECS,
    };
    let measured: Option<EscapeRead> = escapes.read(&request.project, window).ok();
    let mut spent: BTreeMap<&str, u64> = BTreeMap::new();
    let mut budget_spent_engines = BTreeSet::new();
    let mut failed = 0usize;
    let mut stopped_by_person = false;

    for spec in &request.lanes {
        let budget = request.budgets[&spec.engine];
        let mut row = BacktestLaneRow {
            lane: spec.lane.clone(),
            engine: spec.engine.clone(),
            model: spec.model.clone(),
            tokens_used: 0,
            state: LaneState::Running,
            escapes_n: None,
            caught_n: None,
            unresolved_n: None,
        };
        store.crew_backtest_save_lane(run_id, &row, &[])?;
        // Each finding with its fingerprint; a fingerprint seen twice on one pull request is one finding.
        let mut found: Vec<(PastFinding, String)> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut replayed = BTreeSet::new();
        let mut state = LaneState::Done;
        for pr in eligible {
            // A person's stop ends the run: this lane and every later one stop before their next replay.
            if stop() {
                state = LaneState::Stopped;
                stopped_by_person = true;
                break;
            }
            // Only this engine's own spending is compared with this engine's own budget.
            if spent.get(spec.engine.as_str()).copied().unwrap_or(0) >= budget {
                state = LaneState::Stopped;
                budget_spent_engines.insert(spec.engine.clone());
                break;
            }
            let Ok(replay) = runner.replay(spec, pr) else {
                state = LaneState::Failed;
                break;
            };
            row.tokens_used = row.tokens_used.saturating_add(replay.tokens_used);
            let engine_total = spent.entry(spec.engine.as_str()).or_insert(0);
            *engine_total = engine_total.saturating_add(replay.tokens_used);
            replayed.insert(pr.number);
            for finding in replay.findings {
                if seen.insert((pr.number, finding.fingerprint.clone())) {
                    let past = PastFinding {
                        pr: pr.number,
                        lane: spec.lane.clone(),
                        engine: spec.engine.clone(),
                        model: spec.model.clone(),
                        fingerprint: finding.fingerprint.clone(),
                        files: finding.files,
                    };
                    found.push((past, finding.fingerprint));
                }
            }
        }
        row.state = state;
        let past: Vec<PastFinding> = found.iter().map(|(f, _)| f.clone()).collect();
        // A stopped lane is scored on the pull requests it did replay; a failed lane is not scored.
        // A lane stopped before its first replay saw no pull request: it is not scored, which is not zero escapes.
        let scored = measured.is_some()
            && (state == LaneState::Done || (state == LaneState::Stopped && !replayed.is_empty()));
        let scoped: Vec<Escape> = match (&measured, scored) {
            (Some(all), true) => all
                .escapes
                .iter()
                .filter(|e| replayed.contains(&e.pr))
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        if scored {
            row.escapes_n = Some(count(scoped.len()));
            if let Some(all) = &measured {
                // An unresolved escape on a pull request this lane did not replay is not this lane's to report.
                row.unresolved_n = Some(count(
                    all.unresolved
                        .iter()
                        .filter(|u| u.pr.is_none_or(|pr| replayed.contains(&pr)))
                        .count(),
                ));
            }
            row.caught_n = Some(rates_caught(&past, &scoped));
        }
        if state == LaneState::Failed {
            failed += 1;
        }
        let stored: Vec<BacktestResultRow> = found
            .iter()
            .map(|(f, fp)| BacktestResultRow {
                lane: f.lane.clone(),
                engine: f.engine.clone(),
                pr: f.pr,
                finding_fp: fp.clone(),
                // Without a measured log nothing is known about escapes: not a miss, so never `unmatched`.
                outcome: if scored {
                    outcome_of(f, &scoped)
                } else {
                    ResultOutcome::NotMeasured
                },
            })
            .collect();
        store.crew_backtest_save_lane(run_id, &row, &stored)?;
    }

    let (state, reason) = if failed == request.lanes.len() {
        (BacktestState::Failed, None)
    } else if stopped_by_person {
        (BacktestState::Stopped, Some(STOPPED_BY_PERSON.to_owned()))
    } else if !budget_spent_engines.is_empty() {
        let engines: Vec<&str> = budget_spent_engines.iter().map(String::as_str).collect();
        (
            BacktestState::Stopped,
            Some(format!("token budget spent: {}", engines.join(", "))),
        )
    } else {
        (BacktestState::Finished, None)
    };
    store.crew_backtest_set_state(run_id, state, reason.as_deref(), Some(now_unix))
}

/// A fake runner for tests only: what each (lane, engine) raises on each pull request, what each replay costs, and
/// which lanes break. Nothing runs and nothing is read from disk or the network.
#[cfg(test)]
#[derive(Default)]
pub struct FakeLaneRunner {
    /// Findings per (lane, engine, pull request).
    pub findings: BTreeMap<(String, String, u64), Vec<ReplayedFinding>>,
    /// Tokens one replay costs, per engine. An engine not listed costs 0.
    pub cost: BTreeMap<String, u64>,
    /// (lane, engine) pairs whose replay breaks.
    pub broken: BTreeSet<(String, String)>,
    /// Every replay asked for, in order: (lane, engine, pull request).
    pub calls: std::cell::RefCell<Vec<(String, String, u64)>>,
}

#[cfg(test)]
impl LaneRunner for FakeLaneRunner {
    fn replay(
        &self,
        spec: &LaneSpec,
        pr: &PastPr,
    ) -> std::result::Result<LaneReplay, ReplayFailed> {
        self.calls
            .borrow_mut()
            .push((spec.lane.clone(), spec.engine.clone(), pr.number));
        if self
            .broken
            .contains(&(spec.lane.clone(), spec.engine.clone()))
        {
            return Err(ReplayFailed);
        }
        let key = (spec.lane.clone(), spec.engine.clone(), pr.number);
        Ok(LaneReplay {
            findings: self.findings.get(&key).cloned().unwrap_or_default(),
            tokens_used: self.cost.get(&spec.engine).copied().unwrap_or(0),
        })
    }
}
