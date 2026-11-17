//! The reviewer backtest at rest (slice 4n): a run and its per-engine token budgets, each lane's progress and each
//! finding's outcome. Ids, counts and fingerprints only: no prompt, no diff text, no secret and no money.
//! Nothing here starts a backtest; only a person's choice does (see `crew::backtest::replay`).

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, Transaction, params};

use super::Store;
use crate::error::{CoreError, Result};

/// Where a run stands. `Estimated` is a run the person has seen the size of and not yet started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BacktestState {
    Estimated,
    Running,
    Stopped,
    Finished,
    Failed,
}

impl BacktestState {
    fn name(self) -> &'static str {
        match self {
            Self::Estimated => "estimated",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Finished => "finished",
            Self::Failed => "failed",
        }
    }

    fn of(name: &str) -> Result<Self> {
        match name {
            "estimated" => Ok(Self::Estimated),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "finished" => Ok(Self::Finished),
            "failed" => Ok(Self::Failed),
            other => Err(CoreError::Internal(format!(
                "a stored backtest state is unknown: {other}"
            ))),
        }
    }
}

/// Where one lane stands. `Stopped` means its engine's token budget was spent; `Failed` means the replay broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneState {
    Pending,
    Running,
    Stopped,
    Failed,
    Done,
}

impl LaneState {
    fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Done => "done",
        }
    }

    fn of(name: &str) -> Result<Self> {
        match name {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "done" => Ok(Self::Done),
            other => Err(CoreError::Internal(format!(
                "a stored backtest lane state is unknown: {other}"
            ))),
        }
    }
}

/// How one replayed finding fared. `NoEscape` is a finding on a pull request the escape log says had none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultOutcome {
    Caught,
    Unmatched,
    NoEscape,
}

impl ResultOutcome {
    fn name(self) -> &'static str {
        match self {
            Self::Caught => "caught",
            Self::Unmatched => "unmatched",
            Self::NoEscape => "no_escape",
        }
    }

    fn of(name: &str) -> Result<Self> {
        match name {
            "caught" => Ok(Self::Caught),
            "unmatched" => Ok(Self::Unmatched),
            "no_escape" => Ok(Self::NoEscape),
            other => Err(CoreError::Internal(format!(
                "a stored backtest outcome is unknown: {other}"
            ))),
        }
    }
}

/// A run as stored. `budgets` is tokens per engine; the values are never added together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestRunRow {
    pub id: String,
    pub project_id: String,
    pub state: BacktestState,
    pub budgets: BTreeMap<String, u64>,
    pub stop_reason: Option<String>,
    pub created_unix: i64,
    pub finished_unix: Option<i64>,
}

/// A lane of a run. `escapes_n` and `caught_n` are set once the lane is scored against a measured escape log; both
/// stay `None` when the log could not say (Not measured) or the lane failed, and `None` is never read as zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestLaneRow {
    pub lane: String,
    pub engine: String,
    pub model: String,
    pub tokens_used: u64,
    pub state: LaneState,
    pub escapes_n: Option<u32>,
    pub caught_n: Option<u32>,
}

/// One finding fingerprint on one past pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestResultRow {
    pub lane: String,
    pub engine: String,
    pub pr: u64,
    pub finding_fp: String,
    pub outcome: ResultOutcome,
}

fn db_int(value: u64, what: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| CoreError::Invalid(format!("{what} is too large")))
}

fn count_of(value: Option<i64>) -> Result<Option<u32>> {
    value
        .map(|n| {
            u32::try_from(n)
                .map_err(|_| CoreError::Internal("a stored backtest count is invalid".into()))
        })
        .transpose()
}

fn run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(BacktestRunRow, String, String)> {
    let state: String = row.get(2)?;
    let budgets: String = row.get(3)?;
    Ok((
        BacktestRunRow {
            id: row.get(0)?,
            project_id: row.get(1)?,
            state: BacktestState::Estimated,
            budgets: BTreeMap::new(),
            stop_reason: row.get(4)?,
            created_unix: row.get(5)?,
            finished_unix: row.get(6)?,
        },
        state,
        budgets,
    ))
}

fn finish_run_row(parts: (BacktestRunRow, String, String)) -> Result<BacktestRunRow> {
    let (mut run, state, budgets) = parts;
    run.state = BacktestState::of(&state)?;
    run.budgets = serde_json::from_str(&budgets)
        .map_err(|_| CoreError::Internal("stored backtest budgets are invalid".into()))?;
    Ok(run)
}

const RUN_COLUMNS: &str =
    "id, project_id, state, budgets, stop_reason, created_unix, finished_unix";

fn update_lane(tx: &Transaction<'_>, run_id: &str, lane: &BacktestLaneRow) -> Result<()> {
    let changed = tx.execute(
        "UPDATE crew_backtest_lanes
            SET model = ?4, tokens_used = ?5, state = ?6, escapes_n = ?7, caught_n = ?8
          WHERE run_id = ?1 AND lane = ?2 AND engine = ?3",
        params![
            run_id,
            lane.lane,
            lane.engine,
            lane.model,
            db_int(lane.tokens_used, "the tokens used")?,
            lane.state.name(),
            lane.escapes_n,
            lane.caught_n,
        ],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!(
            "backtest lane {}/{} of run {run_id}",
            lane.lane, lane.engine
        )));
    }
    Ok(())
}

impl Store {
    /// Records a new run of `project_id` with its lanes (as `(lane, engine, model)`), each pending with nothing
    /// used. The budgets are tokens per engine; an empty set, or a zero budget, is refused. An id already used, or a
    /// project that does not exist, is refused.
    pub fn crew_backtest_create(
        &self,
        id: &str,
        project_id: &str,
        state: BacktestState,
        budgets: &BTreeMap<String, u64>,
        lanes: &[(String, String, String)],
        created_unix: i64,
    ) -> Result<()> {
        if budgets.is_empty() || budgets.values().any(|tokens| *tokens == 0) {
            return Err(CoreError::Invalid(
                "every engine needs a token budget of at least 1".into(),
            ));
        }
        for tokens in budgets.values() {
            db_int(*tokens, "a token budget")?;
        }
        let budgets = serde_json::to_string(budgets)
            .map_err(|_| CoreError::Internal("the budgets cannot be written".into()))?;
        self.commit(|tx, _| {
            let known: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?1)",
                [project_id],
                |row| row.get(0),
            )?;
            if !known {
                return Err(CoreError::NotFound(format!("project {project_id}")));
            }
            tx.execute(
                "INSERT INTO crew_backtest_runs (id, project_id, state, budgets, created_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, project_id, state.name(), budgets, created_unix],
            )?;
            for (lane, engine, model) in lanes {
                tx.execute(
                    "INSERT INTO crew_backtest_lanes (run_id, lane, engine, model, state)
                     VALUES (?1, ?2, ?3, ?4, 'pending')",
                    params![id, lane, engine, model],
                )?;
            }
            Ok(())
        })
    }

    /// Moves a run to `state`, with the reason it stopped (engine ids only) and when it finished.
    pub fn crew_backtest_set_state(
        &self,
        id: &str,
        state: BacktestState,
        stop_reason: Option<&str>,
        finished_unix: Option<i64>,
    ) -> Result<()> {
        let changed = self.writer().execute(
            "UPDATE crew_backtest_runs SET state = ?2, stop_reason = ?3, finished_unix = ?4 WHERE id = ?1",
            params![id, state.name(), stop_reason, finished_unix],
        )?;
        if changed == 0 {
            return Err(CoreError::NotFound(format!("backtest run {id}")));
        }
        Ok(())
    }

    /// Saves one lane's progress and the findings it has so far, in one transaction. Results replace earlier ones
    /// with the same pull request and fingerprint, so saving the same lane twice leaves one row each.
    pub fn crew_backtest_save_lane(
        &self,
        run_id: &str,
        lane: &BacktestLaneRow,
        results: &[BacktestResultRow],
    ) -> Result<()> {
        self.commit(|tx, _| {
            update_lane(tx, run_id, lane)?;
            for result in results {
                if result.lane != lane.lane || result.engine != lane.engine {
                    return Err(CoreError::Invalid(
                        "a result belongs to another lane than the one saved".into(),
                    ));
                }
                tx.execute(
                    "INSERT OR REPLACE INTO crew_backtest_results (run_id, lane, engine, pr, finding_fp, outcome)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        run_id,
                        result.lane,
                        result.engine,
                        db_int(result.pr, "a pull request number")?,
                        result.finding_fp,
                        result.outcome.name(),
                    ],
                )?;
            }
            Ok(())
        })
    }

    /// One run.
    pub fn crew_backtest_run(&self, id: &str) -> Result<BacktestRunRow> {
        let parts = self
            .reader()
            .query_row(
                &format!("SELECT {RUN_COLUMNS} FROM crew_backtest_runs WHERE id = ?1"),
                [id],
                run_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("backtest run {id}")))?;
        finish_run_row(parts)
    }

    /// A project's runs, newest first.
    pub fn crew_backtest_runs_of(&self, project_id: &str) -> Result<Vec<BacktestRunRow>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM crew_backtest_runs WHERE project_id = ?1
             ORDER BY created_unix DESC, id DESC"
        ))?;
        let rows = statement
            .query_map([project_id], run_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(finish_run_row).collect()
    }

    /// A run's lanes, in lane then engine order.
    pub fn crew_backtest_lanes(&self, run_id: &str) -> Result<Vec<BacktestLaneRow>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT lane, engine, model, tokens_used, state, escapes_n, caught_n
               FROM crew_backtest_lanes WHERE run_id = ?1 ORDER BY lane, engine",
        )?;
        let rows = statement
            .query_map([run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(lane, engine, model, tokens, state, escapes, caught)| {
                Ok(BacktestLaneRow {
                    lane,
                    engine,
                    model,
                    tokens_used: u64::try_from(tokens).map_err(|_| {
                        CoreError::Internal("stored backtest tokens are invalid".into())
                    })?,
                    state: LaneState::of(&state)?,
                    escapes_n: count_of(escapes)?,
                    caught_n: count_of(caught)?,
                })
            })
            .collect()
    }

    /// A run's results, by lane, engine, pull request and fingerprint.
    pub fn crew_backtest_results(&self, run_id: &str) -> Result<Vec<BacktestResultRow>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT lane, engine, pr, finding_fp, outcome FROM crew_backtest_results
              WHERE run_id = ?1 ORDER BY lane, engine, pr, finding_fp",
        )?;
        let rows = statement
            .query_map([run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(lane, engine, pr, finding_fp, outcome)| {
                Ok(BacktestResultRow {
                    lane,
                    engine,
                    pr: u64::try_from(pr).map_err(|_| {
                        CoreError::Internal("a stored pull request number is invalid".into())
                    })?,
                    finding_fp,
                    outcome: ResultOutcome::of(&outcome)?,
                })
            })
            .collect()
    }
}
