//! Rows and a repository for the HTTP tests of `orbynd`, kept here because persistence and git belong to the core (docs/refactor.md).
//! Built only for tests (`test-support`).

use std::path::Path;

use serde_json::json;

use crate::store::{MergeEntry, MergeRecord};

/// A merge that reached `branch` in `repo`, with `text` in each free-text field.
pub fn merge_record(task: &str, repo: &str, branch: &str, text: &str) -> MergeRecord {
    MergeRecord {
        id: 0,
        task_id: task.to_owned(),
        crew_id: "crew-1".to_owned(),
        repo: repo.to_owned(),
        branch: branch.to_owned(),
        before_commit: "a".repeat(40),
        merge_commit: "b".repeat(40),
        candidate: "c".repeat(40),
        risk: "high".to_owned(),
        reasons: json!(["touches CI", text]),
        areas: vec!["src/app".to_owned()],
        engine: "claude".to_owned(),
        mode: "full".to_owned(),
        files: 3,
        lines: 30,
        proof: json!({ "cost": null, "task_checks": [], "note": text }),
        not_verified: json!([text]),
        merged_unix: crate::util::unix_now(),
        due_unix: None,
        review_state: "waiting".to_owned(),
        comment: None,
        watch: "unknown".to_owned(),
        reverted_commit: None,
        needs_person: None,
    }
}

/// A queue entry waiting for a person, with `text` in each free-text field.
pub fn merge_entry(task: &str, repo: &str, text: &str) -> MergeEntry {
    MergeEntry {
        task_id: task.to_owned(),
        crew_id: "crew-1".to_owned(),
        repo: repo.to_owned(),
        branch: "main".to_owned(),
        candidate: "c".repeat(40),
        state: "waiting".to_owned(),
        risk: "high".to_owned(),
        reasons: json!([text]),
        areas: vec!["src".to_owned()],
        samples: vec![text.to_owned()],
        engine: "claude".to_owned(),
        files: 1,
        lines: 1,
        mode: "full".to_owned(),
        note: Some(text.to_owned()),
        released: false,
        queued_unix: 1,
    }
}

/// Runs git in `repo` as a fixed test identity and returns its trimmed output; a failure panics.
pub fn git(repo: &Path, args: &[&str]) -> String {
    std::fs::create_dir_all(repo).expect("a folder for the repository");
    let out = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Gives a stored task a worker thread and a checkout, as a running task has, and holds `command` for the person as the
/// chain's denial would. Returns the hold's id. For the HTTP tests of the person's answers to a held command.
pub fn hold_command(core: &crate::Core, task_id: &str, checkout: &Path, command: &str) -> String {
    use crate::governance::approvals::ActionAsk;
    core.store
        .add_crew_run(task_id, "lead-thread", "member-01")
        .unwrap();
    let worker = format!("worker-{task_id}");
    core.store
        .set_crew_worker(task_id, &worker, &checkout.to_string_lossy(), None)
        .unwrap();
    let task = core.store.crew_task(task_id).unwrap().unwrap();
    let ask = ActionAsk {
        kind: orbyn_contracts::ActionKind::Command,
        target: command.to_owned(),
        summary: "A request from the worker.".to_owned(),
        detail: json!({}),
        paths: Vec::new(),
    };
    core.held_make(
        &task,
        &worker,
        &ask,
        "The lead said no.",
        &[],
        crate::util::unix_now(),
    )
    .expect("a held command")
}

/// A live project at `path`, as a background crew's repository. Returns its id.
pub fn add_project(core: &crate::Core, path: &str) -> String {
    core.store
        .add_project(crate::store::NewProject {
            path,
            name: "project",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id
}

/// The actions of the audit rows that start with `prefix`, oldest first.
pub fn audit_actions(core: &crate::Core, prefix: &str) -> Vec<String> {
    let mut rows = core.store.list_audit(None, 500).unwrap();
    rows.reverse();
    rows.into_iter()
        .map(|row| row.action)
        .filter(|action| action.starts_with(prefix))
        .collect()
}
