//! The confined Codex worker: its gate, its private home and Codex's own sandbox turned off.

use std::path::{Path, PathBuf};

use serde_json::json;
use tokio::sync::mpsc;

use super::turn::{thread_params, turn_params};
use super::{CodexDriver, confined_home};
use crate::agents::driver::{EngineDriver, RunKind, RunSpec};
use crate::host::confine::{CODEX_REFUSED, Confinement};
use orbyn_contracts::PermissionMode;

const TASK: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn temp() -> (crate::testing::TempDir, PathBuf) {
    let dir = crate::testing::TempDir::new();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    (dir, base)
}

fn rules(base: &Path) -> (PathBuf, Confinement) {
    let data = base.join("data");
    for folder in ["runs/mine", "workspace/repo/p"] {
        std::fs::create_dir_all(data.join(folder)).unwrap();
    }
    let real = base.join("real-codex");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(
        real.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-fake-api-key"}"#,
    )
    .unwrap();
    let configured = Confinement::new(
        &data,
        &data.join("runs/mine"),
        &[],
        &[&data.join("workspace/repo/p")],
    )
    .unwrap()
    .with_codex_worker_home_at(TASK, &real)
    .unwrap();
    (data, configured)
}

fn spec(confine: Option<Confinement>, base: &Path) -> RunSpec {
    RunSpec {
        run_id: "r".into(),
        thread_id: "t".into(),
        cwd: base.join("data/workspace/repo/p"),
        checkout: base.join("data/workspace/repo/p"),
        library: base.join("data/workspace/lib"),
        prompt: "p".into(),
        user_prompt: "p".into(),
        context: "<orbyn_context>c</orbyn_context>".into(),
        kind: RunKind::Turn,
        mode: PermissionMode::Supervised,
        plan: false,
        model: Some("gpt-test".into()),
        effort: None,
        options: std::collections::BTreeMap::new(),
        resume: None,
        mcp_url: Some("http://127.0.0.1:1/mcp/x".into()),
        run_dir: base.join("data/runs/mine"),
        confine,
        crew_task: Some(TASK.to_owned()),
    }
}

#[test]
fn the_gate_still_refuses_codex_but_the_confined_launch_path_does_not() {
    assert!(!crate::host::confine::CODEX_WORKERS_ALLOWED);
    let (_dir, base) = temp();
    let (_data, configured) = rules(&base);
    let mut spec = spec(Some(configured), &base);
    assert_eq!(
        confined_home(&spec, false).unwrap_err().to_string(),
        CODEX_REFUSED
    );
    let home = confined_home(&spec, true)
        .expect("the gate and the home pass")
        .expect("a confined worker has a home");
    assert!(
        home.starts_with(base.join("data/crew/codex-homes")),
        "{home:?}"
    );
    // Without the private home it refuses rather than running unconfined.
    spec.confine.as_mut().unwrap().codex_home = None;
    assert!(confined_home(&spec, true).is_err());
    // A crew worker with no rules at all is never started unconfined.
    spec.confine = None;
    assert!(confined_home(&spec, true).is_err());
}

#[test]
fn a_confined_worker_turns_codex_s_own_sandbox_off_and_keeps_the_saved_approval_policy() {
    let (_dir, base) = temp();
    let (_data, configured) = rules(&base);
    let mut spec = spec(Some(configured), &base);
    for mode in [PermissionMode::Supervised, PermissionMode::Auto] {
        spec.mode = mode;
        let thread = thread_params(&spec, "workspace-write");
        assert_eq!(thread["sandbox"], "danger-full-access", "{thread}");
        assert_eq!(thread["approvalPolicy"], "on-request", "{thread}");
        let params = turn_params(
            &spec,
            "workspace-write",
            "thread-1",
            &json!({ "model": "gpt-test" }),
        )
        .unwrap();
        assert_eq!(
            params["sandboxPolicy"],
            json!({ "type": "dangerFullAccess" }),
            "{params}"
        );
        assert_eq!(params["approvalPolicy"], "on-request", "{params}");
        assert_eq!(params["threadId"], "thread-1");
    }
    // An unconfined run keeps its normal sandbox and workspace root.
    spec.confine = None;
    let thread = thread_params(&spec, "workspace-write");
    assert_eq!(thread["sandbox"], "workspace-write", "{thread}");
    let params = turn_params(
        &spec,
        "workspace-write",
        "thread-1",
        &json!({ "model": "gpt-test" }),
    )
    .unwrap();
    assert_eq!(
        params["sandboxPolicy"]["type"], "workspaceWrite",
        "{params}"
    );
    assert_eq!(
        params["sandboxPolicy"]["writableRoots"],
        json!([spec.checkout.clone()]),
        "{params}"
    );
}

#[tokio::test]
async fn start_run_refuses_a_confined_worker_while_the_gate_is_off() {
    let (_dir, base) = temp();
    let (_data, configured) = rules(&base);
    let spec = spec(Some(configured), &base);
    let driver = CodexDriver::new(
        "codex-not-installed-for-this-test".into(),
        "workspace-write".into(),
    );
    let (events, _receiver) = mpsc::channel(1);
    let Err(error) = driver.start_run(spec, events).await else {
        panic!("a confined worker started while the gate was off");
    };
    assert_eq!(error.to_string(), CODEX_REFUSED);
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_confined_worker_starts_when_the_gate_and_its_home_pass() {
    if crate::host::confine::unavailable().is_some() {
        return;
    }
    let (_dir, base) = temp();
    let (_data, configured) = rules(&base);
    let spec = spec(Some(configured), &base);
    let driver = CodexDriver::new("/usr/bin/true".into(), "workspace-write".into());
    let (events, _receiver) = mpsc::channel(4);
    let handle = driver
        .start_run_with(spec, events, true)
        .await
        .expect("a confined worker starts");
    handle.cancel.cancel();
    handle.task.abort();
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn a_confined_codex_worker_is_refused_by_the_machine_not_the_old_gate() {
    let (_dir, base) = temp();
    let (_data, configured) = rules(&base);
    let spec = spec(Some(configured), &base);
    let program = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let driver = CodexDriver::new(program, "workspace-write".into());
    let (events, _receiver) = mpsc::channel(1);
    let Err(error) = driver.start_run_with(spec, events, true).await else {
        panic!("a confined worker started without a restriction");
    };
    assert_ne!(error.to_string(), CODEX_REFUSED);
    assert!(error.to_string().contains("macOS"), "{error}");
}
