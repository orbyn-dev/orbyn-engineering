use super::fake::Fake;
use super::runs::{CodeRuns, PendingList, Refused};
use async_trait::async_trait;
use futures_util::future::join_all;
use orbyn_code_sandbox::{
    CodeExecutor, Limits, Run, RunError, RunSeed, Step, ToolFailure, ToolHost,
};
use serde_json::{Value, json};
use std::future::pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

struct Echo;

#[async_trait]
impl ToolHost for Echo {
    async fn call(&self, name: &str, _args: Value) -> Result<Value, ToolFailure> {
        Ok(json!(name))
    }
}

/// The fake, with a yield in `prepare` so concurrent starts really interleave.
struct YieldFake;

#[async_trait]
impl CodeExecutor for YieldFake {
    async fn prepare(
        &self,
        source: &str,
        limits: Limits,
        seed: RunSeed,
        host: Arc<dyn ToolHost>,
    ) -> Result<Box<dyn Run>, RunError> {
        tokio::task::yield_now().await;
        Fake.prepare(source, limits, seed, host).await
    }
}

fn runs_with(executor: Arc<dyn CodeExecutor>) -> CodeRuns {
    let runs = CodeRuns::new();
    runs.set_executor(executor, Duration::from_millis(50));
    runs
}

fn runs() -> CodeRuns {
    runs_with(Arc::new(Fake))
}

fn seed() -> orbyn_code_sandbox::RunSeed {
    orbyn_code_sandbox::RunSeed {
        start_ms: 0,
        seed: 1,
    }
}

async fn start(
    runs: &CodeRuns,
    id: &str,
    thread: &str,
    crew_worker: bool,
    code: &str,
) -> Result<Arc<super::runs::Live>, Refused> {
    runs.start(
        id,
        thread,
        crew_worker,
        code,
        seed(),
        |_: PendingList, _| Arc::new(Echo),
        |step, _| step,
    )
    .await
}

#[tokio::test]
async fn a_run_finishes_on_its_own_task_and_answers_again_when_asked_again() {
    let runs = runs();
    let live = start(&runs, "r1", "t1", false, "call a\ncall b")
        .await
        .ok()
        .unwrap();
    let Some(Step::Done { result, .. }) = live.wait(Duration::from_secs(5)).await else {
        panic!("not done")
    };
    assert_eq!(result, json!(["a", "b"]));
    let again = runs.get("r1", "t1").unwrap();
    assert!(matches!(
        again.wait(Duration::from_secs(5)).await,
        Some(Step::Done { .. })
    ));
}

#[tokio::test]
async fn a_slow_run_outlives_the_slice_and_a_later_wait_gets_the_end() {
    let runs = runs();
    let live = start(&runs, "r1", "t1", false, "sleep 300\ncall a")
        .await
        .ok()
        .unwrap();
    assert!(
        live.wait(Duration::from_millis(50)).await.is_none(),
        "still running after a slice"
    );
    drop(live);
    let live = runs
        .get("r1", "t1")
        .expect("the run is owned by the store, not the call");
    assert!(matches!(
        live.wait(Duration::from_secs(5)).await,
        Some(Step::Done { .. })
    ));
}

#[tokio::test]
async fn another_thread_cannot_see_a_run() {
    let runs = runs();
    start(&runs, "r1", "t1", false, "sleep 100")
        .await
        .ok()
        .unwrap();
    assert!(runs.get("r1", "t2").is_none());
    assert!(runs.get("r1", "t1").is_some());
}

#[tokio::test]
async fn a_thread_may_have_four_live_runs_and_the_fifth_is_refused() {
    let runs = runs();
    for i in 0..4 {
        start(&runs, &format!("r{i}"), "t1", false, "sleep 5000")
            .await
            .ok()
            .unwrap();
    }
    let Err(Refused::Limit(message)) = start(&runs, "r4", "t1", false, "call a").await else {
        panic!("not refused")
    };
    assert!(message.contains("4 scripts"), "{message}");
    assert!(
        start(&runs, "other", "t2", false, "call a").await.is_ok(),
        "another thread is not limited"
    );
}

#[tokio::test]
async fn a_syntax_error_is_refused_before_any_run_exists() {
    let runs = runs();
    assert!(matches!(
        start(&runs, "r1", "t1", false, "SYNTAX").await,
        Err(Refused::Script(_))
    ));
    assert!(runs.get("r1", "t1").is_none());
}

/// The cap check and the insertion are separated by `prepare`, so a burst must reserve its slot
/// first or every start would pass the check together.
#[tokio::test]
async fn many_concurrent_starts_respect_the_thread_and_core_caps() {
    let runs = Arc::new(runs_with(Arc::new(YieldFake)));
    let results = join_all((0..20).map(|i| {
        let runs = runs.clone();
        async move { start(&runs, &format!("r{i}"), "t1", false, "sleep 60000").await }
    }))
    .await;
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        4,
        "one thread has four live runs"
    );

    let runs = Arc::new(runs_with(Arc::new(YieldFake)));
    let results = join_all((0..20).map(|i| {
        let runs = runs.clone();
        async move {
            start(
                &runs,
                &format!("r{i}"),
                &format!("t{i}"),
                false,
                "sleep 60000",
            )
            .await
        }
    }))
    .await;
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        16,
        "the core has sixteen live runs"
    );
}

#[tokio::test]
async fn crew_workers_together_are_capped_and_leave_room_for_the_lead() {
    let runs = runs();
    for i in 0..8 {
        start(
            &runs,
            &format!("w{i}"),
            &format!("worker{i}"),
            true,
            "sleep 60000",
        )
        .await
        .ok()
        .unwrap();
    }
    let Err(Refused::Limit(message)) = start(&runs, "w8", "worker8", true, "sleep 60000").await
    else {
        panic!("not refused")
    };
    assert!(
        message.contains("Too many") && message.contains("Crew worker"),
        "{message}"
    );
    assert!(
        start(&runs, "lead", "lead", false, "sleep 60000")
            .await
            .is_ok(),
        "the lead still has slots"
    );
}

#[tokio::test]
async fn a_duplicate_run_id_is_refused() {
    let runs = runs();
    start(&runs, "same", "t1", false, "sleep 60000")
        .await
        .ok()
        .unwrap();
    let Err(Refused::Duplicate(message)) = start(&runs, "same", "t2", false, "sleep 60000").await
    else {
        panic!("not refused")
    };
    assert!(message.contains("same"), "{message}");
}

#[tokio::test]
async fn a_failed_prepare_releases_its_start_slot() {
    let runs = runs_with(Arc::new(YieldFake));
    for i in 0..4 {
        assert!(matches!(
            start(&runs, &format!("bad{i}"), "t1", false, "SYNTAX").await,
            Err(Refused::Script(_))
        ));
    }
    assert!(start(&runs, "ok", "t1", false, "sleep 60000").await.is_ok());
}

/// Redaction covers every string value, leaves schema keys alone, and matches whole names, so a
/// shorter hidden name does not corrupt a longer allowed one.
#[test]
fn redaction_covers_values_but_not_keys_and_matches_whole_names() {
    let mut spec = json!({
        "name": "orbyn_worktree_status",
        "description": "Calls orbyn_thread_send and crew_start.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "description": "Not orbyn_code_run.",
                    "enum": ["orbyn_thread_send", "orbyn_code_search"],
                },
            },
            "required": ["mode"],
        },
    });
    let hidden = [
        "orbyn_code".to_owned(),
        "orbyn_thread_send".to_owned(),
        "orbyn_code_run".to_owned(),
        "crew_start".to_owned(),
    ];
    super::redact_tool_names(&mut spec, &hidden);
    let text = spec.to_string();
    assert!(!text.contains("orbyn_thread_send"), "{text}");
    assert!(!text.contains("orbyn_code_run"), "{text}");
    assert!(!text.contains("crew_start"), "{text}");
    assert!(
        text.contains("orbyn_code_search"),
        "the longer allowed name survives: {text}"
    );
    assert!(
        text.contains("\"mode\""),
        "schema keys are untouched: {text}"
    );
    assert!(text.contains("another tool"), "{text}");
}

/// A host whose only call never returns; the guard records that the cancel dropped its future.
struct DroppingHost {
    dropped: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
}

struct DropGuard(Arc<AtomicBool>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl ToolHost for DroppingHost {
    async fn call(&self, _name: &str, _args: Value) -> Result<Value, ToolFailure> {
        self.started.store(true, Ordering::SeqCst);
        let _guard = DropGuard(self.dropped.clone());
        pending::<Result<Value, ToolFailure>>().await
    }
}

#[tokio::test]
async fn a_cancel_aborts_an_in_flight_call_and_drops_its_future() {
    let runs = runs();
    let dropped = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicBool::new(false));
    let host = Arc::new(DroppingHost {
        dropped: dropped.clone(),
        started: started.clone(),
    });
    let _live = runs
        .start(
            "r2",
            "t1",
            false,
            "call orbyn_environment_read",
            seed(),
            move |_, _| host,
            |step, _| step,
        )
        .await
        .ok()
        .unwrap();
    for _ in 0..100 {
        if started.load(Ordering::SeqCst) {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(started.load(Ordering::SeqCst), "the call is in flight");
    assert!(runs.begin_cancel("r2").is_some());
    runs.finish_cancel("r2").await;
    assert!(
        dropped.load(Ordering::SeqCst),
        "the call future was dropped by the cancel"
    );
}

#[test]
fn a_cancelled_finish_records_cancelled_over_a_completed_step() {
    let settled = super::settle(
        Step::Done {
            result: json!(1),
            logs: vec!["x".into()],
            truncated: false,
        },
        true,
        None,
    );
    let Step::Failed {
        error,
        logs,
        truncated,
    } = settled
    else {
        panic!("a cancel ends as failed");
    };
    assert_eq!(error.code.as_deref(), Some("cancelled"));
    assert_eq!(error.message, "The run was cancelled.");
    assert_eq!(logs, vec!["x".to_owned()]);
    assert!(!truncated);
}
