//! The daily per-engine budget of a background crew (docs/crew/background-engine-start.md, slice 4h, PR 2): what each engine may
//! spend on one crew in a day, runs and turns, set at creation like the run budget and reset at the person's midnight (decision 1:
//! their `utc_offset_minutes`, the same clock the scheduled tasks take). Turns are counted from engine events —
//! `crew_worker_run_ended` fires once per turn — and minutes are wall time, so an engine's count is only its own. **No engine takes
//! a hard limit:** a turn in flight is never killed for the day's sake, a run already going is cancelled at the cap, and what the
//! engine spent is shown, never clamped. A new event whose engine has spent the day's budget is refused outright with an audit row
//! reading "daily budget spent: <engine>" and the time the day turns: never held, never queued, unlike a review-capacity pause. A
//! crew made before this slice has no daily budget (0 and 0): nothing is counted for it and nothing is refused.

use orbyn_contracts::{CrewError, ErrorCode};

use super::router::{RouteInput, route};
use super::score::EvalHistory;
use crate::app::Core;
use crate::store::CrewTask;

/// What the cancel at the cap says: the engine, its turns against the cap, and when the day turns.
pub(super) struct DailyCap {
    pub(super) engine: String,
    pub(super) turns: u32,
    pub(super) cap: u32,
    pub(super) reset: String,
}

/// The person's today, and when it turns: `(day, reset)` from their UTC offset. The day is the local calendar date
/// `YYYY-MM-DD`; the reset is the next local midnight as RFC 3339, so the client can show the person their own clock.
pub(super) fn local_day_and_reset(offset_minutes: i32, now: i64) -> Option<(String, String)> {
    let offset = time::UtcOffset::from_whole_seconds(offset_minutes * 60).ok()?;
    let at = time::OffsetDateTime::from_unix_timestamp(now)
        .ok()?
        .to_offset(offset);
    let day = at.date().to_string();
    let reset = at
        .replace_time(time::Time::MIDNIGHT)
        .checked_add(time::Duration::days(1))?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()?;
    Some((day, reset))
}

impl Core {
    /// One engine turn of a background run ended, whatever the turn did: it counts live on the run (one more turn, the wall
    /// minutes so far) and, when the crew has a daily budget, on the engine's day. A failure here never changes how the run
    /// ended. Called once per turn from `crew_worker_run_ended`.
    pub(super) fn crew_background_turn_ended(&self, task: &CrewTask, worker: &str) {
        let Ok(Some(run)) = self.store.background_run(&task.crew_id) else {
            return;
        };
        if run.state != "running" {
            return;
        }
        let minutes = crate::util::parse_time(&run.started_at).map_or(0, |at| {
            u32::try_from((crate::util::unix_now() - at).max(0) / 60).unwrap_or(u32::MAX)
        });
        if let Err(error) = self.store.count_background_run_turn(&run.id, minutes) {
            tracing::warn!(%error, run = %run.id, "could not count a background run's turn");
        }
        let Ok(Some(crew)) = self.store.background_crew(&run.background_id) else {
            return;
        };
        // A crew from before the daily budget has none: nothing is counted for it.
        if crew.daily_runs == 0 && crew.daily_turns == 0 {
            return;
        }
        let Ok(engine) = self.store.thread(worker).map(|thread| thread.engine) else {
            return;
        };
        let Some((day, _)) = local_day_and_reset(crew.utc_offset_minutes, crate::util::unix_now())
        else {
            return;
        };
        if let Err(error) = self
            .store
            .add_background_daily_turn(&crew.id, &run.id, &day, &engine)
        {
            tracing::warn!(%error, run = %run.id, "could not count a background run's daily turn");
        }
    }

    /// Whether the worker's engine has spent this crew's daily turns: `Some` carries what the cancel says. The turn was just
    /// counted; a run at the cap is cancelled by the caller, not retried. A crew from before the daily budget never matches.
    pub(super) fn crew_background_daily_at_cap(&self, task: &CrewTask) -> Option<DailyCap> {
        let run = self.store.background_run(&task.crew_id).ok().flatten()?;
        if run.state != "running" {
            return None;
        }
        let crew = self
            .store
            .background_crew(&run.background_id)
            .ok()
            .flatten()?;
        if crew.daily_turns == 0 {
            return None;
        }
        let worker = self
            .store
            .crew_run(&task.id)
            .ok()
            .flatten()?
            .worker_thread_id?;
        let engine = self.store.thread(&worker).ok()?.engine;
        let (day, reset) = local_day_and_reset(crew.utc_offset_minutes, crate::util::unix_now())?;
        let (_, turns) = self
            .store
            .background_daily_spent(&crew.id, &day, &engine)
            .ok()?;
        (turns >= crew.daily_turns).then_some(DailyCap {
            engine,
            turns,
            cap: crew.daily_turns,
            reset,
        })
    }

    /// The refusal before a background start stores anything: the engine the router would give the run has spent the crew's
    /// daily budget (runs or turns), so the event is refused — never held, never queued — with an audit row naming the engine
    /// and a sentence that tells the person when their day turns. `lead_thread` is the run's own id (a run's id is its lead
    /// thread's). Any crew or routing problem other than the cap itself is left to the paths that own it.
    pub(super) fn refuse_background_daily(
        &self,
        input: &RouteInput<'_>,
        views: &[super::view::EngineView],
        project_id: &str,
        lead_thread: &str,
    ) -> Result<(), CrewError> {
        let Ok(Some(run)) = self.store.background_run(lead_thread) else {
            return Ok(());
        };
        if run.state != "running" {
            return Ok(());
        }
        let Ok(Some(crew)) = self.store.background_crew(&run.background_id) else {
            return Ok(());
        };
        if crew.daily_runs == 0 && crew.daily_turns == 0 {
            return Ok(());
        }
        let Ok(config) = self.crew_router_config_with_mappings(project_id) else {
            return Ok(());
        };
        let Ok(decision) = route(input, views, &config, &EvalHistory::default()) else {
            return Ok(());
        };
        let engine = decision.engine.as_str().to_owned();
        let Some((day, reset)) =
            local_day_and_reset(crew.utc_offset_minutes, crate::util::unix_now())
        else {
            return Ok(());
        };
        let Ok((runs, turns)) = self.store.background_daily_spent(&crew.id, &day, &engine) else {
            return Ok(());
        };
        if runs < crew.daily_runs && turns < crew.daily_turns {
            return Ok(());
        }
        if let Err(error) = self
            .store
            .audit_background_daily_refusal(&crew.id, &engine, &day, &reset)
        {
            tracing::warn!(%error, "could not audit a background crew's daily refusal");
        }
        Err(CrewError::new(
            ErrorCode::NotAuthorized,
            format!("daily budget spent: {engine}. It resets at {reset}."),
        ))
    }
}
