//! The two places the Crew paths call into review capacity. Start: before a new worker is granted, a Low-priority start
//! that Crew began itself pauses while too many merges wait for a person. Merge: after a merge row is stored, the merged
//! range's provenance is recorded. The logic lives here so `crew_start` and the merge queue carry one line each. With
//! the Crew Preview flag off neither does anything.

use orbyn_contracts::{CrewError, ErrorCode};
use std::time::Duration;

use serde_json::json;

use super::{Author, Priority, StartDecision, record_at_merge, start_allowed, waiting_count};
use crate::app::Core;
use crate::error::CoreError;
use crate::store::{MergeRecord, Store};

/// What a person reads on a `provenance_failed` row. Code-owned and fixed per stage: never git's output, a path or an
/// error's text, which can hold an absolute path of this machine.
fn failure_reason(stage: &str) -> &'static str {
    match stage {
        "hunks" => {
            "The merged changes could not be read or stored, so their authorship was not recorded."
        }
        "note" => {
            "The local provenance note could not be written, so the merge's lines are not counted."
        }
        _ => "Provenance could not be recorded for this merge.",
    }
}

/// The ledger write is retried a few times: a failure row that is lost is the one silent outcome this hook must not have.
const LEDGER_TRIES: u32 = 3;

/// Writes one `provenance_failed` row on the merge's task: the stage, a fixed reason, that the merge reads as Not
/// measured, and `resolved: false` until a retry succeeds. A store failure is retried, then logged.
fn write_failure(store: &Store, merge: &MergeRecord, stage: &str) {
    let body = json!({
        "stage": stage,
        "why": failure_reason(stage),
        "coverage": "Not measured",
        "retry": "at the next merge",
        "resolved": false,
    });
    for attempt in 1..=LEDGER_TRIES {
        match store.add_crew_autopilot_event(
            &merge.task_id,
            &merge.crew_id,
            "provenance_failed",
            &body,
        ) {
            Ok(_) => return,
            Err(write) if attempt == LEDGER_TRIES => {
                tracing::warn!(error = %write, task = %merge.task_id, "could not write a provenance_failed event");
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20 * u64::from(attempt))),
        }
    }
}

/// Records the provenance of one stored merge. A failure is written to the approval ledger on the merge's task as
/// `provenance_failed` (the same place `area_breaker_failed` goes) with a fixed reason, logged with its detail, and
/// returned so a test can see the failure path was taken. The merge then reads as Not measured in coverage: a failed
/// record leaves no note, and a merge without one is never counted. The caller never lets it fail the merge: the merge
/// has already happened.
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
    write_failure(store, merge, stage);
    Err(error)
}

/// The most unresolved failures one retry pass looks at.
const RETRY_READ: u32 = 10;

/// Tries again every merge whose provenance failed and has not since been recorded, oldest first, at most
/// `RETRY_READ` per pass. A try that works writes a `resolved: true` row on the same task and the merge then counts in
/// coverage; one that fails again changes nothing (the first row already says so) and is tried at the next pass. The
/// range is recorded as of its own merge time, so a repository whose first merge failed begins measuring at it.
pub(crate) async fn retry_provenance(store: &Store) {
    let Ok(failures) = store.crew_unresolved_provenance_failures(RETRY_READ) else {
        return;
    };
    let mut seen = std::collections::BTreeSet::new();
    for failure in failures {
        if !seen.insert(failure.task_id.clone()) {
            continue;
        }
        let Ok(Some(merge)) = store.crew_merge(&failure.task_id) else {
            continue;
        };
        let agent = agent_of(store, &merge);
        let retried = match record_at_merge(store, &merge, &agent, &[], merge.merged_unix).await {
            Ok(recorded) => recorded.note,
            Err(error) => Err(error),
        };
        match retried {
            Ok(()) => {
                let body = json!({ "stage": failure.body["stage"], "resolved": true });
                if let Err(error) = store.add_crew_autopilot_event(
                    &merge.task_id,
                    &merge.crew_id,
                    "provenance_failed",
                    &body,
                ) {
                    tracing::warn!(%error, task = %merge.task_id, "could not mark a provenance failure resolved");
                }
            }
            Err(error) => {
                tracing::warn!(%error, task = %merge.task_id, "the merge's provenance still fails");
            }
        }
    }
}

/// The task's engine, and its routed model when that decision was for this engine.
fn agent_of(store: &Store, merge: &MergeRecord) -> Author {
    let model = store
        .crew_route_decisions(&merge.task_id)
        .ok()
        .and_then(|all| all.into_iter().next_back())
        .filter(|d| d.engine.as_str() == merge.engine)
        .map(|d| d.model.as_str().to_owned());
    Author::Agent {
        engine: merge.engine.clone(),
        model,
    }
}

/// What `capacity_start_check` says first, so a caller can tell this refusal from any other `not_authorized`.
const PAUSED: &str = "review capacity:";

/// What a person sees on a merge whose Comment fix is held back by full review capacity. Code-owned text.
pub(crate) const FIX_WAITING: &str = "The comment fix is waiting because review capacity is full. Answer the comment again after some merges are reviewed.";

/// Whether `error` is the refusal of a Crew-started start while review capacity is full. Such a start is not an error to
/// retry: nothing was stored, and the caller says it is waiting and leaves the next try to a later trigger.
pub(crate) fn paused_by_capacity(error: &CrewError) -> bool {
    error.code == ErrorCode::NotAuthorized && error.message.starts_with(PAUSED)
}

impl Core {
    /// Before a worker is granted: refuses a NEW start that Crew began itself at Low priority while more merges wait
    /// for a person than the project's limit, and only when a person turned `pause_enabled` on (off by default).
    /// Nothing is stored when it refuses.
    ///
    /// `stored` is the task's `(priority, crew_started)`. A task being created has no row yet, so the start path
    /// passes `Some((Low, true))` when its launch carries `Links::crew_started` (review fixes, merge-comment fixes)
    /// and `None` otherwise, which reads as Medium and person-started and never pauses. A store failure reading the
    /// settings or the queue lets the start go, since this only ever holds back optional work.
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
        // Off until a person turns it on: the default never holds a start back.
        if !settings.pause_enabled {
            return Ok(());
        }
        match start_allowed(settings, waiting, priority, crew_started) {
            StartDecision::Go => Ok(()),
            StartDecision::Paused { waiting, limit } => Err(CrewError::new(
                ErrorCode::NotAuthorized,
                format!(
                    "{PAUSED} {waiting} merges wait for a person and the limit is {limit}; \
                     a new low-priority start from Crew is paused until some are reviewed"
                ),
            )),
        }
    }

    /// After a Comment fix started: removes the waiting note above, so a fix that is now running is not shown as waiting.
    /// Any other note on the merge is left alone. A store failure is logged.
    pub(crate) fn capacity_fix_started(&self, task_id: &str) {
        let cleared = self.store.crew_merge(task_id).and_then(|merge| {
            match merge.and_then(|m| m.needs_person).as_deref() {
                Some(FIX_WAITING) => self.store.set_merge_needs_person(task_id, None),
                _ => Ok(()),
            }
        });
        if let Err(error) = cleared {
            tracing::warn!(%error, task = task_id, "the waiting note was not cleared");
        }
    }

    /// After the task is stored: when Crew began it itself, records it as Low priority and Crew-started so a later
    /// start can be paused against it. A person's start stores nothing (the defaults are Medium and person-started).
    /// A store failure is logged and never fails the start.
    pub(crate) fn capacity_mark_started(&self, task_id: &str, crew_started: bool) {
        if !self.crew_enabled() || !crew_started {
            return;
        }
        let marked = self
            .store
            .set_crew_task_priority(task_id, Priority::Low)
            .and_then(|()| self.store.set_crew_task_crew_started(task_id, true));
        if let Err(error) = marked {
            tracing::warn!(%error, task = task_id, "the task's capacity mark was not stored");
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
        let agent = agent_of(&self.store, merge);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(task = %merge.task_id, "no runtime to record the merge's provenance on");
            // Not silent: the ledger says it, and a later pass records it.
            write_failure(&self.store, merge, "start");
            return;
        };
        let (store, merge) = (self.store.clone(), merge.clone());
        runtime.spawn(async move {
            // Earlier failures first, so a transient one heals at the next merge. Already logged and written to the
            // ledger inside.
            retry_provenance(&store).await;
            let _ = record_provenance(&store, &merge, agent, now).await;
        });
    }
}
