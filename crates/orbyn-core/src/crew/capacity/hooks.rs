//! The two places the Crew paths call into review capacity. Start: before a new worker is granted, a Low-priority start
//! that Crew began itself pauses while too many merges wait for a person. Merge: after a merge row is stored, the merged
//! range's provenance is recorded. The logic lives here so `crew_start` and the merge queue carry one line each. With
//! the Crew Preview flag off neither does anything.

use orbyn_contracts::{CrewError, ErrorCode};
use serde_json::json;

use super::{Author, Priority, StartDecision, record_at_merge, start_allowed, waiting_count};
use crate::app::Core;
use crate::error::CoreError;
use crate::store::{MergeRecord, Store};

/// Records the provenance of one stored merge. A failure is written to the approval ledger on the merge's task as
/// `provenance_failed` (the same place `area_breaker_failed` goes), logged, and returned so a test can see the failure
/// path was taken. The caller never lets it fail the merge: the merge has already happened.
pub(crate) async fn record_provenance(
    store: &Store,
    merge: &MergeRecord,
    agent: Author,
    now: i64,
) -> Result<(), CoreError> {
    // Person commits: EMPTY for now, so every line reads as the agent's. A later change passes only commits it
    // positively identified as a person's (for example commits made through the app), never a guess.
    let (stage, error) = match record_at_merge(store, merge, &agent, &[], now).await {
        Ok(recorded) => match recorded.note {
            Ok(()) => return Ok(()),
            Err(error) => ("note", error),
        },
        Err(error) => ("hunks", error),
    };
    tracing::warn!(%error, stage, task = %merge.task_id, "the merge's provenance was not recorded");
    let body = json!({ "stage": stage, "why": error.public_message(), "resolved": false });
    if let Err(write) =
        store.add_crew_autopilot_event(&merge.task_id, &merge.crew_id, "provenance_failed", &body)
    {
        tracing::warn!(error = %write, task = %merge.task_id, "could not write a provenance_failed event");
    }
    Err(error)
}

impl Core {
    /// Before a worker is granted: refuses a NEW start that Crew began itself at Low priority while more merges wait
    /// for a person than the project's limit. Nothing is stored when it refuses.
    ///
    /// `stored` is the task's `(priority, crew_started)` from the store (migration 47). A task that is being created
    /// has none yet, so the start path passes `None` and this reads it as Medium and person-started: nothing pauses
    /// until a later change sets them. A store failure reading the settings or the queue lets the start go, since
    /// this only ever holds back optional work.
    pub(crate) fn capacity_start_check(
        &self,
        project_id: &str,
        stored: Option<(Priority, bool)>,
    ) -> Result<(), CrewError> {
        if !self.crew_enabled() {
            return Ok(());
        }
        let (priority, crew_started) = stored.unwrap_or((Priority::Medium, false));
        if !(crew_started && priority == Priority::Low) {
            return Ok(());
        }
        let read = || -> crate::error::Result<_> {
            Ok((
                self.store.crew_capacity_settings(project_id)?,
                waiting_count(&self.store.crew_waiting_merges_of_project(project_id)?),
            ))
        };
        let (settings, waiting) = match read() {
            Ok(facts) => facts,
            Err(error) => {
                tracing::warn!(%error, "review capacity could not be read, so the start goes ahead");
                return Ok(());
            }
        };
        match start_allowed(settings, waiting, priority, crew_started) {
            StartDecision::Go => Ok(()),
            StartDecision::Paused { waiting, limit } => Err(CrewError::new(
                ErrorCode::NotAuthorized,
                format!(
                    "review capacity: {waiting} merges wait for a person and the limit is {limit}; \
                     a new low-priority start from Crew is paused until some are reviewed"
                ),
            )),
        }
    }

    /// After a merge row is stored: records per-hunk provenance for `before..merge` in the background. A failure is
    /// recorded as above and never fails or delays the merge. The queue's merge path is synchronous and git is async,
    /// so the write runs as a detached task; its result is visible in the store once it finishes.
    ///
    /// Sampling is not run here. The boundary for it is one `crew_process_merges` pass (its `MergeOutcome::Merged`
    /// list), and that function is not part of this hook.
    pub(crate) fn capacity_after_merge(&self, merge: &MergeRecord, now: i64) {
        if !self.crew_enabled() {
            return;
        }
        let model = self
            .store
            .crew_route_decisions(&merge.task_id)
            .ok()
            .and_then(|all| all.into_iter().next_back())
            .filter(|d| d.engine.as_str() == merge.engine)
            .map(|d| d.model.as_str().to_owned());
        let agent = Author::Agent {
            engine: merge.engine.clone(),
            model,
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(task = %merge.task_id, "no runtime to record the merge's provenance on");
            return;
        };
        let (store, merge) = (self.store.clone(), merge.clone());
        runtime.spawn(async move {
            // Already logged and written to the ledger inside.
            let _ = record_provenance(&store, &merge, agent, now).await;
        });
    }
}
