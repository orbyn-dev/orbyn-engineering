use super::*;
use crate::agents::driver::host::{CrashInfo, CrashKind, ExitRecord};

fn status(state: HostState) -> HostStatus {
    HostStatus {
        family: "claude".into(),
        state,
        host_version: Some("0.1.0".into()),
        restart_count: 2,
        last_crash: None,
        threads: Vec::new(),
        last_error: None,
    }
}

#[test]
fn every_state_maps_to_the_names_the_ui_reads() {
    let cases = [
        (HostState::Idle, EngineHostState::NotStarted),
        (HostState::Stopped, EngineHostState::Stopped),
        (HostState::Crashed, EngineHostState::Crashed),
        (HostState::Starting, EngineHostState::Starting),
        (HostState::Ready, EngineHostState::Running),
        (
            HostState::Restarting {
                attempt: 1,
                last_error: "x".into(),
            },
            EngineHostState::Restarting,
        ),
        (
            HostState::Unavailable { reason: "x".into() },
            EngineHostState::CantStart,
        ),
        (
            HostState::Mismatch { app: 1, host: 2 },
            EngineHostState::VersionMismatch,
        ),
        (
            HostState::NotUsed(NotUsed::FlagOff),
            EngineHostState::NotUsed,
        ),
    ];
    for (state, expect) in cases {
        assert_eq!(contract(&status(state)).state, expect);
    }
}

#[test]
fn the_wire_names_are_snake_case() {
    let names: Vec<String> = [
        EngineHostState::NotStarted,
        EngineHostState::CantStart,
        EngineHostState::VersionMismatch,
        EngineHostState::NotUsed,
    ]
    .iter()
    .map(|state| serde_json::to_string(state).unwrap())
    .collect();
    assert_eq!(
        names,
        [
            "\"not_started\"",
            "\"cant_start\"",
            "\"version_mismatch\"",
            "\"not_used\""
        ]
    );
}

#[test]
fn a_mismatch_carries_both_numbers_and_the_action() {
    let wire = contract(&status(HostState::Mismatch { app: 1, host: 2 }));
    assert_eq!((wire.protocol_app, wire.protocol_host), (Some(1), Some(2)));
    assert_eq!(wire.action.as_deref(), Some("Update Orbyn"));
    let off = contract(&status(HostState::NotUsed(NotUsed::UnsupportedPlatform)));
    assert_eq!(off.not_used, Some(EngineHostNotUsed::UnsupportedPlatform));
}

#[test]
fn the_last_crash_has_exit_kind_time_and_a_redacted_capped_tail() {
    let stderr = format!(
        "boot ok\nANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnop\n{}",
        "x ".repeat(2000)
    );
    let mut crash = CrashInfo::new(CrashKind::Exited, "The engine host exited (exit code 3)")
        .with_stderr(&stderr);
    crash.exit = Some(ExitRecord {
        code: Some(3),
        signal: None,
    });
    let mut s = status(HostState::Restarting {
        attempt: 1,
        last_error: String::new(),
    });
    s.last_crash = Some((crash, 1_700_000_000));
    let wire = contract(&s);
    let crash = wire.last_crash.unwrap();
    assert_eq!(crash.exit_code, Some(3));
    assert_eq!(crash.signal, None);
    assert_eq!(crash.at, "2023-11-14T22:13:20Z");
    assert_eq!(crash.error_kind, EngineHostErrorKind::Exited);
    assert!(!crash.stderr_tail.contains("sk-ant"));
    assert!(!crash.stderr_tail.contains("ANTHROPIC_API_KEY"));
    assert!(crash.stderr_tail.chars().count() <= 1024);
    assert_eq!(wire.restart_count, 2);
    assert_eq!(wire.host_version.as_deref(), Some("0.1.0"));
}

#[test]
fn a_signal_exit_is_reported_as_a_signal() {
    let mut crash = CrashInfo::new(CrashKind::Exited, "gone");
    crash.exit = Some(ExitRecord {
        code: None,
        signal: Some(9),
    });
    let mut s = status(HostState::Crashed);
    s.last_crash = Some((crash, 0));
    let wire = contract(&s).last_crash.unwrap();
    assert_eq!((wire.exit_code, wire.signal), (None, Some(9)));
}

#[test]
fn the_status_json_has_no_environment_or_arguments() {
    let json = serde_json::to_string(&contract(&status(HostState::Ready))).unwrap();
    for word in ["env", "args", "argv", "key", "token"] {
        assert!(!json.contains(word), "{word} in {json}");
    }
}

#[test]
fn the_threads_working_and_the_last_error_kind_are_on_the_wire_and_absent_when_there_are_none() {
    let quiet = contract(&status(HostState::Ready));
    assert_eq!((quiet.threads, quiet.last_error_kind), (None, None));
    let json = serde_json::to_value(contract(&status(HostState::Ready))).unwrap();
    assert!(json.get("threads").is_none() && json.get("last_error_kind").is_none());

    let mut busy = status(HostState::Ready);
    busy.threads = vec!["t-1".into(), "t-2".into()];
    busy.last_error = Some(crate::agents::driver::host::ErrorKind::SessionLost);
    let wire = contract(&busy);
    let threads = wire.threads.unwrap();
    assert_eq!(threads.len(), 2);
    assert_eq!(
        (
            threads[0].thread_id.as_str(),
            threads[0].family.as_str(),
            threads[0].state
        ),
        (
            "t-1",
            "claude",
            orbyn_contracts::EngineHostThreadState::Running
        )
    );
    assert_eq!(
        serde_json::to_value(wire.last_error_kind).unwrap(),
        serde_json::json!("session_lost")
    );
}

#[test]
fn every_protocol_error_kind_has_a_name_on_the_wire() {
    use crate::agents::driver::host::ErrorKind as K;
    let kinds = [
        (K::Auth, "auth"),
        (K::RateLimit, "rate_limit"),
        (K::Network, "network"),
        (K::InvalidRequest, "invalid_request"),
        (K::Unsupported, "unsupported"),
        (K::Cancelled, "cancelled"),
        (K::Engine, "engine"),
        (K::Host, "host"),
        (K::SessionLost, "session_lost"),
    ];
    for (kind, name) in kinds {
        let mut one = status(HostState::Ready);
        one.last_error = Some(kind);
        assert_eq!(
            serde_json::to_value(contract(&one).last_error_kind).unwrap(),
            serde_json::json!(name)
        );
    }
    // A kind the host did not name is not shown.
    let mut unknown = status(HostState::Ready);
    unknown.last_error = Some(K::Unknown);
    assert_eq!(contract(&unknown).last_error_kind, None);
}
