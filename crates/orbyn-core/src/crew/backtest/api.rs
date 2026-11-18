//! A person's way into the reviewer backtest (slice 4n PR 3): the estimate, the start, the stop and the report.
//!
//! - **Only a person starts a run.** The routes are local only and listed in `crew::changes::STATE_CHANGING`, so Autopilot
//!   refuses an agent's request to them; nothing here is called from a timer, a scheduler or a tool. `crew_backtest_start`
//!   is the one function that reaches `replay::begin` and `replay::finish` (the guard test in `replay_tests` enforces it).
//! - **One run at a time.** A second start while one is running is refused (`CrewTaskBusy`); the check and the insert are one
//!   store transaction. A stop only asks: the run stays `running` until its last replay is over, then ends as `stopped`.
//! - **No live engine.** Until a live `LaneRunner` exists nothing is connected, and a start is refused (`Unsupported`) before
//!   anything is written. Tests install a fake with `crew_backtest_use`.
//! - **Outcomes are Not measured.** The escape log (slice 4k) is not merged, so the source is `NoEscapeLog` and every lane
//!   shows finding counts only, never a zero.
//! - **Budgets are tokens per engine,** one entry per engine and never summed. No money appears in a request or a view.
//! - **History is the project's own merges** (`crew_merges`), numbered by the merge's record id: a pull request nobody
//!   merged through Crew is not replayed (open question 7 of the design note).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use orbyn_contracts as wire;

use super::history::{HISTORY_DAYS, OUTCOME_DAYS, PastPr, eligible_prs};
use super::outcomes::{EscapeSource, NoEscapeLog, Window};
use super::replay::{
    BacktestReport, BacktestRequest, LaneKind, LaneResult, LaneRunner, LaneSpec, STOPPED_BY_PERSON,
    begin, finish, report,
};
use super::score::LaneScore;
use crate::app::Core;
use crate::crew::valid_id;
use crate::error::{CoreError, Result};
use crate::store::{BacktestState, LaneState};
use crate::util::{new_id, unix_now};

const DAY_SECS: i64 = 86_400;

type SharedRunner = Arc<dyn LaneRunner + Send + Sync>;
type SharedEscapes = Arc<dyn EscapeSource + Send + Sync>;

/// What the core keeps for the backtest: the runner a start uses (none until a live one exists), where outcomes come
/// from, and the task of the run in progress, so a test can wait for it.
#[derive(Default)]
pub(crate) struct Slot {
    inner: Mutex<Parts>,
}

#[derive(Default)]
struct Parts {
    runner: Option<SharedRunner>,
    escapes: Option<SharedEscapes>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Slot {
    fn lock(&self) -> MutexGuard<'_, Parts> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn wire_kind(kind: wire::BacktestLaneKind) -> LaneKind {
    match kind {
        wire::BacktestLaneKind::Review => LaneKind::Review,
        wire::BacktestLaneKind::Check => LaneKind::Check,
    }
}

fn specs(lanes: &[wire::BacktestLaneInput]) -> Vec<LaneSpec> {
    lanes
        .iter()
        .map(|lane| LaneSpec {
            lane: lane.lane.clone(),
            engine: lane.engine.clone(),
            model: lane.model.clone(),
            kind: wire_kind(lane.kind),
        })
        .collect()
}

fn request_of(input: &wire::BacktestStartInput) -> Result<BacktestRequest> {
    let mut budgets = BTreeMap::new();
    for budget in &input.budgets {
        // Two entries for one engine would silently become one: refuse, so no budget is ever merged or summed.
        if budgets
            .insert(budget.engine.clone(), budget.tokens)
            .is_some()
        {
            return Err(CoreError::Invalid(
                "an engine has one token budget, given once".into(),
            ));
        }
    }
    Ok(BacktestRequest {
        project: input.project.clone(),
        lanes: specs(&input.lanes),
        budgets,
    })
}

fn state_view(state: BacktestState) -> wire::BacktestState {
    match state {
        BacktestState::Estimated => wire::BacktestState::Estimated,
        BacktestState::Running => wire::BacktestState::Running,
        BacktestState::Stopped => wire::BacktestState::Stopped,
        BacktestState::Finished => wire::BacktestState::Finished,
        BacktestState::Failed => wire::BacktestState::Failed,
    }
}

fn lane_state_view(state: LaneState) -> wire::BacktestLaneState {
    match state {
        LaneState::Pending => wire::BacktestLaneState::Pending,
        LaneState::Running => wire::BacktestLaneState::Running,
        LaneState::Stopped => wire::BacktestLaneState::Stopped,
        LaneState::Failed => wire::BacktestLaneState::Failed,
        LaneState::Done => wire::BacktestLaneState::Done,
    }
}

fn rate_view(rate: super::score::Rate) -> wire::BacktestRate {
    wire::BacktestRate {
        numerator: rate.numerator,
        denominator: rate.denominator,
        permille: rate.permille,
    }
}

fn score_view(score: &LaneScore) -> wire::LaneScore {
    wire::LaneScore {
        recall: score.recall.map(rate_view),
        precision: score.precision.map(rate_view),
        escapes_n: score.escapes_n,
        findings_n: score.findings_n,
        unmatched_n: score.unmatched_n,
    }
}

/// A run as a person reads it. Built only from the stored report.
fn run_view(report: BacktestReport) -> wire::BacktestRun {
    let run = report.run;
    wire::BacktestRun {
        id: run.id,
        project: run.project_id,
        state: state_view(run.state),
        budgets: run
            .budgets
            .into_iter()
            .map(|(engine, tokens)| wire::BacktestEngineBudget { engine, tokens })
            .collect(),
        stop_reason: run.stop_reason,
        created_unix: run.created_unix,
        finished_unix: run.finished_unix,
        lanes: report
            .lanes
            .into_iter()
            .map(|lane| {
                let (outcome, findings_n, score) = match &lane.result {
                    LaneResult::Scored(score) => (
                        wire::BacktestLaneOutcome::Scored,
                        score.findings_n,
                        Some(score_view(score)),
                    ),
                    LaneResult::NotMeasured { findings_n } => {
                        (wire::BacktestLaneOutcome::NotMeasured, *findings_n, None)
                    }
                    LaneResult::Unscored { findings_n } => {
                        (wire::BacktestLaneOutcome::Unscored, *findings_n, None)
                    }
                };
                wire::BacktestLaneRun {
                    lane: lane.lane,
                    engine: lane.engine,
                    model: lane.model,
                    state: lane_state_view(lane.state),
                    tokens_used: lane.tokens_used,
                    outcome,
                    findings_n,
                    score,
                }
            })
            .collect(),
    }
}

impl Core {
    fn backtest_escapes(&self) -> SharedEscapes {
        self.crew_backtest
            .lock()
            .escapes
            .clone()
            .unwrap_or_else(|| Arc::new(NoEscapeLog))
    }

    /// The project's merges as the history a backtest replays: one entry per merge Crew recorded for it.
    fn backtest_history(&self, project: &str) -> Result<Vec<PastPr>> {
        Ok(self
            .store
            .crew_merges_of_project(project)?
            .into_iter()
            .map(|merge| PastPr {
                number: u64::try_from(merge.id).unwrap_or_default(),
                merged_unix: merge.merged_unix,
            })
            .collect())
    }

    /// How big a backtest would be: the merges it may replay and each lane's replays. Reads only; nothing runs and
    /// nothing is stored. The counts are information: they never change which engine the router picks.
    pub fn crew_backtest_estimate(
        &self,
        project: &str,
        lanes: &[wire::BacktestLaneInput],
    ) -> Result<wire::BacktestEstimate> {
        self.crew_preview_only()?;
        self.store.project(project)?;
        if !lanes.is_empty() {
            // The names a start would refuse (a builder lane, money) are refused here too. The budget is not asked.
            let budgets = lanes.iter().map(|l| (l.engine.clone(), 1)).collect();
            BacktestRequest {
                project: project.to_owned(),
                lanes: specs(lanes),
                budgets,
            }
            .validate()?;
        }
        let now = unix_now();
        let history = self.backtest_history(project)?;
        let eligible = u32::try_from(eligible_prs(&history, now).len()).unwrap_or(u32::MAX);
        let window = Window {
            from_unix: now - HISTORY_DAYS * DAY_SECS,
            until_unix: now - OUTCOME_DAYS * DAY_SECS,
        };
        Ok(wire::BacktestEstimate {
            project: project.to_owned(),
            history_days: u32::try_from(HISTORY_DAYS).unwrap_or_default(),
            outcome_days: u32::try_from(OUTCOME_DAYS).unwrap_or_default(),
            eligible_prs: eligible,
            lanes: lanes
                .iter()
                .map(|lane| wire::BacktestEstimateLane {
                    lane: lane.lane.clone(),
                    engine: lane.engine.clone(),
                    model: lane.model.clone(),
                    replays: eligible,
                })
                .collect(),
            outcomes_measured: self.backtest_escapes().escapes(project, window).is_ok(),
            running: self.store.crew_backtest_running()?,
        })
    }

    /// A person starts a backtest. The request is validated and the run recorded (refused when another is running), then
    /// the replay goes on in the background; the answer is the run as it stands, `running`.
    pub async fn crew_backtest_start(
        self: &Arc<Self>,
        input: &wire::BacktestStartInput,
    ) -> Result<wire::BacktestRun> {
        self.crew_preview_only()?;
        let request = request_of(input)?;
        request.validate()?;
        let Some(runner) = self.crew_backtest.lock().runner.clone() else {
            return Err(CoreError::Unsupported(
                "A backtest cannot run yet: no replay engine is connected. The estimate and the report are available."
                    .into(),
            ));
        };
        self.store.project(&request.project)?;
        let history = self.backtest_history(&request.project)?;
        let escapes = self.backtest_escapes();
        let id = new_id();
        let now = unix_now();
        begin(&self.store, &id, &request, now, true)?;
        let store = self.store.clone();
        let run_id = id.clone();
        let task = tokio::task::spawn_blocking(move || {
            let asked = || store.crew_backtest_stop_asked(&run_id).unwrap_or(false);
            if let Err(error) = finish(
                &store, &run_id, &request, &history, now, &*runner, &*escapes, &asked,
            ) {
                tracing::warn!(%error, run = %run_id, "a backtest did not finish");
            }
        });
        self.crew_backtest.lock().task = Some(task);
        Ok(run_view(report(&self.store, &id)?))
    }

    /// A person asks a running backtest to stop. It stops before its next replay and ends as `stopped`, scored on what
    /// it replayed. A run that already ended is returned as it is. A stop that arrives after the last replay has begun
    /// is not seen by the replay: the run then ends `finished`, with no stop reason, having done all it was asked to.
    pub fn crew_backtest_stop(&self, id: &str) -> Result<wire::BacktestRun> {
        self.crew_preview_only()?;
        valid_id(id)?;
        self.store.crew_backtest_run(id)?;
        self.store.crew_backtest_ask_stop(id, STOPPED_BY_PERSON)?;
        Ok(run_view(report(&self.store, id)?))
    }

    /// One run as stored. Reads only.
    pub fn crew_backtest_report(&self, id: &str) -> Result<wire::BacktestRun> {
        self.crew_preview_only()?;
        valid_id(id)?;
        Ok(run_view(report(&self.store, id)?))
    }

    /// At startup: a run left `running` by a crash (a release build aborts on a panic, so the run's own process cannot
    /// close it) becomes failed with the reason "interrupted". Runs whatever the Preview flag says: a run can only exist
    /// from an earlier process. A failure is logged and never stops the core opening.
    pub(crate) fn crew_backtest_recover(&self) {
        match self.store.crew_backtest_recover_interrupted(unix_now()) {
            Ok(0) => {}
            Ok(n) => tracing::warn!(
                runs = n,
                "backtest runs from a previous process were interrupted"
            ),
            Err(error) => tracing::warn!(%error, "backtest runs could not be recovered"),
        }
    }

    /// Tests: the runner a start uses and, optionally, where outcomes come from. No live one exists.
    #[cfg(any(test, feature = "test-support"))]
    pub fn crew_backtest_use(
        &self,
        runner: Arc<dyn LaneRunner + Send + Sync>,
        escapes: Option<Arc<dyn EscapeSource + Send + Sync>>,
    ) {
        let mut parts = self.crew_backtest.lock();
        parts.runner = Some(runner);
        parts.escapes = escapes;
    }

    /// Tests: waits until the run in progress has ended.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn crew_backtest_join(&self) {
        let task = self.crew_backtest.lock().task.take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

/// A runner for tests that is safe to share with the replay's thread: what each (lane, engine) raises on each pull
/// request, what a replay costs, and, when `hold` is set, a replay that waits for a message before it answers, so a test
/// can keep a run open. Nothing runs and nothing is read from disk or the network.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct ScriptedRunner {
    pub findings: BTreeMap<(String, String, u64), Vec<super::replay::ReplayedFinding>>,
    pub cost: BTreeMap<String, u64>,
    pub calls: Mutex<Vec<(String, String, u64)>>,
    /// Every replay asks for one message here first.
    pub hold: Option<Mutex<std::sync::mpsc::Receiver<()>>>,
    /// Told the pull request number each time a replay begins, before it waits on `hold`.
    pub began: Option<Mutex<std::sync::mpsc::Sender<u64>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl LaneRunner for ScriptedRunner {
    fn replay(
        &self,
        spec: &LaneSpec,
        pr: &PastPr,
    ) -> std::result::Result<super::replay::LaneReplay, super::replay::ReplayFailed> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push((
            spec.lane.clone(),
            spec.engine.clone(),
            pr.number,
        ));
        if let Some(began) = &self.began {
            let _ = began
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .send(pr.number);
        }
        if let Some(hold) = &self.hold {
            let _ = hold.lock().unwrap_or_else(|e| e.into_inner()).recv();
        }
        let key = (spec.lane.clone(), spec.engine.clone(), pr.number);
        Ok(super::replay::LaneReplay {
            findings: self.findings.get(&key).cloned().unwrap_or_default(),
            tokens_used: self.cost.get(&spec.engine).copied().unwrap_or(0),
        })
    }
}
