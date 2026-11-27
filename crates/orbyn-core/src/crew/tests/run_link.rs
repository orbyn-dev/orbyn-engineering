//! The Run tab's link read (Crew Preview): which Crew task made one pull request. It answers from the task's own forge
//! intent, so a number the project's own repository never published, another repository's number of the same value, an
//! intent the forge never numbered and a project the service does not know link nothing. No engines start here: the read
//! only reads what the store holds.

use std::path::Path;

use orbyn_contracts::{CrewId, CrewMember, CrewTimestamp, CrewToken};
use serde_json::json;

use crate::CoreConfig;
use crate::app::Core;
use crate::crew::Stamper;
use crate::error::CoreError;
use crate::store::{ForgeMerge, ForgePhase, NewProject};
use crate::testing::TempDir;

/// A core with the Preview flag as asked, holding nothing.
fn core(preview: bool) -> (TempDir, Core) {
    let dir = TempDir::new();
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.crew_preview = preview;
    (dir, Core::open(config).unwrap())
}

/// A stored work task under `label`: a brief and the key's one check.
fn task(core: &Core, label: &str) -> String {
    let stamper = Stamper {
        crew_id: CrewId::new("crew-1").unwrap(),
        engine: CrewToken::new("claude").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&json!({
            "kind": "implement",
            "outcome": format!("Task {label}."),
            "inputs": [],
            "files": [{ "path": "src/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key = stamper
        .answer_key(
            &json!({
                "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": "echo done" },
                    "proves": "It runs." }],
                "protected_paths": ["tests/**"]
            }),
            &brief.id,
        )
        .unwrap();
    core.store
        .create_crew_task(&brief, &key, &format!("run-{label}"))
        .unwrap()
        .id
}

/// A project at `path`. Returns its id, and the path the store kept for it.
fn project(core: &Core, path: &Path) -> (String, String) {
    let id = core
        .store
        .add_project(NewProject {
            path: &path.to_string_lossy(),
            name: "project",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id;
    let kept = core.store.project(&id).unwrap().path;
    (id, kept)
}

/// A forge intent for `task` in the repository at `repo`, published as pull request `pull` when the forge numbered it.
fn intent(core: &Core, task: &str, repo: &str, pull: Option<u64>) {
    core.store
        .add_crew_forge_merge(&ForgeMerge {
            task_id: task.to_owned(),
            repo: repo.to_owned(),
            remote: "origin".into(),
            branch: "main".into(),
            source: "crew/task-1".into(),
            candidate: "a".repeat(40),
            base: "b".repeat(40),
            combined: None,
            pull,
            phase: ForgePhase::Prepared,
            merge_commit: None,
            evidence: Vec::new(),
            note: None,
            merge_sent: false,
            revision: 0,
        })
        .unwrap();
}

/// A project with one task whose intent published `number`.
fn linked(preview: bool, number: u64) -> (TempDir, Core, String, String) {
    let (dir, core) = core(preview);
    let (project_id, path) = project(&core, &dir.path().join("checkout"));
    let task = task(&core, "linked");
    intent(&core, &task, &path, Some(number));
    (dir, core, project_id, task)
}

#[test]
fn the_link_is_the_task_whose_intent_published_that_number() {
    let (_dir, core, project_id, task) = linked(true, 142);
    let link = core.crew_run_link(&project_id, 142).unwrap();
    assert_eq!(link.task, task);
}

#[test]
fn a_number_the_project_never_published_links_nothing() {
    let (_dir, core, project_id, _) = linked(true, 142);
    assert!(matches!(
        core.crew_run_link(&project_id, 143),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn another_repositorys_number_of_the_same_value_links_nothing() {
    let (_dir, core, project_id, _) = linked(true, 142);
    // A second project, whose own task published #142 in its own repository.
    let (other_id, other_path) = project(&core, &_dir.path().join("other"));
    let other = task(&core, "other");
    intent(&core, &other, &other_path, Some(142));
    // Each project answers its own task, never the other's.
    assert_eq!(core.crew_run_link(&other_id, 142).unwrap().task, other);
    assert_ne!(core.crew_run_link(&project_id, 142).unwrap().task, other);
}

#[test]
fn an_intent_the_forge_never_numbered_links_nothing() {
    let (dir, core) = core(true);
    let (project_id, path) = project(&core, &dir.path().join("checkout"));
    let task = task(&core, "unnumbered");
    intent(&core, &task, &path, None);
    assert!(matches!(
        core.crew_run_link(&project_id, 142),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn an_unknown_project_links_nothing() {
    let (_dir, core, _, _) = linked(true, 142);
    assert!(matches!(
        core.crew_run_link("sha256:no-such-project", 142),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn with_the_flag_off_the_read_is_not_there() {
    let (_dir, core, project_id, _) = linked(false, 142);
    assert!(matches!(
        core.crew_run_link(&project_id, 142),
        Err(CoreError::NotFound(_))
    ));
}
