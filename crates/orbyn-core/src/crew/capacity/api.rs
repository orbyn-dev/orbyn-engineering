//! Review capacity as the routes read and write it: the state a person sees, the coverage map and the person's own
//! settings. Preview only: with the flag off every call answers "not found", like the routes.

use std::collections::BTreeMap;

use orbyn_contracts as wire;

use super::{
    CapacitySettings, MergeProvenance, Priority, Sampling, StartDecision, coverage, plan_size,
    start_allowed, waiting_count,
};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::util::unix_now;

/// Rows of the sampling plan shown to a person: batches of 1 to 4, 5 to 8, 9 to 12 and 13 to 16.
const PLAN_ROWS: u32 = 4;

fn sampling_wire(sampling: Sampling) -> wire::CapacitySampling {
    match sampling {
        Sampling::Off => wire::CapacitySampling::Off,
        Sampling::OneInFour => wire::CapacitySampling::OneInFour,
    }
}

fn sampling_of(sampling: wire::CapacitySampling) -> Sampling {
    match sampling {
        wire::CapacitySampling::Off => Sampling::Off,
        wire::CapacitySampling::OneInFour => Sampling::OneInFour,
    }
}

fn priority_wire(priority: Priority) -> wire::CapacityPriority {
    match priority {
        Priority::High => wire::CapacityPriority::High,
        Priority::Medium => wire::CapacityPriority::Medium,
        Priority::Low => wire::CapacityPriority::Low,
    }
}

fn priority_of(priority: wire::CapacityPriority) -> Priority {
    match priority {
        wire::CapacityPriority::High => Priority::High,
        wire::CapacityPriority::Medium => Priority::Medium,
        wire::CapacityPriority::Low => Priority::Low,
    }
}

impl Core {
    /// A project's capacity settings and what the pause rule reads now. A new Low, Crew-started start is `paused`
    /// only when pausing is on and more merges wait than the limit.
    pub fn crew_capacity_state(&self, project_id: &str) -> Result<wire::CapacityState> {
        self.crew_preview_only()?;
        self.store.project(project_id)?;
        let settings = self.store.crew_capacity_settings(project_id)?;
        let waiting = waiting_count(&self.store.crew_waiting_merges_of_project(project_id)?);
        let paused = match start_allowed(settings, waiting, Priority::Low, true) {
            StartDecision::Paused { waiting, limit } if settings.pause_enabled => {
                Some(wire::PausedStart { waiting, limit })
            }
            _ => None,
        };
        let plan = (1..=PLAN_ROWS)
            .map(|row| {
                let to = row * 4;
                wire::SamplePlan {
                    from: to - 3,
                    to,
                    sampled: plan_size(Sampling::OneInFour, to),
                }
            })
            .collect();
        Ok(wire::CapacityState {
            project: project_id.to_owned(),
            waiting,
            limit: settings.limit,
            sampling: sampling_wire(settings.sampling),
            pause_enabled: settings.pause_enabled,
            settings_saved: self.store.crew_capacity_settings_saved(project_id)?,
            paused,
            plan,
        })
    }

    /// The project's review coverage since `since` (unix seconds, 0 for all). Merge-level review only.
    pub fn crew_review_coverage(
        &self,
        project_id: &str,
        since: i64,
    ) -> Result<wire::ReviewCoverage> {
        self.crew_preview_only()?;
        self.store.project(project_id)?;
        let merges = self.store.crew_merges_of_project(project_id)?;
        let mut hunks = Vec::with_capacity(merges.len());
        let mut began = BTreeMap::new();
        for merge in &merges {
            hunks.push(self.store.crew_hunks_of_merge(merge.id)?);
            if !began.contains_key(&merge.repo)
                && let Some(at) = self.store.crew_provenance_began(&merge.repo)?
            {
                began.insert(merge.repo.clone(), at);
            }
        }
        let items: Vec<_> = merges
            .iter()
            .zip(&hunks)
            .map(|(merge, hunks)| MergeProvenance { merge, hunks })
            .collect();
        let map = coverage(&items, &began, since);
        Ok(wire::ReviewCoverage {
            project: project_id.to_owned(),
            began_unix: map.began_unix,
            as_of_unix: unix_now(),
            merges: map.merges,
            not_measured_merges: map.not_measured_merges,
            agent_lines: map.agent_lines,
            reviewed_lines: map.reviewed_lines,
            in_queue_lines: map.in_queue_lines,
            areas: map
                .areas
                .into_iter()
                .map(|a| wire::AreaCoverage {
                    area: a.area,
                    merges: a.merges,
                    agent_lines: a.agent_lines,
                    reviewed_lines: a.reviewed_lines,
                    in_queue_lines: a.in_queue_lines,
                    not_measured_merges: a.not_measured_merges,
                })
                .collect(),
            engines: map
                .engines
                .into_iter()
                .map(|e| wire::EngineCoverage {
                    engine: e.engine,
                    model: e.model,
                    agent_lines: e.agent_lines,
                    reviewed_lines: e.reviewed_lines,
                })
                .collect(),
        })
    }

    /// A person's settings for one project. The route is person-only; nothing an agent can call reaches this.
    pub fn crew_set_capacity_settings(
        &self,
        input: &wire::CapacitySettingsInput,
    ) -> Result<wire::CapacityState> {
        self.crew_preview_only()?;
        self.store.set_crew_capacity_settings(
            &input.project,
            CapacitySettings {
                limit: input.limit,
                sampling: sampling_of(input.sampling),
                pause_enabled: input.pause_enabled,
            },
        )?;
        self.crew_capacity_state(&input.project)
    }

    /// A person sets a task's priority and whether Crew began it. A field left out stays as it is.
    pub fn crew_set_task_capacity(
        &self,
        task_id: &str,
        input: &wire::CapacityTaskInput,
    ) -> Result<wire::CapacityTaskAnswer> {
        self.crew_preview_only()?;
        if input.priority.is_none() && input.crew_started.is_none() {
            return Err(CoreError::Invalid(
                "send a priority, crew_started or both".into(),
            ));
        }
        // Reads first, so an unknown task changes nothing.
        self.store.crew_task_capacity(task_id)?;
        if let Some(priority) = input.priority {
            self.store
                .set_crew_task_priority(task_id, priority_of(priority))?;
        }
        if let Some(started) = input.crew_started {
            self.store.set_crew_task_crew_started(task_id, started)?;
        }
        let (priority, crew_started) = self.store.crew_task_capacity(task_id)?;
        Ok(wire::CapacityTaskAnswer {
            task: task_id.to_owned(),
            priority: priority_wire(priority),
            crew_started,
        })
    }
}
