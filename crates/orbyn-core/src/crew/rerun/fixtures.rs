//! A stored task with a checkout, for the HTTP tests of `orbynd` and the core's own: built only for tests (`test-support`), kept here
//! because persistence belongs to the core (docs/refactor.md). It builds the rows a started task has and moves the task along the
//! lifecycle table to the state asked for; nothing starts an engine.

use std::path::Path;

use orbyn_contracts::{AnswerKey, Brief, CrewId, CrewMember, CrewTimestamp, CrewToken};
use serde_json::{Value, json};

use crate::app::Core;
use crate::crew::Stamper;

static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn stamper() -> Stamper {
    Stamper {
        crew_id: CrewId::new("crew-1").unwrap(),
        engine: CrewToken::new("claude").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    }
}

/// A brief of its own (so two tasks in one core are two tasks) and a key with one `command` check per command (`chk-1`, `chk-2`,
/// ...) and a `diff_size` check.
fn parts(commands: &[&str]) -> (Brief, AnswerKey, String) {
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = stamper();
    let brief = stamp
        .brief(&json!({
            "kind": "implement",
            "outcome": format!("Task {n}."),
            "inputs": [],
            "files": [{ "path": "src/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let mut checks: Vec<Value> = commands
        .iter()
        .enumerate()
        .map(|(n, run)| {
            json!({ "id": format!("chk-{}", n + 1), "kind": "command",
                "params": { "run": run }, "proves": "The command exits 0." })
        })
        .collect();
    checks.push(json!({ "id": "chk-size", "kind": "diff_size",
        "params": { "max_files": 1000, "max_lines": 100_000 }, "proves": "The diff stays small." }));
    let key = stamp
        .answer_key(
            &json!({ "checks": checks, "protected_paths": [] }),
            &brief.id,
        )
        .unwrap();
    (brief, key, format!("run-fixture-{n}"))
}

/// One Evidence of a live verification, written without running anything (the key's commands are not for a fixture to run).
fn live_evidence(core: &Core, id: &str) -> Vec<String> {
    use crate::crew::leases::{core_author, timestamp};
    use orbyn_contracts::{
        CheckOutcome, CrewCommit, CrewHash, CrewObject, CrewShortText, Evidence,
    };
    let task = core.store.crew_task(id).unwrap().unwrap();
    let key = core.store.crew_answer_key(id).unwrap().unwrap();
    let mut evidence = Evidence {
        v: 0,
        id: key.id.clone(),
        crew_id: key.crew_id.clone(),
        author: core_author(),
        created_at: timestamp(crate::util::unix_now()).unwrap(),
        run_id: CrewId::new(task.run_id).unwrap(),
        check: key.checks[0].id.clone(),
        claim: None,
        command: CrewShortText::new("a live check").unwrap(),
        outcome: CheckOutcome::Pass,
        exit_code: Some(0),
        duration_ms: 1,
        commit: CrewCommit::new("1".repeat(40)).unwrap(),
        log_hash: CrewHash::new(format!("sha256:{}", "2".repeat(64))).unwrap(),
        excerpt: String::new(),
        artifacts: Vec::new(),
        proves: CrewShortText::new("It runs.").unwrap(),
        does_not_prove: CrewShortText::new("Anything else.").unwrap(),
    };
    evidence.id = CrewObject::Evidence(evidence.clone())
        .computed_id()
        .unwrap();
    core.store
        .add_crew_object(id, &CrewObject::Evidence(evidence.clone()))
        .unwrap();
    vec![evidence.id.as_str().to_owned()]
}

/// Stores a task whose key runs `commands` in `checkout` (a git repository) and brings it to `state`: `running`, `verifying`,
/// `needs_person`, `cancelled`, `passed`, or `stopped` (stopped for `repeated_failure` after a live verification). `stopped:never`
/// is a stop with no live verification, and `stopped:<reason>` one with that reason, after a live verification. A live verification
/// is one Evidence, stored without running a command. The worker thread is named but not stored, so no run of it is open.
/// Returns the task's id.
pub async fn task_in(core: &Core, checkout: &Path, commands: &[&str], state: &str) -> String {
    let (brief, key, run) = parts(commands);
    let id = core.store.create_crew_task(&brief, &key, &run).unwrap().id;
    core.store
        .add_crew_run(&id, "lead-thread", "member-01")
        .unwrap();
    core.store
        .set_crew_worker(
            &id,
            &format!("worker-{id}"),
            &checkout.to_string_lossy(),
            None,
        )
        .unwrap();
    let go = |to: &str| {
        core.store.move_crew_task(&id, to, None).unwrap();
    };
    let stop = |reason: &str| {
        core.store
            .move_crew_task(&id, "stopped", Some(reason))
            .unwrap();
    };
    match state {
        "briefed" => {}
        "running" => {
            go("routed");
            go("running");
        }
        "verifying" => {
            go("routed");
            go("running");
            go("verifying");
        }
        "needs_person" => {
            go("routed");
            go("needs_person");
        }
        "cancelled" => {
            go("routed");
            go("cancelled");
        }
        "passed" => {
            go("routed");
            go("running");
            go("verifying");
            let evidence = live_evidence(core, &id);
            core.store.pass_crew_task(&id, &evidence).unwrap();
        }
        "stopped:never" => {
            go("routed");
            go("running");
            stop("budget");
        }
        other if other == "stopped" || other.starts_with("stopped:") => {
            go("routed");
            go("running");
            go("verifying");
            live_evidence(core, &id);
            stop(other.strip_prefix("stopped:").unwrap_or("repeated_failure"));
        }
        other => panic!("no such state: {other}"),
    }
    id
}

/// A stopped task whose run never recorded a checkout.
pub fn task_without_checkout(core: &Core) -> String {
    let (brief, key, run) = parts(&["exit 0"]);
    let id = core.store.create_crew_task(&brief, &key, &run).unwrap().id;
    core.store
        .add_crew_run(&id, "lead-thread", "member-01")
        .unwrap();
    for to in ["routed", "running"] {
        core.store.move_crew_task(&id, to, None).unwrap();
    }
    core.store
        .move_crew_task(&id, "stopped", Some("repeated_failure"))
        .unwrap();
    id
}
