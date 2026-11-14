//! Crew Flow (Preview): the read side of the Flow screens. It lists a crew's work tasks with the ten cells of each
//! task's line, and zooms into one task's lanes, findings, gates and rounds. Nothing here writes: every function reads
//! through the store's read connection, starts no process and takes no lock beyond it. It approves, merges and reverts
//! nothing; the person's answers are `autopilot`'s.
//!
//! What it never returns: the answer key or any part of a check (command, output, what it proves), a repro, a thread
//! id, a path on this machine, a bus message. A text is redacted, the key's text withheld and the length cut
//! ([`scrub`]). A gate's reason is read only once the task is final, so a running task shows no check text at all (H1).

mod cells;
mod facts;
mod scrub;

use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::Value;

use self::cells::{cells, in_filter, is_security, latest_rounds, round};
use self::facts::Facts;
use self::scrub::{Scrub, TEXT_MAX};
use super::lifecycle::is_final;
use super::{Page, valid_id, wire_name};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::redaction::Redactor;

/// The most work tasks one list call reads; the newest are kept.
pub const SCAN_MAX: u32 = 500;
/// The most lanes, findings and gates one task shows.
pub const LANES_MAX: usize = 64;
pub const FINDINGS_MAX: usize = 200;
pub const GATES_MAX: usize = 200;
const FILES_MAX: usize = 50;
const QUERY_MAX: usize = 80;

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

fn short(id: &str) -> String {
    id.chars().take(12).collect()
}

impl Core {
    /// Crews that have work tasks, the most recently moved first.
    pub fn crew_flow_crews(&self, page: Page) -> Result<wire::FlowCrewList> {
        self.crew_preview_only()?;
        let (rows, total) = self.store.crew_totals_page(page.limit, page.offset)?;
        let seen = u64::from(page.offset) + rows.len() as u64;
        Ok(wire::FlowCrewList {
            crews: rows
                .into_iter()
                .map(|r| wire::FlowCrewRow {
                    label: short(&r.crew_id),
                    id: r.crew_id,
                    tasks: r.tasks,
                    finished: r.finished,
                    updated_at: r.updated_at,
                })
                .collect(),
            total,
            next_offset: (seen < u64::from(total)).then(|| u32::try_from(seen).unwrap_or(u32::MAX)),
        })
    }

    /// One crew's work tasks with their cells, needs-you first, then newest first. `filter` is `all`, `needs_you`,
    /// `reviewing`, `failed` or `passed`; `engine` and `q` narrow further.
    pub fn crew_flow_tasks(
        &self,
        crew: &str,
        filter: Option<&str>,
        engine: Option<&str>,
        q: Option<&str>,
        page: Page,
    ) -> Result<wire::FlowTaskList> {
        self.crew_preview_only()?;
        valid_id(crew)?;
        let filter = match filter {
            None => wire::FlowFilter::All,
            Some(text) => serde_json::from_value(Value::String(text.to_owned()))
                .map_err(|_| CoreError::Invalid("that filter is not one of the list".into()))?,
        };
        if let Some(engine) = engine {
            valid_id(engine)?;
        }
        let needle = q.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
        if needle
            .as_ref()
            .is_some_and(|q| q.chars().count() > QUERY_MAX)
        {
            return Err(CoreError::Invalid(format!(
                "the search is at most {QUERY_MAX} characters"
            )));
        }
        let person = self.display_redactor()?;
        let mut tasks = self.store.crew_work_tasks(crew, SCAN_MAX + 1)?;
        let truncated = tasks.len() > SCAN_MAX as usize;
        tasks.truncate(SCAN_MAX as usize);

        let mut counts = wire::FlowCounts {
            all: 0,
            needs_you: 0,
            reviewing: 0,
            failed: 0,
            passed: 0,
        };
        let mut matched: Vec<(bool, wire::FlowTaskRow)> = Vec::new();
        for task in tasks {
            let state = task.state.clone();
            let facts = self.flow_facts(task)?;
            let line = cells(&facts);
            counts.all += 1;
            let needs = in_filter(&line, &state, wire::FlowFilter::NeedsYou);
            counts.needs_you += u32::from(needs);
            counts.reviewing += u32::from(in_filter(&line, &state, wire::FlowFilter::Reviewing));
            counts.failed += u32::from(in_filter(&line, &state, wire::FlowFilter::Failed));
            counts.passed += u32::from(in_filter(&line, &state, wire::FlowFilter::Passed));
            if !in_filter(&line, &state, filter) {
                continue;
            }
            let row = self.flow_row(&facts, line, &person)?;
            if engine.is_some_and(|e| row.engine.as_deref() != Some(e)) {
                continue;
            }
            if let Some(needle) = &needle {
                let hay = format!(
                    "{} {} {}",
                    row.name,
                    row.kind,
                    row.engine.as_deref().unwrap_or("")
                )
                .to_lowercase();
                if !hay.contains(needle.as_str()) {
                    continue;
                }
            }
            matched.push((needs, row));
        }
        // A row that needs the person rises to the top; the order inside each group stays newest first.
        matched.sort_by_key(|(needs, _)| !*needs);
        let total = u32::try_from(matched.len()).unwrap_or(u32::MAX);
        let start = (page.offset as usize).min(matched.len());
        let end = (start + page.limit as usize).min(matched.len());
        Ok(wire::FlowTaskList {
            crew: crew.to_owned(),
            counts,
            tasks: matched.drain(start..end).map(|(_, row)| row).collect(),
            total,
            next_offset: (end < total as usize).then(|| u32::try_from(end).unwrap_or(u32::MAX)),
            truncated,
        })
    }

    fn flow_row(
        &self,
        facts: &Facts,
        line: Vec<wire::FlowCell>,
        person: &Arc<Redactor>,
    ) -> Result<wire::FlowTaskRow> {
        let scrub = Scrub::of(self, person, &facts.task.id)?;
        Ok(wire::FlowTaskRow {
            id: facts.task.id.clone(),
            name: facts
                .brief
                .as_ref()
                .map_or_else(String::new, |b| scrub.text(&b.outcome, 160)),
            kind: facts
                .brief
                .as_ref()
                .map_or_else(String::new, |b| wire_name(&b.kind)),
            engine: facts.route.as_ref().map(|r| r.engine.as_str().to_owned()),
            owner: facts
                .run
                .as_ref()
                .map_or_else(String::new, |r| scrub.text(&r.member, 60)),
            state: facts.task.state.clone(),
            round: round(facts),
            cells: line,
            created_at: facts.task.created_at.clone(),
            updated_at: facts.task.updated_at.clone(),
        })
    }

    /// One work task zoomed: its lanes, findings, gates and rounds.
    pub fn crew_flow_task(&self, id: &str) -> Result<wire::FlowTaskDetail> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let task = self
            .store
            .crew_task(id)?
            .filter(|_| self.store.crew_task_is_work(id).unwrap_or(false))
            .ok_or_else(|| CoreError::NotFound("task".into()))?;
        let finished = is_final(&task.state);
        let person = self.display_redactor()?;
        let facts = self.flow_facts(task)?;
        let scrub = Scrub::of(self, &person, id)?;
        let line = cells(&facts);
        let row = self.flow_row(&facts, line, &person)?;
        let mut truncated = false;

        let findings_all = facts.review["findings"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut lanes = Vec::new();
        for (lane, round, state, task) in latest_rounds(&facts.review) {
            let mine = |f: &&Value| f["lane"] == lane.as_str() && f["round"] == round;
            let of = |check: &str| {
                findings_all
                    .iter()
                    .filter(mine)
                    .filter(|f| f["check"] == check)
                    .count()
            };
            let route = self.store.crew_route_decisions(&task)?.pop();
            lanes.push(wire::FlowLane {
                group: if is_security(&lane) {
                    "security"
                } else {
                    "code"
                }
                .to_owned(),
                findings: u32::try_from(findings_all.iter().filter(mine).count()).unwrap_or(0),
                confirmed: u32::try_from(of("confirmed")).unwrap_or(0),
                unconfirmed: u32::try_from(of("unconfirmed")).unwrap_or(0),
                engine: route.as_ref().map(|r| r.engine.as_str().to_owned()),
                model: route.as_ref().map(|r| scrub.text(r.model.as_str(), 80)),
                lane,
                task,
                round,
                state,
            });
        }
        if lanes.len() > LANES_MAX {
            lanes.truncate(LANES_MAX);
            truncated = true;
        }

        let text =
            |f: &Value, key: &str, max: usize| scrub.text(f[key].as_str().unwrap_or(""), max);
        let mut findings: Vec<wire::FlowFinding> = findings_all
            .iter()
            .map(|f| wire::FlowFinding {
                id: f["id"].as_str().unwrap_or("").to_owned(),
                lane: text(f, "lane", 60),
                round: u32::try_from(f["round"].as_u64().unwrap_or(1)).unwrap_or(1),
                severity: text(f, "severity", 20),
                path: text(f, "path", TEXT_MAX),
                line: u32::try_from(f["line"].as_u64().unwrap_or(0)).unwrap_or(0),
                claim: text(f, "claim", TEXT_MAX),
                check: text(f, "check", 20),
                status: text(f, "status", 20),
                blocks: f["blocks"] == true,
                fix_task: f["fix_task"].as_str().map(str::to_owned),
            })
            .collect();
        if findings.len() > FINDINGS_MAX {
            findings.truncate(FINDINGS_MAX);
            truncated = true;
        }

        let skip = facts.gates.len().saturating_sub(GATES_MAX);
        truncated |= skip > 0;
        let gates = facts
            .gates
            .iter()
            .skip(skip)
            .map(|g| wire::FlowGate {
                stage: wire_name(&g.stage),
                decision: wire_name(&g.decision),
                decided_by: wire_name(&g.decided_by),
                lane: g.lane.as_ref().map(|l| l.as_str().to_owned()),
                // A reason can quote what a check ran; it is read only once the task is final.
                reason: finished.then(|| scrub.text(g.reason.as_str(), TEXT_MAX)),
                at: g.created_at.as_str().to_owned(),
            })
            .collect();

        let mut rounds = vec![wire::FlowRound {
            n: 1,
            task: facts.task.id.clone(),
            state: facts.task.state.clone(),
        }];
        for (n, (fix, state)) in (2u32..).zip(&facts.fixes) {
            rounds.push(wire::FlowRound {
                n,
                task: fix.fix_task_id.clone(),
                state: state.clone(),
            });
        }

        let merge = match (&facts.entry, &facts.record) {
            (Some(e), _) => Some((
                e.state.clone(),
                e.risk.clone(),
                strings(&e.reasons),
                e.mode.clone(),
            )),
            (None, Some(r)) => Some((
                "merged".to_owned(),
                r.risk.clone(),
                strings(&r.reasons),
                r.mode.clone(),
            )),
            (None, None) => None,
        };
        Ok(wire::FlowTaskDetail {
            brief: wire::FlowBrief {
                purpose: facts
                    .brief
                    .as_ref()
                    .map_or_else(String::new, |b| scrub.text(&b.outcome, 600)),
                files: facts.brief.as_ref().map_or_else(Vec::new, |b| {
                    b.files
                        .iter()
                        .take(FILES_MAX)
                        .map(|g| scrub.text(g.path.as_str(), 200))
                        .collect()
                }),
                budget_turns: facts.brief.as_ref().map_or(0, |b| b.budget.turns),
                budget_minutes: facts.brief.as_ref().map_or(0, |b| b.budget.minutes),
            },
            route: facts.route.as_ref().map(|r| wire::FlowRoute {
                engine: r.engine.as_str().to_owned(),
                model: scrub.text(r.model.as_str(), 80),
                tier: wire_name(&r.tier),
            }),
            lanes,
            findings,
            gates,
            rounds,
            merge: merge.map(|(state, risk, reasons, mode)| wire::FlowMerge {
                state,
                risk,
                reasons: scrub.texts(&reasons, TEXT_MAX),
                mode,
            }),
            watch: facts.record.as_ref().map(|r| r.watch.clone()),
            you: facts.record.as_ref().map(|r| wire::FlowYou {
                review_state: r.review_state.clone(),
                due_unix: r.due_unix,
                needs_person: r.needs_person.as_deref().map(|n| scrub.text(n, TEXT_MAX)),
            }),
            row,
            truncated,
        })
    }
}
