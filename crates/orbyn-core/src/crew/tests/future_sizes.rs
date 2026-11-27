//! Stack budgets for the Crew check and forge-watch futures.
//!
//! Tokio worker threads and test threads have 2 MiB stacks. One verification is a chain of
//! futures: `crew_watch_forge_main` -> `prepare_and_verify_forge_correction` ->
//! `verify_forge_correction` -> `crew_verify` -> `run_check` -> `run_line`/`run_command`. A future
//! that is awaited without `Box::pin` sits on its caller's frame, so the chain's frames add up;
//! #1419 overflowed a test thread once one future in that chain grew (#1353). This test creates
//! each future with cheap arguments, never polls it, and asserts its measured size is below a
//! budget chosen from that measurement (rounded up to the next 4 KiB, never above 64 KiB). The
//! budget fails loudly when a future grows toward the 2 MiB stack, and a future over 16 KiB is
//! boxed at its await site instead of raising the budget.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::{CrewCheck, PermissionMode};
use serde_json::json;

use crate::CoreConfig;
use crate::app::Core;
use crate::crew::MergeTarget;
use crate::crew::checks::{Site, run_check, run_line};
use crate::crew::lifecycle::Guard;
use crate::governance::approvals::RunScope;
use crate::store::{CrewRun, CrewTask};
use crate::testing::TempDir;
use crate::tools::mcp::McpBinding;

/// Each budget is its measured size plus at least 25% headroom (other targets and compilers lay futures out a little
/// differently), rounded up to a 4 KiB multiple; no budget may exceed 64 KiB.
/// Measured on the Linux box: run_check 18,248, run_line 18,184, crew_verify 2,512,
/// crew_watch_forge_main 5,696, prepare_and_verify_forge_correction 5,064 and crew_start_tool 12,160.
const RUN_CHECK_BUDGET: usize = 24_576;
const RUN_LINE_BUDGET: usize = 24_576;
const CREW_VERIFY_BUDGET: usize = 4_096;
const WATCH_FORGE_BUDGET: usize = 8_192;
const PREPARE_CORRECTION_BUDGET: usize = 8_192;
const CREW_START_BUDGET: usize = 16_384;

fn assert_budget(name: &str, size: usize, budget: usize) {
    eprintln!("future_sizes: {name} = {size} bytes (budget {budget})");
    assert!(
        size < budget,
        "the {name} future is {size} bytes, not below its {budget}-byte stack budget"
    );
    assert_eq!(
        budget % 4_096,
        0,
        "the {name} budget is not a 4 KiB multiple"
    );
    assert!(budget <= 64 * 1024, "the {name} budget is above 64 KiB");
}

#[test]
fn the_check_and_forge_watch_futures_fit_their_stack_budgets() {
    let dir = TempDir::new();
    let core = Arc::new(
        Core::open(CoreConfig::new(
            dir.path().join("data"),
            "http://127.0.0.1:1".into(),
        ))
        .unwrap(),
    );

    // No check is run: the future is created, measured and dropped.
    let check: CrewCheck = serde_json::from_value(json!({
        "id": "chk-budget",
        "kind": "command",
        "params": { "run": "true" },
        "proves": "The command exits 0."
    }))
    .unwrap();
    let site = Site {
        worktree: Path::new("."),
        deadline: Instant::now() + Duration::from_secs(60),
        base: None,
    };
    let future = run_check(&check, &site);
    assert_budget(
        "checks::run_check",
        std::mem::size_of_val(&future),
        RUN_CHECK_BUDGET,
    );
    drop(future);
    let future = run_line("true", site.worktree, site.deadline);
    assert_budget(
        "checks::run_line",
        std::mem::size_of_val(&future),
        RUN_LINE_BUDGET,
    );
    drop(future);

    let task = CrewTask {
        id: "task-000000000000000000000000".into(),
        crew_id: "crew-000000000000000000000000".into(),
        run_id: "run-01".into(),
        state: "verifying".into(),
        stop_reason: None,
        stop_detail: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };
    let run = CrewRun {
        task_id: task.id.clone(),
        lead_thread_id: "thread-01".into(),
        member: "worker-01".into(),
        worker_thread_id: None,
        worktree: Some(".".into()),
        base_commit: None,
        attempts: 0,
        failures: 0,
        guard: Guard::default(),
        last_report_run: None,
        protected: None,
        verifying_since: None,
        index_flags: None,
        resumed_at: None,
        resume_count: 0,
        downtime_secs: 0,
    };
    let future = core.crew_verify(&task, &run);
    assert_budget(
        "Core::crew_verify",
        std::mem::size_of_val(&future),
        CREW_VERIFY_BUDGET,
    );
    drop(future);

    let target = MergeTarget {
        repo: dir.path().join("repo"),
        branch: "main".into(),
        owed: None,
    };
    let future = core.crew_watch_forge_main(&target, None, 0);
    assert_budget(
        "Core::crew_watch_forge_main",
        std::mem::size_of_val(&future),
        WATCH_FORGE_BUDGET,
    );
    drop(future);
    let future = core.prepare_and_verify_forge_correction(&task.id, &target, "watch", 0);
    assert_budget(
        "Core::prepare_and_verify_forge_correction",
        std::mem::size_of_val(&future),
        PREPARE_CORRECTION_BUDGET,
    );
    drop(future);

    let binding = McpBinding {
        scope: RunScope {
            run_id: "run-lead".into(),
            thread_id: "thread-lead".into(),
            engine: "alpha".into(),
            family: "alpha".into(),
            write_scope: None,
            mode: PermissionMode::Supervised,
            plan: false,
        },
        permission_prompt: false,
        skills: Vec::new(),
    };
    let args = json!({ "brief": {}, "answer_key": {} });
    let future = core.crew_start_tool(&binding, &args);
    assert_budget(
        "Core::crew_start_tool",
        std::mem::size_of_val(&future),
        CREW_START_BUDGET,
    );
    drop(future);
}
