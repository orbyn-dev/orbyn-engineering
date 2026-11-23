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

/// A double fork whose middle process has exited orphans the `setsid` grandchild to `init` before any walk can see its parent link, so
/// the group kill alone left it running after a timeout or a withdraw. Linux makes the leader a subreaper, which adopts the orphan, so
/// the walk finds it again; macOS has no such flag and keeps that documented limit, which is why the case is Linux-only. The
/// `terminate` case ignores SIGTERM, so only a walk that recorded the orphan before the leader exited can still finish it.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_double_forked_grandchild_dies_on_drop_and_after_terminate() {
    use crate::testing::double_fork_sleeper;

    for how in ["its owner", "terminate, wait and drop"] {
        let dir = TempDir::new();
        let pid_file = dir.path().join("pid");
        let mut child = spawn(
            dir.path(),
            format!(
                "{} sleep 600",
                double_fork_sleeper(&pid_file, how != "its owner")
            ),
        );
        let escaped = sleeper_pid(&pid_file).await;
        assert!(process_alive(escaped));
        if how == "its owner" {
            drop(child);
        } else {
            child.terminate();
            child.wait().await.unwrap();
            drop(child);
        }
        assert_process_reaped(escaped, how).await;
    }
}

/// A smoke test, not a proof of pid-reuse safety: after the leader was waited for, `id()` is `None`, nothing is walked, and the drop
/// signals nothing but the group and what was recorded. A process of the test's own (its parent is this test, not the leader) lives on.
#[tokio::test]
async fn a_reaped_leader_signals_nothing_outside_the_group_and_the_record() {
    let dir = TempDir::new();
    let mut bystander = std::process::Command::new("/bin/sleep")
        .arg("600")
        .spawn()
        .unwrap();
    let mut child = spawn(dir.path(), "exit 0".into());
    child.wait().await.unwrap();
    assert!(child.child.id().is_none(), "reaped: nothing may be walked");
    assert!(child.tree.seen.lock().unwrap().is_empty());
    drop(child);
    assert!(
        bystander.try_wait().unwrap().is_none(),
        "the bystander was left alone"
    );
    bystander.kill().unwrap();
    bystander.wait().unwrap();
}
