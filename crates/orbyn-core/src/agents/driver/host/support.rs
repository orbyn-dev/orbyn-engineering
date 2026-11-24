//! Fixtures shared by the host's tests.

use std::path::PathBuf;

use crate::agents::driver::{RunKind, RunSpec};
use crate::host::confine::Confinement;
use crate::testing::TempDir;

/// A run that resumes nothing, starts in `/workspace/repo/app` and has no restriction.
pub fn run_spec() -> RunSpec {
    RunSpec {
        run_id: "run-1".into(),
        thread_id: "thread-1".into(),
        cwd: PathBuf::from("/workspace/repo/app"),
        checkout: PathBuf::from("/workspace/repo/app"),
        library: PathBuf::from("/workspace"),
        local_skills: None,
        prompt: "context\n\nhello".into(),
        user_prompt: "hello".into(),
        context: "context".into(),
        kind: RunKind::Turn,
        mode: orbyn_contracts::PermissionMode::Supervised,
        plan: false,
        model: Some("sonnet".into()),
        effort: Some("high".into()),
        options: std::collections::BTreeMap::new(),
        resume: None,
        mcp_url: Some("http://127.0.0.1:1/mcp/x".into()),
        run_dir: PathBuf::from("/run"),
        confine: None,
        crew_task: None,
    }
}

/// A run restricted to a fresh work folder, with the folder kept alive by the first value.
pub fn confined_spec() -> (TempDir, RunSpec) {
    let dir = TempDir::new();
    let data = dir.path().join("data");
    let run = dir.path().join("run");
    let work = dir.path().join("work");
    for folder in [&data, &run, &work] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let mut spec = run_spec();
    spec.cwd = work.clone();
    spec.checkout = work.clone();
    spec.thread_id = "worker-thread".into();
    spec.confine = Some(Confinement::new(&data, &run, &[], &[&work]).unwrap());
    (dir, spec)
}
