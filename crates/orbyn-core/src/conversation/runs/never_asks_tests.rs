//! The run-start rules of an engine that never asks (Cursor on the host): its one mode, the refusal of Supervised and Auto
//! edits, the notice once per project and the person's one-turn override (docs/cursor-thread-contract.md). The engine is a
//! stand-in named `cursor`: no real Cursor turn runs here.

use std::sync::Mutex;

use orbyn_contracts::{CreateThread, PermissionMode, RunState, UpdateThread};
use serde_json::{Value, json};

use super::*;
use crate::Core;
use crate::agents::engines::stand_in::StandIn;
use crate::app::CoreConfig;
use crate::testing::TempDir;

struct Rig {
    _dir: TempDir,
    core: Core,
    project: String,
    seen: Arc<Mutex<Vec<PermissionMode>>>,
}

async fn rig(engines: &[(&'static str, Vec<PermissionMode>)]) -> Rig {
    let dir = TempDir::new();
    let root = crate::testing::library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    for (id, modes) in engines {
        core.engines
            .register(StandIn::new(id, modes.clone(), seen.clone()));
    }
    core.workspace
        .setup(orbyn_contracts::WorkspaceSetup {
            url: None,
            adopt_path: Some(root.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    let project = core.new_project("project", false, None).await.unwrap();
    core.store.trust_project(&project.id, "t").unwrap();
    Rig {
        _dir: dir,
        core,
        project: project.id,
        seen,
    }
}

impl Rig {
    async fn thread(&self, project: &str, engine: &str, mode: PermissionMode) -> String {
        let thread = self
            .core
            .create_thread(CreateThread {
                project_id: Some(project.to_owned()),
                engine: engine.into(),
                model: None,
                title: None,
                worktree: None,
                skill_profile: None,
                base_branch: None,
                effort: None,
                options: std::collections::BTreeMap::new(),
                branch_name_seed: None,
            })
            .await
            .unwrap();
        self.save(&thread.id, mode);
        thread.id
    }

    fn save(&self, thread: &str, mode: PermissionMode) {
        let patch = UpdateThread {
            permission_mode: Some(mode),
            ..Default::default()
        };
        self.core.store.update_thread(thread, &patch, "t").unwrap();
    }

    fn acknowledge(&self, project: &str) {
        self.core
            .engines
            .acknowledge_notice(project, "cursor", "person")
            .unwrap();
    }

    async fn ends(&self, run: &str) -> RunState {
        for _ in 0..4000 {
            let state = self.core.store.run(run).unwrap().state;
            if state.is_terminal() {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("run never ended");
    }

    fn audits(&self, action: &str) -> usize {
        self.core
            .store
            .list_audit(None, 200)
            .unwrap()
            .iter()
            .filter(|row| row.action == action)
            .count()
    }
}

fn cursor() -> (&'static str, Vec<PermissionMode>) {
    ("cursor", vec![PermissionMode::NoApprovals])
}

fn refusal(error: CoreError) -> (u16, &'static str, String, Value) {
    match error {
        CoreError::EngineRefused {
            code,
            message,
            fields,
        } => (409, code, message, fields),
        other => panic!("expected a typed refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_supervised_or_auto_edits_cursor_thread_is_refused_at_run_start_with_the_typed_body() {
    let rig = rig(&[cursor()]).await;
    rig.acknowledge(&rig.project);
    for mode in [PermissionMode::Supervised, PermissionMode::AutoEdits] {
        let thread = rig.thread(&rig.project, "cursor", mode).await;
        let error = rig.core.runs.admit(&thread, "go", "t").unwrap_err();
        assert_eq!(error.status(), 409);
        let (_, code, message, fields) = refusal(error);
        assert_eq!(code, "engine_mode_refused");
        assert!(
            message.ends_with('.') && !message.contains('{'),
            "{message}"
        );
        assert_eq!(
            fields,
            json!({
                "engine": "cursor",
                "thread_id": thread,
                "mode": serde_json::to_value(mode).unwrap(),
                "allowed": ["no_approvals"],
            })
        );
        // Nothing started, and the saved mode is untouched: saving is never refused.
        assert!(rig.core.store.thread(&thread).unwrap().active_run.is_none());
        assert_eq!(
            rig.core
                .store
                .thread(&thread)
                .unwrap()
                .lifecycle
                .unwrap()
                .permission_mode,
            mode
        );
    }
    assert!(rig.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn no_approvals_runs_and_so_do_the_modes_that_promise_no_ask() {
    let rig = rig(&[cursor()]).await;
    rig.acknowledge(&rig.project);
    for mode in [
        PermissionMode::NoApprovals,
        PermissionMode::Auto,
        PermissionMode::FullAccess,
    ] {
        let thread = rig.thread(&rig.project, "cursor", mode).await;
        let run = rig.core.runs.admit(&thread, "go", "t").unwrap();
        assert_eq!(rig.ends(&run.id).await, RunState::Completed);
    }
    // The engine runs its one mode whatever the thread saved.
    assert!(
        rig.seen
            .lock()
            .unwrap()
            .iter()
            .all(|mode| *mode == PermissionMode::NoApprovals)
    );
}

#[tokio::test]
async fn goal_command_and_queued_starts_are_refused_the_same_way() {
    let rig = rig(&[cursor()]).await;
    rig.acknowledge(&rig.project);
    let thread = rig
        .thread(&rig.project, "cursor", PermissionMode::Supervised)
        .await;
    let goal = rig
        .core
        .runs
        .admit(&thread, "/goal ship it", "t")
        .unwrap_err();
    assert_eq!(refusal(goal).1, "engine_mode_refused");
    let queued = rig.core.runs.enqueue(&thread, "later", &[]).unwrap_err();
    assert_eq!(refusal(queued).1, "engine_mode_refused");
    assert!(rig.core.store.list_queue(&thread).unwrap().is_empty());
}

#[tokio::test]
async fn the_notice_comes_first_and_is_per_project_and_idempotent() {
    let rig = rig(&[cursor()]).await;
    let thread = rig
        .thread(&rig.project, "cursor", PermissionMode::Supervised)
        .await;
    // Both refusals apply: the notice is the one given.
    let (status, code, _, fields) = refusal(rig.core.runs.admit(&thread, "go", "t").unwrap_err());
    assert_eq!((status, code), (409, "engine_notice_required"));
    assert_eq!(
        fields,
        json!({ "engine": "cursor", "project_id": rig.project })
    );
    let before = rig.core.engines.notice(&rig.project, "cursor").unwrap();
    assert!(!before.acknowledged && before.acknowledged_at.is_none());

    let first = rig
        .core
        .engines
        .acknowledge_notice(&rig.project, "cursor", "person")
        .unwrap();
    assert!(first.acknowledged && first.acknowledged_at.is_some());
    let again = rig
        .core
        .engines
        .acknowledge_notice(&rig.project, "cursor", "person")
        .unwrap();
    assert_eq!(again, first, "a second acknowledgement changes nothing");
    assert_eq!(rig.audits("engine_notice.acknowledge"), 1, "one audit row");
    assert_eq!(
        rig.core.engines.notice(&rig.project, "cursor").unwrap(),
        first
    );
    // Acknowledged: now the mode is what refuses.
    let (_, code, _, _) = refusal(rig.core.runs.admit(&thread, "go", "t").unwrap_err());
    assert_eq!(code, "engine_mode_refused");

    // A second project has not acknowledged.
    let other = rig.core.new_project("second", false, None).await.unwrap();
    rig.core.store.trust_project(&other.id, "t").unwrap();
    assert!(
        !rig.core
            .engines
            .notice(&other.id, "cursor")
            .unwrap()
            .acknowledged
    );
    let elsewhere = rig
        .thread(&other.id, "cursor", PermissionMode::NoApprovals)
        .await;
    let (_, code, _, fields) = refusal(rig.core.runs.admit(&elsewhere, "go", "t").unwrap_err());
    assert_eq!(code, "engine_notice_required");
    assert_eq!(fields["project_id"], json!(other.id));
}

#[tokio::test]
async fn an_engine_with_no_notice_or_a_missing_project_answers_not_found() {
    let rig = rig(&[cursor()]).await;
    let none = rig.core.engines.notice(&rig.project, "claude").unwrap_err();
    assert_eq!(none.status(), 404);
    let ack = rig
        .core
        .engines
        .acknowledge_notice(&rig.project, "claude", "person")
        .unwrap_err();
    assert_eq!(ack.status(), 404);
    let gone = rig
        .core
        .engines
        .notice("no-such-project", "cursor")
        .unwrap_err();
    assert_eq!(gone.status(), 404);
    assert_eq!(rig.audits("engine_notice.acknowledge"), 0);
}

#[tokio::test]
async fn the_override_runs_one_turn_stores_nothing_and_the_next_send_is_refused_again() {
    let rig = rig(&[cursor()]).await;
    let thread = rig
        .thread(&rig.project, "cursor", PermissionMode::Supervised)
        .await;
    // It does not skip the notice, and a refused attempt leaves no audit row.
    let (_, code, _, _) = refusal(
        rig.core
            .runs
            .admit_without_approvals(&thread, "go", &[], "t")
            .unwrap_err(),
    );
    assert_eq!(code, "engine_notice_required");
    assert_eq!(rig.audits("thread.send_without_approvals"), 0);

    rig.acknowledge(&rig.project);
    let run = rig
        .core
        .runs
        .admit_without_approvals(&thread, "go", &[], "t")
        .unwrap();
    assert_eq!(rig.ends(&run.id).await, RunState::Completed);
    assert_eq!(
        rig.seen.lock().unwrap().as_slice(),
        [PermissionMode::NoApprovals]
    );
    assert_eq!(rig.audits("thread.send_without_approvals"), 1);
    // The saved mode stays, so a plain send is refused again.
    let saved = rig.core.store.thread(&thread).unwrap().lifecycle.unwrap();
    assert_eq!(saved.permission_mode, PermissionMode::Supervised);
    let (_, code, _, _) = refusal(rig.core.runs.admit(&thread, "again", "t").unwrap_err());
    assert_eq!(code, "engine_mode_refused");
}

#[tokio::test]
async fn the_override_is_only_for_an_engine_that_never_asks() {
    let rig = rig(&[("asker", PermissionMode::ALL.to_vec())]).await;
    let thread = rig
        .thread(&rig.project, "asker", PermissionMode::Supervised)
        .await;
    let error = rig
        .core
        .runs
        .admit_without_approvals(&thread, "go", &[], "t")
        .unwrap_err();
    assert!(matches!(error, CoreError::Unsupported(_)), "{error:?}");
    assert_eq!(rig.audits("thread.send_without_approvals"), 0);
}

#[tokio::test]
async fn other_engines_keep_their_fallback_to_supervised_and_are_never_refused() {
    use PermissionMode::{Auto, AutoEdits, FullAccess, Supervised};
    let modes = vec![Supervised, AutoEdits, FullAccess];
    let rig = rig(&[("opener", modes)]).await;
    // No notice applies to them either.
    for saved in [Supervised, Auto, FullAccess] {
        let thread = rig.thread(&rig.project, "opener", saved).await;
        let run = rig.core.runs.admit(&thread, "go", "t").unwrap();
        assert_eq!(rig.ends(&run.id).await, RunState::Completed);
    }
    assert_eq!(
        rig.seen.lock().unwrap().as_slice(),
        [Supervised, Supervised, FullAccess]
    );
}

#[tokio::test]
async fn the_status_says_one_mode_and_reviewer_only_for_the_engine_that_never_asks() {
    use orbyn_contracts::EngineCrewRole;
    let rig = rig(&[cursor(), ("asker", PermissionMode::ALL.to_vec())]).await;
    let status = rig.core.engines.status("cursor").await.unwrap();
    assert_eq!(
        status.capabilities.permission_modes,
        [PermissionMode::NoApprovals]
    );
    assert_eq!(status.crew_role, EngineCrewRole::ReviewerOnly);
    assert_eq!(
        status.crew_role_reason.as_deref(),
        Some("Not yet confined for writing work")
    );
    let wire = serde_json::to_value(&status).unwrap();
    assert_eq!(wire["crew_role"], "reviewer_only");
    assert_eq!(
        wire["capabilities"]["permission_modes"],
        json!(["no_approvals"])
    );
    // Every other engine is unchanged.
    let other = rig.core.engines.status("asker").await.unwrap();
    assert_eq!(other.capabilities.permission_modes, PermissionMode::ALL);
    assert_eq!(other.crew_role, EngineCrewRole::Any);
    assert_eq!(other.crew_role_reason, None);
    let wire = serde_json::to_value(&other).unwrap();
    assert_eq!(wire["crew_role"], "any");
    assert_eq!(wire["crew_role_reason"], Value::Null);
}

/// Every engine Orbyn cannot ask before it acts is treated by what its driver reports, not by name: one mode, the notice
/// first, then Supervised and Auto edits refused at run start with the typed body, and a saved mode left alone.
#[tokio::test]
async fn every_engine_that_cannot_ask_refuses_supervised_and_auto_edits_at_run_start() {
    use crate::agents::driver::acp::AcpDriver;
    use crate::agents::engines::known::{KNOWN_ENGINES, known_engine};
    let silent: Vec<&str> = KNOWN_ENGINES
        .iter()
        .filter(|known| !known.asks)
        .map(|known| known.id)
        .collect();
    for id in [
        "kiro",
        "droid",
        "glm",
        "mistral-vibe",
        "omp",
        "deepseek",
        "fx",
    ] {
        assert!(silent.contains(&id), "{id} must be in the silent list");
    }
    let rig = rig(&[]).await;
    for id in silent {
        let known = known_engine(id).unwrap();
        rig.core
            .engines
            .register(Arc::new(AcpDriver::known(known.as_custom(), known)));
        let thread = rig
            .thread(&rig.project, id, PermissionMode::Supervised)
            .await;
        // The notice comes first, as for Cursor.
        let first = rig.core.runs.admit(&thread, "go", "t").unwrap_err();
        assert_eq!(refusal(first).1, "engine_notice_required", "{id}");
        rig.core
            .engines
            .acknowledge_notice(&rig.project, id, "person")
            .unwrap();
        for mode in [PermissionMode::Supervised, PermissionMode::AutoEdits] {
            rig.save(&thread, mode);
            let error = rig.core.runs.admit(&thread, "go", "t").unwrap_err();
            assert_eq!(error.status(), 409, "{id}");
            let (_, code, message, fields) = refusal(error);
            assert_eq!(code, "engine_mode_refused", "{id}");
            assert!(
                message.ends_with('.') && !message.contains('{'),
                "{message}"
            );
            assert_eq!(
                fields,
                json!({
                    "engine": id,
                    "thread_id": thread,
                    "mode": serde_json::to_value(mode).unwrap(),
                    "allowed": ["no_approvals"],
                }),
                "{id}"
            );
            // The saved mode is untouched: it is refused at start, never silently changed.
            assert_eq!(
                rig.core
                    .store
                    .thread(&thread)
                    .unwrap()
                    .lifecycle
                    .unwrap()
                    .permission_mode,
                mode,
                "{id}"
            );
        }
    }
}
