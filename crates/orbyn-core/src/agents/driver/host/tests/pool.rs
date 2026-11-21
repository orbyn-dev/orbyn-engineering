use std::time::Duration;

use serde_json::json;

use super::*;
use crate::agents::driver::host::connection::{ClosedReason, RunFrame};
use crate::agents::driver::host::frames::PROTOCOL;

#[tokio::test]
async fn a_host_that_says_hello_and_ready_is_running() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    assert_eq!(rig.pool.state(), HostState::Idle);
    let connection = rig.pool.acquire().await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Ready);
    let status = rig.pool.status().await;
    assert_eq!(status.host_version.as_deref(), Some("9.9.9"));
    assert!(connection.has_capability("steer"));
    assert!(!connection.has_capability("fork"));
    assert_eq!(status.restart_count, 0);
}

#[tokio::test]
async fn the_real_fake_family_completes_the_handshake() {
    let Some(pool) = real_fake(fast()) else {
        return;
    };
    let connection = pool.acquire().await.unwrap();
    assert!(connection.has_capability("steer"));
    assert_eq!(pool.state(), HostState::Ready);
    pool.shutdown().await;
}

#[tokio::test]
async fn a_protocol_mismatch_is_final_and_names_both_numbers() {
    let Some(rig) = standin(&[("STANDIN_PROTOCOL", "2")], fast()) else {
        return;
    };
    let error = rig.pool.acquire().await.unwrap_err();
    assert!(
        error.0.contains("app protocol 1") && error.0.contains("host protocol 2"),
        "{}",
        error.0
    );
    assert!(error.0.contains("Update Orbyn"));
    assert_eq!(
        rig.pool.state(),
        HostState::Mismatch {
            app: PROTOCOL,
            host: 2
        }
    );
    // nothing restarts it
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.pool.status().await.restart_count, 0);
    assert!(rig.pool.acquire().await.is_err());
}

#[tokio::test]
async fn a_hello_timeout_counts_as_a_crash() {
    let timings = Timings {
        hello: Duration::from_millis(300),
        first_hello: Duration::from_millis(300),
        ..fast()
    };
    let Some(rig) = standin(&[("STANDIN_HELLO_DELAY_MS", "6000")], timings) else {
        return;
    };
    assert!(rig.pool.acquire().await.is_err());
    let status = rig.pool.status().await;
    let (crash, _) = status.last_crash.expect("a crash was recorded");
    assert_eq!(crash.kind, CrashKind::HelloTimeout);
    assert_eq!(status.restart_count, 1);
    assert!(matches!(rig.pool.state(), HostState::Restarting { .. }));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn five_restarts_in_five_minutes_make_the_family_unavailable_until_asked() {
    let Some(rig) = standin(&[("STANDIN_EXIT_BEFORE_HELLO", "1")], fast()) else {
        return;
    };
    assert!(rig.pool.acquire().await.is_err());
    wait_for(
        &rig.pool,
        |s| matches!(s, HostState::Unavailable { .. }),
        "unavailable",
    )
    .await;
    let status = rig.pool.status().await;
    assert_eq!(status.restart_count, 5);
    let (crash, _) = status.last_crash.unwrap();
    assert_eq!(crash.exit.unwrap().code, Some(1));
    // the host's last words are kept, with the credential-looking line gone
    assert!(crash.stderr_tail.contains("exiting before hello"));
    assert!(!crash.stderr_tail.contains("sk-ant"));
    let refusal = rig.pool.acquire().await.unwrap_err();
    assert!(refusal.0.contains("unavailable"));
    // no restarts until the person asks
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.pool.status().await.restart_count, 5);
    rig.pool.retry().await;
    assert_eq!(rig.pool.state(), HostState::Crashed);
}

#[tokio::test]
async fn a_malformed_frame_is_counted_and_dropped_and_ten_in_a_minute_crash_the_host() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let (_, outcome) = start(&driver, spec("[malformed:3]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let connection = rig.pool.acquire().await.unwrap();
    assert_eq!(
        connection.malformed_count(),
        3,
        "{:?}",
        rig.pool.status().await
    );
    assert_eq!(rig.pool.state(), HostState::Ready);

    let (_, outcome) = start(&driver, spec("[malformed:10]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    let (crash, _) = rig.pool.status().await.last_crash.unwrap();
    assert_eq!(crash.kind, CrashKind::Malformed);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_late_respond_is_ignored_and_the_host_stays_up() {
    let Some(pool) = real_fake(fast()) else {
        return;
    };
    let connection = pool.acquire().await.unwrap();
    let answer = connection
        .request(
            "respond",
            json!({"requestId": "p-unknown", "decision": {"behavior": "deny"}}),
        )
        .await
        .unwrap();
    assert_eq!(answer, serde_json::Value::Null);
    assert!(!connection.is_closed());
    pool.shutdown().await;
}

#[tokio::test]
async fn a_failure_keeps_the_hosts_cause_chain() {
    let Some(pool) = real_fake(fast()) else {
        return;
    };
    let connection = pool.acquire().await.unwrap();
    connection
        .request(
            "open",
            json!({"operation": "create", "sessionId": "s", "cwd": "/"}),
        )
        .await
        .unwrap();
    let send_id = connection.next_id();
    let mut route = connection.register(send_id, "s", "thread-s").unwrap();
    connection
        .request_as(
            send_id,
            "send",
            json!({"sessionId": "s", "message": "[fail]"}),
        )
        .await
        .unwrap();
    let done = loop {
        match tokio::time::timeout(Duration::from_secs(10), route.frames.recv())
            .await
            .unwrap()
        {
            Some(RunFrame::Done(done)) => break done,
            Some(_) => {}
            None => panic!("route closed"),
        }
    };
    assert!(!done.ok);
    // the wrapper, then ten nested errors
    assert!(done.error.unwrap().depth() >= 10);
    pool.shutdown().await;
}

#[tokio::test]
async fn a_host_that_stops_answering_pings_is_a_crash() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (_, outcome) = start(&rig.driver(), spec("[stall]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(matches!(outcome, EngineOutcome::FailedWith(_)));
    let (crash, _) = rig.pool.status().await.last_crash.unwrap();
    assert_eq!(crash.kind, CrashKind::Stalled);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_frame_over_eight_mib_is_a_protocol_error_and_one_at_the_cap_is_not() {
    // Parsing 8 MiB in a debug build outlasts the fast ping interval; slow it so only the cap is under test.
    let timings = Timings {
        ping_every: Duration::from_secs(5),
        ..fast()
    };
    let Some(rig) = standin(&[], timings) else {
        return;
    };
    let driver = rig.driver();
    let (_, outcome) = start(&driver, spec("[exactly]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        outcome,
        EngineOutcome::Completed,
        "a frame at the cap is allowed"
    );
    let (_, outcome) = start(&driver, spec("[big]")).await.unwrap().outcome().await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    let (crash, _) = rig.pool.status().await.last_crash.unwrap();
    assert_eq!(crash.kind, CrashKind::Oversize);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn an_idle_host_stops_but_never_one_with_an_open_background_task() {
    let timings = Timings {
        idle_stop: Some(Duration::from_millis(200)),
        ..fast()
    };
    // with background work open
    let Some(busy) = standin(&[], timings.clone()) else {
        return;
    };
    let (_, outcome) = start(&busy.driver(), spec("[bg]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        busy.pool.state(),
        HostState::Ready,
        "stopped with a task open"
    );
    assert_eq!(busy.pool.acquire().await.unwrap().open_background(), 1);
    busy.pool.shutdown().await;

    // with nothing open
    let Some(idle) = standin(&[], timings) else {
        return;
    };
    let (_, outcome) = start(&idle.driver(), spec("hi"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    wait_for(&idle.pool, |s| *s == HostState::Stopped, "stopped").await;
    // and the next run starts it again
    let (_, outcome) = start(&idle.driver(), spec("again"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    idle.pool.shutdown().await;
}

#[tokio::test]
async fn shutting_down_closes_the_host() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let connection = rig.pool.acquire().await.unwrap();
    rig.pool.shutdown().await;
    assert_eq!(connection.closed().await, ClosedReason::Shutdown);
    assert_eq!(rig.pool.state(), HostState::Idle);
}

#[test]
fn the_flag_off_and_an_empty_host_path_keep_the_rust_drivers() {
    assert!(HostConfig::from_env(None, Some("1")).is_none());
    assert!(HostConfig::from_env(Some("".into()), Some("1")).is_none());
    let config = HostConfig::from_env(Some("/x/orbyn-engine-host".into()), None).unwrap();
    assert!(!config.claude);
    let hub = HostHub::new(&config);
    let inner: Arc<dyn crate::agents::driver::EngineDriver> =
        Arc::new(ClaudeDriver::new("claude".into(), Duration::from_secs(1)));
    let wrapped = hub.wrap("claude", inner.clone(), "claude".into());
    assert!(
        Arc::ptr_eq(&wrapped, &inner),
        "a flag-off family is not wrapped"
    );
    let flag = HostConfig::from_env(Some("/x/h".into()), Some("true")).unwrap();
    assert!(!flag.claude, "the host wants exactly 1");
}
