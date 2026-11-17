//! The start hook of review capacity: before a new worker is granted, a Low-priority start that Crew began itself
//! pauses while too many merges wait for a person. The logic lives here so `crew_start` carries one line. With the
//! Crew Preview flag off it does nothing.

use orbyn_contracts::{CrewError, ErrorCode};

use super::{Priority, StartDecision, start_allowed, waiting_count};
use crate::app::Core;

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
}
