//! Everything the Flow screens know about one work task, read once. Reads only: the store's read connection, never a
//! write, a process or a file. The answer key is not read into it; only its text, to withhold it from what is shown.

use orbyn_contracts::{Brief, Evidence, Gate, RouteDecision};
use serde_json::Value;

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{CrewFix, CrewRun, CrewTask, MergeEntry, MergeRecord};

pub(in crate::crew) struct Facts {
    pub task: CrewTask,
    pub run: Option<CrewRun>,
    pub brief: Option<Brief>,
    /// The build's routing: the latest decision of the task.
    pub route: Option<RouteDecision>,
    /// The fixes that answered findings, oldest first, each with its task's state.
    pub fixes: Vec<(CrewFix, String)>,
    /// Evidence of the build now under way: the latest fix when it has any, else the task's own.
    pub evidence: Vec<Evidence>,
    /// The evidence of the task that is building now (the latest fix, else the task itself), and no other.
    pub build_evidence: Vec<Evidence>,
    /// That task's checks gates about its own brief, oldest first. A lead's repro gate on a finding is not one.
    pub build_gates: Vec<Gate>,
    pub gates: Vec<Gate>,
    /// `Core::review_status`, or an empty review when no lane ran.
    pub review: Value,
    pub entry: Option<MergeEntry>,
    pub record: Option<MergeRecord>,
}

impl Facts {
    /// The worker never ran: the task waits for a person (`needs_person`), no report of its worker was ever taken and nothing
    /// was checked. Only a start can end that way: the router refused, a worker that cannot be restricted was refused, or a
    /// setting blocked the launch. A task the checks could not decide has a report, so it is never this. It reads stored data
    /// only (`crew_runs.last_report_run`, which `begin_crew_verification` sets on the first report or missed report, the
    /// evidence and the fixes), so a task stored before this was written derives the same way, with no migration.
    pub(crate) fn could_not_start(&self) -> bool {
        self.task.state == "needs_person"
            && self
                .run
                .as_ref()
                .is_some_and(|run| run.last_report_run.is_none() && run.attempts == 0)
            && self.evidence.is_empty()
            && self.gates.is_empty()
            && self.fixes.is_empty()
    }
}

impl Core {
    pub(in crate::crew) fn flow_facts(&self, task: CrewTask) -> Result<Facts> {
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
        let building = fixes
            .last()
            .map_or_else(|| id.clone(), |(fix, _)| fix.fix_task_id.clone());
        let build_evidence = store.crew_evidence(&building)?;
        let build_brief = store.crew_brief(&building)?.map(|b| b.id);
        let build_gates = store
            .crew_gates(&building)?
            .into_iter()
            .filter(|g| {
                g.stage == orbyn_contracts::GateStage::Checks
                    && build_brief
                        .as_ref()
                        .is_some_and(|brief| *brief == g.subject)
            })
            .collect();
        let review = self
            .review_status(&id)
            .map_err(|_| CoreError::Internal("the review could not be read".into()))?;
        Ok(Facts {
            run: store.crew_run(&id)?,
            brief: store.crew_brief(&id)?,
            route: store.crew_route_decisions(&id)?.pop(),
            fixes,
            evidence,
            build_evidence,
            build_gates,
            gates: store.crew_gates(&id)?,
            review,
            entry: store.merge_entry(&id)?,
            record: store.crew_merge(&id)?,
            task,
        })
    }
}
