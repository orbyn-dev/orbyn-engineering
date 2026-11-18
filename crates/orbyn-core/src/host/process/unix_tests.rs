//! A descendant that left the process group with `setsid` is still reaped by the owner's drop, also after `terminate` and after the
//! leader was waited for. A leader that is already reaped is never walked (its pid may belong to someone else).

use super::*;
use crate::testing::{TempDir, assert_process_reaped, process_alive, setsid_sleeper, sleeper_pid};

fn spawn(dir: &std::path::Path, script: String) -> ManagedChild {
    let command = ResolvedCommand {
        program: "/bin/sh".into(),
        prefix: Vec::new(),
        path: "/bin/sh".into(),
    };
    ManagedChild::spawn(engine_command(&command, &["-c".into(), script.into()], dir)).unwrap()
}

#[tokio::test]
async fn a_setsid_descendant_dies_when_the_owner_is_dropped() {
    let dir = TempDir::new();
    let pid_file = dir.path().join("pid");
    let child = spawn(
        dir.path(),
        format!("{} wait", setsid_sleeper(&pid_file, false)),
    );
    let escaped = sleeper_pid(&pid_file).await;
    assert!(process_alive(escaped));
    drop(child);
    assert_process_reaped(escaped, "its owner").await;
}

/// The callers of `terminate` wait for the leader and then drop. The `setsid` child ignores SIGTERM and is reparented to `init` when
/// the leader exits, before the drop: it was recorded by `terminate`, so the drop still finishes it.
#[tokio::test]
async fn a_setsid_child_that_ignores_sigterm_dies_after_terminate_wait_and_drop() {
    let dir = TempDir::new();
    let pid_file = dir.path().join("pid");
    let mut child = spawn(
        dir.path(),
        format!("{} wait", setsid_sleeper(&pid_file, true)),
    );
    let escaped = sleeper_pid(&pid_file).await;
    child.terminate();
    child.wait().await.unwrap();
    assert!(
        process_alive(escaped),
        "it ignored SIGTERM and outlived its leader"
    );
    drop(child);
    assert_process_reaped(escaped, "terminate, wait and drop").await;
}

/// After the leader was waited for, its pid is free to be reused: the owner knows it is reaped (`id()` is `None`), so the drop signals
/// the group and what was recorded (checked by start time) and walks nothing.
#[tokio::test]
async fn a_reaped_leader_is_not_walked() {
    let dir = TempDir::new();
    let mut child = spawn(dir.path(), "exit 0".into());
    let leader = child.child.id().unwrap() as i32;
    child.wait().await.unwrap();
    assert!(child.child.id().is_none(), "reaped: nothing may be walked");
    assert!(child.tree.seen.lock().unwrap().is_empty());
    assert!(leader > 1);
    drop(child);
}
