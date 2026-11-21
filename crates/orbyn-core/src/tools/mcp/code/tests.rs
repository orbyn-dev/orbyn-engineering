use super::fake::Fake;
use super::runs::{CodeRuns, PendingList, Refused};
use async_trait::async_trait;
use orbyn_code_sandbox::{Step, ToolFailure, ToolHost};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

struct Echo;

#[async_trait]
impl ToolHost for Echo {
    async fn call(&self, name: &str, _args: Value) -> Result<Value, ToolFailure> {
        Ok(json!(name))
    }
}

fn runs() -> CodeRuns {
    let runs = CodeRuns::new();
    runs.set_executor(Arc::new(Fake), Duration::from_millis(50));
    runs
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
    code: &str,
) -> Result<Arc<super::runs::Live>, Refused> {
    runs.start(
        id,
        thread,
        code,
        seed(),
        |_: PendingList| Arc::new(Echo),
        |step| step,
    )
    .await
}

#[tokio::test]
async fn a_run_finishes_on_its_own_task_and_answers_again_when_asked_again() {
    let runs = runs();
    let live = start(&runs, "r1", "t1", "call a\ncall b")
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
    let live = start(&runs, "r1", "t1", "sleep 300\ncall a")
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
    start(&runs, "r1", "t1", "sleep 100").await.ok().unwrap();
    assert!(runs.get("r1", "t2").is_none());
    assert!(runs.get("r1", "t1").is_some());
}

#[tokio::test]
async fn a_thread_may_have_four_live_runs_and_the_fifth_is_refused() {
    let runs = runs();
    for i in 0..4 {
        start(&runs, &format!("r{i}"), "t1", "sleep 5000")
            .await
            .ok()
            .unwrap();
    }
    let Err(Refused::Limit(message)) = start(&runs, "r4", "t1", "call a").await else {
        panic!("not refused")
    };
    assert!(message.contains("4 scripts"), "{message}");
    assert!(
        start(&runs, "other", "t2", "call a").await.is_ok(),
        "another thread is not limited"
    );
}

#[tokio::test]
async fn a_syntax_error_is_refused_before_any_run_exists() {
    let runs = runs();
    assert!(matches!(
        start(&runs, "r1", "t1", "SYNTAX").await,
        Err(Refused::Script(_))
    ));
    assert!(runs.get("r1", "t1").is_none());
}
