//! Each crew worker's turn counter is its own (#1750). The fake has always kept one counter per world, so a
//! worker's script was handed the world's start order: the number counted every worker started before it, and
//! every turn every worker took. These tests record that interference, then show a world where the number a
//! worker's script is handed is that worker's own turn, so one worker cannot shift another's replies.

use std::sync::Arc;

use serde_json::json;

use super::harness::{Script, Turn, World, brief, key, report, world, world_own_turns};

/// A turn that leaves a file outside the brief's lease, so the task fails its check and runs its worker again.
fn outside_the_lease(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/bad.txt"), "outside\n").unwrap();
}

/// Every turn leaves the same file outside the lease, so the loop guard stops the task after three turns.
fn failing() -> Script {
    Arc::new(|_, root| {
        outside_the_lease(root);
        Turn::Report(report("done"))
    })
}

/// Starts a task, waits for it to stop, and answers its worker's thread.
async fn stopped(w: &World) -> String {
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "stopped").await;
    w.worker(&id)
}

/// The shared schedule numbers a worker by the workers started before it, which is the interference: the
/// second worker's script never sees a first turn, so the same script answers a different arm for it.
#[tokio::test]
async fn the_shared_schedule_shifts_a_later_worker_by_the_turns_before_it() {
    let w = world(true, failing()).await;
    let first = stopped(&w).await;
    let second = stopped(&w).await;

    // The worker counted its own turns both times: three runs each.
    assert_eq!(w.own_turn_count(&first), 3);
    assert_eq!(w.own_turn_count(&second), 3);
    // What each worker's script was handed counted the first worker's turns too.
    assert_eq!(w.turns_of(&first), [0, 1, 2]);
    assert_eq!(
        w.turns_of(&second),
        [3, 4, 5],
        "the second worker's script never saw a turn 0"
    );
}

/// With each worker's own counter, the same two tasks hand both workers 0, 1, 2: neither worker's script can
/// be shifted by the other's turns.
#[tokio::test]
async fn each_worker_counts_its_own_turns() {
    let w = world_own_turns(true, failing()).await;
    let first = stopped(&w).await;
    let second = stopped(&w).await;

    assert_eq!(w.turns_of(&first), [0, 1, 2]);
    assert_eq!(w.turns_of(&second), [0, 1, 2]);
    assert_eq!(w.own_turn_count(&first), 3);
    assert_eq!(w.own_turn_count(&second), 3);
}
