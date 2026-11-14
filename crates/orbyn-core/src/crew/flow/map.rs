//! The Crew Map read (Crew Preview): the lead and the workers of a crew as nodes, who has written to whom as edges, and what the
//! engine registry says of each engine. It reads the store's read connection and the registry's cached statuses; it starts no
//! process, reads no model list and writes nothing. A node shows the brief's outcome in place of a live step, and a worker's parent is
//! the lead (depth one). It never returns a message's text, a thread id, a worktree path, a check or a count of tokens.

use std::collections::{BTreeMap, BTreeSet};

use orbyn_contracts as wire;
use orbyn_contracts::{FlowDeliveryMode, FlowEngineStatus, FlowQuiet, FlowRole, FlowSubagents};

use super::scrub::Scrub;
use crate::agents::provider_facts::{ProviderFacts, Unavailable};
use crate::app::Core;
use crate::crew::{valid_id, wire_name};
use crate::error::{CoreError, Result};

/// The most nodes and edges a map shows.
pub const NODES_MAX: usize = 64;
pub const EDGES_MAX: usize = 200;
const LEASE_MAX: usize = 3;
/// A running worker with no event for this long is quiet, and for this long may be stuck.
const QUIET_SECS: i64 = 5 * 60;
const STUCK_SECS: i64 = 15 * 60;

/// How long a running task has been silent. Only a task being worked on can be quiet: a task that is verifying has a check running.
pub(super) fn quiet_of(state: &str, idle_secs: Option<i64>) -> FlowQuiet {
    match (state, idle_secs) {
        ("running" | "changed_approach", Some(idle)) if idle >= STUCK_SECS => FlowQuiet::MayBeStuck,
        ("running" | "changed_approach", Some(idle)) if idle >= QUIET_SECS => FlowQuiet::Quiet,
        _ => FlowQuiet::Active,
    }
}

/// Engines whose driver turns the engine's own subagents into `subagent.activity` events (`agents/driver/claude/events.rs`,
/// `agents/driver/codex/events.rs`). The driver has no capability flag for it, so this keys on the driver's family; any other engine,
/// whatever its tier, says "not reported".
const SUBAGENT_FAMILIES: [&str; 2] = ["claude", "codex"];

pub(super) fn reports_subagents(tier: orbyn_contracts::EngineTier, family: &str) -> bool {
    tier == orbyn_contracts::EngineTier::Native && SUBAGENT_FAMILIES.contains(&family)
}

pub(super) fn delivery_of(mode: Option<&str>) -> Option<FlowDeliveryMode> {
    match mode? {
        "live" => Some(FlowDeliveryMode::Live),
        "next_turn" => Some(FlowDeliveryMode::NextTurn),
        "tool_result" => Some(FlowDeliveryMode::ToolResult),
        "held" => Some(FlowDeliveryMode::Held),
        "failed" => Some(FlowDeliveryMode::Failed),
        _ => None,
    }
}

/// What the core calls a message of a kind. Never a word of the message.
pub(super) fn label_of(kind: &str) -> &'static str {
    match kind {
        "fact" => "a fact",
        "question" => "a question",
        "handoff" => "a handoff",
        "blocker" => "a blocker",
        "answer" => "an answer",
        _ => "a message",
    }
}

fn status_of(facts: &ProviderFacts) -> FlowEngineStatus {
    match facts.unavailable() {
        None => FlowEngineStatus::Ready,
        Some(Unavailable::NotInstalled) => FlowEngineStatus::NotInstalled,
        Some(Unavailable::SignedOut) => FlowEngineStatus::SignedOut,
        Some(Unavailable::SignInUnconfirmed) => FlowEngineStatus::SignInUnconfirmed,
        Some(Unavailable::Broken(_)) => FlowEngineStatus::Broken,
    }
}

impl Core {
    /// One crew's map. Up to 64 nodes (the lead first, then the newest workers) and 200 edges.
    pub async fn crew_flow_map(&self, crew: &str) -> Result<wire::FlowMap> {
        self.crew_preview_only()?;
        valid_id(crew)?;
        let person = self.display_redactor()?;
        let scrub = Scrub::of_crew(self, &person, crew)?;
        let mut tasks = self.store.crew_map_tasks(crew, (NODES_MAX + 1) as u32)?;
        let mut truncated = tasks.len() > NODES_MAX;
        tasks.truncate(NODES_MAX);

        // The registry's statuses, by engine id. Never `refresh = true` from a route: the first call may probe the engines once under the
        // statuses lock and every later call is the cached list, as the Providers list reads it. No model list is read.
        let statuses: BTreeMap<String, (FlowEngineStatus, bool)> = self
            .engines
            .statuses(false)
            .await
            .map_err(|_| CoreError::Internal("the engines could not be read".into()))?
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    (
                        status_of(&ProviderFacts::of(s)),
                        reports_subagents(s.tier, s.instance_of.as_deref().unwrap_or(&s.id)),
                    ),
                )
            })
            .collect();
        let engine_status = |engine: &str| {
            statuses
                .get(engine)
                .map_or(FlowEngineStatus::SignInUnconfirmed, |(status, _)| *status)
        };

        // Who was told what and is still waiting, and how long each worker has been silent: one bounded query each.
        let held: BTreeMap<String, u32> = self.store.crew_held_counts(crew)?.into_iter().collect();
        let mut threads = Vec::new();
        let mut rows = Vec::new();
        for task in tasks {
            let Some(run) = self.store.crew_run(&task.id)? else {
                continue;
            };
            if let Some(thread) = &run.worker_thread_id {
                threads.push(thread.clone());
            }
            let brief = self.store.crew_brief(&task.id)?;
            let route = self.store.crew_route_decisions(&task.id)?.pop();
            rows.push((task, run, brief, route));
        }
        let last: BTreeMap<String, String> = self
            .store
            .thread_last_events(&threads)?
            .into_iter()
            .collect();
        let now = crate::util::unix_now();

        let lead = rows.iter().find_map(|(_, _, brief, _)| {
            brief
                .as_ref()
                .map(|b| (b.author.engine.as_str().to_owned(), b.author.member.clone()))
        });
        let mut nodes = Vec::new();
        let mut lead_name = None;
        if let Some((engine, member)) = lead {
            let member = member.map_or_else(|| "lead".to_owned(), |m| m.as_str().to_owned());
            lead_name = Some(scrub.text(&member, 60));
            nodes.push(wire::FlowNode {
                member: scrub.text(&member, 60),
                role: FlowRole::Lead,
                task: None,
                engine: scrub.text(&engine, 60),
                model: None,
                effort: None,
                state: None,
                quiet: FlowQuiet::Active,
                engine_status: engine_status(&engine),
                subagents: if statuses.get(&engine).is_some_and(|(_, native)| *native) {
                    FlowSubagents::Reported
                } else {
                    FlowSubagents::NotReported
                },
                outcome: String::new(),
                lease: Vec::new(),
                read_only: false,
                unread: held.get(&member).copied().unwrap_or(0),
                started_by: None,
                created_at: None,
                updated_at: None,
            });
        }
        for (task, run, brief, route) in rows {
            let engine = route
                .as_ref()
                .map_or_else(|| "none".to_owned(), |r| r.engine.as_str().to_owned());
            let idle = run
                .worker_thread_id
                .as_ref()
                .and_then(|t| last.get(t))
                .and_then(|ts| crate::util::parse_time(ts))
                .map(|at| now.saturating_sub(at).max(0));
            let grants = brief.as_ref().map(|b| &b.files);
            nodes.push(wire::FlowNode {
                member: scrub.text(&run.member, 60),
                role: FlowRole::Worker,
                task: Some(task.id.clone()),
                model: route.as_ref().map(|r| scrub.text(r.model.as_str(), 80)),
                effort: route.as_ref().map(|r| wire_name(&r.effort)),
                state: Some(task.state.clone()),
                quiet: quiet_of(&task.state, idle),
                engine_status: engine_status(&engine),
                subagents: if statuses.get(&engine).is_some_and(|(_, native)| *native) {
                    FlowSubagents::Reported
                } else {
                    FlowSubagents::NotReported
                },
                outcome: brief
                    .as_ref()
                    .map_or_else(String::new, |b| scrub.text(&b.outcome, 160)),
                lease: grants.map_or_else(Vec::new, |g| {
                    g.iter()
                        .filter(|g| g.mode == orbyn_contracts::LeaseMode::Write)
                        .take(LEASE_MAX)
                        .map(|g| scrub.text(g.path.as_str(), 120))
                        .collect()
                }),
                read_only: grants.is_some_and(|g| {
                    !g.iter()
                        .any(|g| g.mode == orbyn_contracts::LeaseMode::Write)
                }),
                unread: held.get(&run.member).copied().unwrap_or(0),
                started_by: lead_name.clone(),
                created_at: Some(task.created_at.clone()),
                updated_at: Some(task.updated_at.clone()),
                engine: scrub.text(&engine, 60),
            });
        }

        let mut edges: Vec<wire::FlowEdge> = self
            .store
            .crew_message_edges(crew, (EDGES_MAX + 1) as u32)?
            .into_iter()
            .map(|e| wire::FlowEdge {
                from: scrub.text(&e.sender, 60),
                to: scrub.text(&e.recipient, 60),
                label: label_of(&e.kind).to_owned(),
                kind: scrub.text(&e.kind, 20),
                count: e.count,
                delivery: delivery_of(e.mode.as_deref()),
            })
            .collect();
        if edges.len() > EDGES_MAX {
            edges.truncate(EDGES_MAX);
            truncated = true;
        }
        let seen: BTreeSet<String> = nodes.iter().map(|n| n.engine.clone()).collect();
        let engines = seen
            .into_iter()
            .map(|engine| wire::FlowEngineRow {
                status: engine_status(&engine),
                engine,
            })
            .collect();
        Ok(wire::FlowMap {
            crew: crew.to_owned(),
            nodes,
            edges,
            engines,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_running_worker_can_be_quiet_and_the_thresholds_are_five_and_fifteen_minutes() {
        assert_eq!(quiet_of("running", Some(299)), FlowQuiet::Active);
        assert_eq!(quiet_of("running", Some(300)), FlowQuiet::Quiet);
        assert_eq!(quiet_of("changed_approach", Some(899)), FlowQuiet::Quiet);
        assert_eq!(quiet_of("running", Some(900)), FlowQuiet::MayBeStuck);
        assert_eq!(quiet_of("running", None), FlowQuiet::Active);
        for state in [
            "briefed",
            "routed",
            "verifying",
            "needs_person",
            "passed",
            "stopped",
            "cancelled",
        ] {
            assert_eq!(quiet_of(state, Some(10_000)), FlowQuiet::Active, "{state}");
        }
    }

    /// A GET never forces a probe of every engine: the map reads the registry's statuses without a refresh.
    #[test]
    fn the_map_never_asks_the_registry_to_refresh() {
        let forced = ["statuses(", "true)"].concat();
        assert!(!include_str!("map.rs").contains(&forced));
    }

    #[test]
    fn only_the_native_drivers_that_emit_subagent_events_report_them() {
        use orbyn_contracts::EngineTier as Tier;
        assert!(reports_subagents(Tier::Native, "claude"));
        assert!(reports_subagents(Tier::Native, "codex"));
        assert!(
            !reports_subagents(Tier::Native, "alpha"),
            "another native engine"
        );
        assert!(!reports_subagents(Tier::Acp, "opencode"));
        assert!(
            !reports_subagents(Tier::Acp, "claude"),
            "an ACP setup of a family is not its driver"
        );
        assert!(!reports_subagents(Tier::Agui, "claude"));
        assert!(!reports_subagents(Tier::Terminal, "claude"));
        assert!(!reports_subagents(Tier::Agui, "remote"));
    }

    #[test]
    fn a_message_is_labelled_by_its_kind_and_never_by_its_words() {
        for (kind, label) in [
            ("fact", "a fact"),
            ("question", "a question"),
            ("handoff", "a handoff"),
            ("blocker", "a blocker"),
            ("answer", "an answer"),
            ("anything else", "a message"),
        ] {
            assert_eq!(label_of(kind), label);
        }
        assert_eq!(
            delivery_of(Some("next_turn")),
            Some(FlowDeliveryMode::NextTurn)
        );
        assert_eq!(
            delivery_of(Some("tool_result")),
            Some(FlowDeliveryMode::ToolResult)
        );
        assert_eq!(delivery_of(Some("nonsense")), None);
        assert_eq!(delivery_of(None), None);
    }
}
