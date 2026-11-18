//! A descendant that left the process group with `setsid` is still reaped when its owner is dropped.

use super::*;
use crate::testing::{TempDir, assert_process_reaped, process_alive, setsid_sleeper, sleeper_pid};

#[tokio::test]
async fn a_setsid_descendant_dies_when_the_owner_is_dropped() {
    let dir = TempDir::new();
    let pid_file = dir.path().join("pid");
    let command = ResolvedCommand {
        program: "/bin/sh".into(),
        prefix: Vec::new(),
        path: "/bin/sh".into(),
    };
    let script = setsid_sleeper(&pid_file);
    let child = ManagedChild::spawn(engine_command(
        &command,
        &["-c".into(), script.into()],
        dir.path(),
    ))
    .unwrap();
    let escaped = sleeper_pid(&pid_file).await;
    assert!(process_alive(escaped));
    drop(child);
    assert_process_reaped(escaped, "its owner").await;
}
