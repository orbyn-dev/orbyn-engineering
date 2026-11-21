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

/// What the daily check of a run in flight found.
pub(super) enum DailyCheck {
    Clear,
    Cap(DailyCap),
    Unreadable,
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
    /// minutes so far) and, when the crew has a daily budget, on the engine's day. While the run is still running, a turn that
    /// cannot be counted on a crew with a daily budget (or whose crew cannot be read) is an `Err` the caller answers by stopping
    /// the run `budget`: the cap check that follows would otherwise read a ledger without the turn. A late turn (the run has
    /// ended) is counted best effort: a failure is logged and never changes the run or its task. A read failure of the run
    /// itself is logged and answered `Ok`: it cannot be told from a task that belongs to no background run, and the cap check
    /// reads the run again and fails closed. Called once per turn from `crew_worker_run_ended`.
    pub(super) fn crew_background_turn_ended(
        &self,
        task: &CrewTask,
        worker: &str,
    ) -> Result<(), &'static str> {
        const UNCOUNTED: &str =
            "The crew's daily budget could not be updated, so the run stopped instead of going on.";
        let run = match self.store.background_run(&task.crew_id) {
            Ok(Some(run)) => run,
            Ok(None) => return Ok(()),
            Err(error) => {
                tracing::warn!(%error, task = %task.id, "could not read a background run to count its turn");
                return Ok(());
            }
        };
        // Only a run still running is stopped for an uncounted turn; for an ended run the day count is best effort.
        let live = run.state == "running";
        let uncounted = || if live { Err(UNCOUNTED) } else { Ok(()) };
        // A turn that ends after its run did (a run cancelled at the cap while the engine was mid-turn) still happened: it counts
        // for the engine's day, though no longer for the run.
        if live {
            let minutes = crate::util::parse_time(&run.started_at).map_or(0, |at| {
                u32::try_from((crate::util::unix_now() - at).max(0) / 60).unwrap_or(u32::MAX)
            });
            if let Err(error) = self.store.count_background_run_turn(&run.id, minutes) {
                tracing::warn!(%error, run = %run.id, "could not count a background run's turn");
            }
        }
        let crew = match self.store.background_crew(&run.background_id) {
            Ok(Some(crew)) => crew,
            Ok(None) => return Ok(()),
            Err(error) => {
                tracing::warn!(%error, run = %run.id, "could not read a background crew to count its daily turn");
                return uncounted();
            }
        };
        // A crew from before the daily budget has none: nothing is counted for it.
        if crew.daily_runs == 0 && crew.daily_turns == 0 {
            return Ok(());
        }
        // From here the crew has a daily budget: a turn that cannot be counted stops a running run, because the check that
        // follows would read a ledger without it and could let the run go on over the cap.
        let engine = match self.store.thread(worker) {
            Ok(thread) => thread.engine,
            Err(error) => {
                tracing::warn!(%error, run = %run.id, "could not read a background worker's engine to count its daily turn");
                return uncounted();
            }
        };
        let Some((day, _)) = local_day_and_reset(crew.utc_offset_minutes, crate::util::unix_now())
        else {
            tracing::warn!(run = %run.id, "a background crew's clock is out of range, so its daily turn was not counted");
            return uncounted();
        };
        if let Err(error) = self
            .store
            .add_background_daily_turn(&crew.id, &run.id, &day, &engine)
        {
            tracing::warn!(%error, run = %run.id, "could not count a background run's daily turn");
            return uncounted();
        }
        Ok(())
    }

    /// Whether the worker's engine has spent this crew's daily turns. The turn was just counted; a run at the cap is cancelled by
    /// the caller, not retried. A crew from before the daily budget is `Clear`. A budget that cannot be read is `Unreadable`, never
    /// `Clear`: a run that cannot be shown to be inside its budget does not go on.
    pub(super) fn crew_background_daily_check(&self, task: &CrewTask) -> DailyCheck {
        let run = match self.store.background_run(&task.crew_id) {
            Ok(Some(run)) => run,
            // A task of no background run has no daily budget.
            Ok(None) => return DailyCheck::Clear,
            Err(_) => return DailyCheck::Unreadable,
        };
        if run.state != "running" {
            return DailyCheck::Clear;
        }
        let crew = match self.store.background_crew(&run.background_id) {
            Ok(Some(crew)) => crew,
            _ => return DailyCheck::Unreadable,
        };
        if crew.daily_turns == 0 {
            return DailyCheck::Clear;
        }
        let engine = self
            .store
            .crew_run(&task.id)
            .ok()
            .flatten()
            .and_then(|run| run.worker_thread_id)
            .and_then(|worker| self.store.thread(&worker).ok())
            .map(|thread| thread.engine);
        let (Some(engine), Some((day, reset))) = (
            engine,
            local_day_and_reset(crew.utc_offset_minutes, crate::util::unix_now()),
        ) else {
            return DailyCheck::Unreadable;
        };
        match self.store.background_daily_spent(&crew.id, &day, &engine) {
            Ok((_, turns)) if turns >= crew.daily_turns => DailyCheck::Cap(DailyCap {
                engine,
                turns,
                cap: crew.daily_turns,
                reset,
            }),
            Ok(_) => DailyCheck::Clear,
            Err(_) => DailyCheck::Unreadable,
        }
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
        let unreadable = |error: &dyn std::fmt::Display| {
            tracing::warn!(%error, "could not read a background crew's daily budget");
            CrewError::new(
                ErrorCode::NotAuthorized,
                "The crew's daily budget could not be read, so no run was started. Try again.",
            )
        };
        let run = match self.store.background_run(lead_thread) {
            Ok(Some(run)) if run.state == "running" => run,
            Ok(_) => return Ok(()),
            Err(error) => return Err(unreadable(&error)),
        };
        let crew = match self.store.background_crew(&run.background_id) {
            Ok(Some(crew)) => crew,
            Ok(None) => return Ok(()),
            Err(error) => return Err(unreadable(&error)),
        };
        if crew.daily_runs == 0 && crew.daily_turns == 0 {
            return Ok(());
        }
        // A routing problem is left to the path that owns it: it refuses the same start itself.
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
            return Err(unreadable(&"the person's clock is out of range"));
        };
        let (runs, turns) = match self.store.background_daily_spent(&crew.id, &day, &engine) {
            Ok(spent) => spent,
            Err(error) => return Err(unreadable(&error)),
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
