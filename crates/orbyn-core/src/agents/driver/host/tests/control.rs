//! The person's stop and restart of a family's host, and the rule that a host with background work
//! open is not stopped unless the person's stop says so.

use super::*;
use crate::agents::driver::host::connection::ClosedReason;

#[tokio::test]
async fn a_stop_ends_the_host_and_its_open_run_and_the_next_run_starts_it_again() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let started = start(&driver, spec("[hang]")).await.unwrap();
    let connection = rig.pool.acquire().await.unwrap();
    rig.pool.stop(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Stopped);
    assert_eq!(connection.closed().await, ClosedReason::Stopped);
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    let (_, outcome) = start(&driver, spec("again")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    // The stop was not a crash.
    let status = rig.pool.status().await;
    assert_eq!((status.restart_count, status.last_crash), (0, None));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_host_with_background_work_open_is_not_stopped_unless_the_stop_says_so() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (_, outcome) = start(&rig.driver(), spec("[bg]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert_eq!(
        rig.pool.stop(false).await,
        Err(Refusal::BackgroundOpen(1)),
        "the task is still running"
    );
    assert_eq!(rig.pool.state(), HostState::Ready, "nothing was stopped");
    assert_eq!(
        rig.pool.restart_now(false).await,
        Err(Refusal::BackgroundOpen(1))
    );
    assert_eq!(rig.pool.state(), HostState::Ready);
    rig.pool.stop(true).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Stopped);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_stop_with_no_host_running_does_nothing() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    rig.pool.stop(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Idle);
}

#[tokio::test]
async fn a_restart_gives_a_family_that_gave_up_a_clean_slate() {
    let dir = crate::testing::TempDir::new();
    let ready = dir.path().join("ready");
    let ready_text = ready.to_string_lossy().into_owned();
    let Some(rig) = standin(&[("STANDIN_NEEDS_FILE", &ready_text)], fast()) else {
        return;
    };
    assert!(rig.pool.acquire().await.is_err());
    wait_for(
        &rig.pool,
        |s| matches!(s, HostState::Unavailable { .. }),
        "unavailable",
    )
    .await;
    assert_eq!(rig.pool.status().await.restart_count, 5);
    // The fault is fixed; the person asks again.
    std::fs::write(&ready, "").unwrap();
    rig.pool.restart_now(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Ready);
    // The tally since the app started stays; the backoff and the give-up count are fresh.
    let status = rig.pool.status().await;
    assert_eq!(status.restart_count, 5);
    let (_, outcome) = start(&rig.driver(), spec("[crash]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    match rig.pool.state() {
        HostState::Restarting { attempt, .. } => {
            assert_eq!(attempt, 1, "the old crashes still count")
        }
        other => assert_eq!(other, HostState::Ready, "already back up"),
    }
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_restart_replaces_a_running_host() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let first = rig.pool.acquire().await.unwrap();
    rig.pool.restart_now(false).await.unwrap();
    assert_eq!(first.closed().await, ClosedReason::Stopped);
    let second = rig.pool.acquire().await.unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(rig.pool.state(), HostState::Ready);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_version_mismatch_stays_until_an_update() {
    let Some(rig) = standin(&[("STANDIN_PROTOCOL", "2")], fast()) else {
        return;
    };
    assert!(rig.pool.acquire().await.is_err());
    assert_eq!(rig.pool.stop(true).await, Err(Refusal::VersionMismatch));
    assert_eq!(
        rig.pool.restart_now(true).await,
        Err(Refusal::VersionMismatch)
    );
}

#[tokio::test]
async fn the_hub_names_a_family_that_does_not_use_the_host_or_does_not_exist() {
    let config = HostConfig::from_env(Some("/x/orbyn-engine-host".into()), Some("0")).unwrap();
    let hub = HostHub::new(&config);
    assert_eq!(
        hub.stop("claude", false).await.unwrap_err(),
        ControlError::Refused(Refusal::NotUsed)
    );
    assert_eq!(
        hub.restart("claude", false).await.unwrap_err(),
        ControlError::Refused(Refusal::NotUsed)
    );
    assert_eq!(
        hub.stop("nope", false).await.unwrap_err(),
        ControlError::UnknownFamily
    );
}

#[tokio::test]
async fn the_status_names_the_threads_working_and_the_kind_of_the_last_failure() {
    use crate::agents::driver::host::ErrorKind;
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let running = start(&driver, spec("[hang]")).await.unwrap();
    assert_eq!(rig.pool.status().await.threads, vec!["thread-1".to_owned()]);
    running.handle.cancel.cancel();
    running.outcome().await;
    assert!(rig.pool.status().await.threads.is_empty());

    start(&driver, spec("[lost]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::SessionLost)
    );
    start(&driver, spec("[limit]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::RateLimit)
    );
    // A run that completes clears it: the screen never shows a stale failure.
    start(&driver, spec("fine")).await.unwrap().outcome().await;
    assert_eq!(rig.pool.status().await.last_error, None);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_run_the_person_cancelled_leaves_the_last_error_alone_whatever_the_engine_called_its_end()
{
    use crate::agents::driver::host::ErrorKind;
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    start(&driver, spec("[limit]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::RateLimit)
    );
    // The SDK ends an interrupted turn as a failure; the person asked for it, so it is not shown as one.
    let started = start(&driver, spec("[hang] [failcancel]")).await.unwrap();
    started.handle.cancel.cancel();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::RateLimit)
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_stop_while_the_host_waits_to_restart_ends_the_restarts() {
    let slow = Timings {
        backoff_base: Duration::from_millis(500),
        backoff_cap: Duration::from_millis(500),
        ..fast()
    };
    let Some(rig) = standin(&[], slow) else {
        return;
    };
    let (_, outcome) = start(&rig.driver(), spec("[crash]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    assert!(matches!(rig.pool.state(), HostState::Restarting { .. }));
    rig.pool.stop(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Stopped);
    // The wait is over and nothing started the host.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(rig.pool.state(), HostState::Stopped);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn the_claude_host_keeps_native_rewind_unsupported_so_a_rewind_leaves_a_handoff() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let refusal = driver
        .revert_session(std::path::Path::new("."), "engine-session", "turn-1")
        .await
        .unwrap_err();
    assert!(matches!(refusal, crate::error::CoreError::Unsupported(_)));
    // Nothing was asked of the host: its own `rollback` only works inside one live host session.
    assert!(rig.log().iter().all(|frame| frame["type"] != "rollback"));
    rig.pool.shutdown().await;
}
