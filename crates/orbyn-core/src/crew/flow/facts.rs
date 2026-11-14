//! Everything the Flow screens know about one work task, read once. Reads only: the store's read connection, never a
//! write, a process or a file. The answer key is not read into it; only its text, to withhold it from what is shown.

use orbyn_contracts::{Brief, Evidence, Gate, RouteDecision};
use serde_json::{Value, json};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{CrewFix, CrewRun, CrewTask, MergeEntry, MergeRecord};

pub(super) struct Facts {
    pub task: CrewTask,
    pub run: Option<CrewRun>,
    pub brief: Option<Brief>,
    /// The build's routing: the latest decision of the task.
    pub route: Option<RouteDecision>,
    /// The fixes that answered findings, oldest first, each with its task's state.
    pub fixes: Vec<(CrewFix, String)>,
    /// Evidence of the build now under way: the latest fix when it has any, else the task's own.
    pub evidence: Vec<Evidence>,
    pub gates: Vec<Gate>,
    /// `Core::review_status`, or an empty review when no lane ran.
    pub review: Value,
    pub entry: Option<MergeEntry>,
    pub record: Option<MergeRecord>,
}

impl Core {
    pub(super) fn flow_facts(&self, task: CrewTask) -> Result<Facts> {
        let store = &self.store;
        let id = task.id.clone();
        let mut fixes = Vec::new();
        for fix in store.crew_fixes_of(&id)? {
            let state = store
                .crew_task(&fix.fix_task_id)?
                .map_or_else(String::new, |t| t.state);
            fixes.push((fix, state));
        }
        let mut evidence = store.crew_evidence(&id)?;
        if let Some((fix, _)) = fixes.last() {
            let latest = store.crew_evidence(&fix.fix_task_id)?;
            if !latest.is_empty() {
                evidence = latest;
            }
        }
        let review = if store.crew_reviews_of(&id)?.is_empty() {
            json!({ "lanes": [], "findings": [], "blocked": false })
        } else {
            self.review_status(&id)
                .map_err(|e| CoreError::Invalid(e.to_string()))?
        };
        Ok(Facts {
            run: store.crew_run(&id)?,
            brief: store.crew_brief(&id)?,
            route: store.crew_route_decisions(&id)?.pop(),
            fixes,
            evidence,
            gates: store.crew_gates(&id)?,
            review,
            entry: store.merge_entry(&id)?,
            record: store.crew_merge(&id)?,
            task,
        })
    }
}
