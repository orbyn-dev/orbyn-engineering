use super::*;

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn backoff_starts_at_one_second_doubles_and_caps_at_thirty() {
    let timings = Timings {
        max_restarts: 100,
        ..Timings::default()
    };
    let mut ledger = Ledger::default();
    let start = Instant::now();
    let waits: Vec<_> = (0..8)
        .map(|n| ledger.crashed(start + secs(n), Duration::ZERO, &timings))
        .collect();
    let expect = [1, 2, 4, 8, 16, 30, 30, 30];
    for (wait, expect) in waits.iter().zip(expect) {
        assert_eq!(*wait, Restart::After(secs(expect)));
    }
}

#[test]
fn a_host_that_stayed_up_a_minute_starts_the_backoff_over() {
    let timings = Timings::default();
    let mut ledger = Ledger::default();
    let start = Instant::now();
    ledger.crashed(start, Duration::ZERO, &timings);
    ledger.crashed(start + secs(2), Duration::ZERO, &timings);
    assert_eq!(
        ledger.crashed(start + secs(100), secs(60), &timings),
        Restart::After(secs(1))
    );
    // a shorter uptime does not reset it
    assert_eq!(
        ledger.crashed(start + secs(102), secs(59), &timings),
        Restart::After(secs(2))
    );
}

#[test]
fn five_restarts_in_five_minutes_give_up_and_the_window_slides() {
    let timings = Timings::default();
    let mut ledger = Ledger::default();
    let start = Instant::now();
    for n in 0..5 {
        assert!(matches!(
            ledger.crashed(start + secs(n * 10), Duration::ZERO, &timings),
            Restart::After(_)
        ));
    }
    assert_eq!(
        ledger.crashed(start + secs(60), Duration::ZERO, &timings),
        Restart::GiveUp
    );
    // old restarts leave the window
    assert!(matches!(
        ledger.crashed(start + secs(5 * 60 + 100), Duration::ZERO, &timings),
        Restart::After(_)
    ));
}

#[test]
fn clearing_forgets_every_restart() {
    let timings = Timings::default();
    let mut ledger = Ledger::default();
    let start = Instant::now();
    for n in 0..5 {
        ledger.crashed(start + secs(n), Duration::ZERO, &timings);
    }
    ledger.clear();
    assert_eq!(ledger.restarts(), 0);
    assert_eq!(
        ledger.crashed(start + secs(6), Duration::ZERO, &timings),
        Restart::After(secs(1))
    );
}

#[test]
fn ten_malformed_frames_in_a_minute_are_a_crash() {
    let timings = Timings::default();
    let mut window = MalformedWindow::default();
    let start = Instant::now();
    for n in 0..9 {
        assert!(!window.record(start + secs(n), &timings));
    }
    assert!(window.record(start + secs(9), &timings));
}

#[test]
fn malformed_frames_spread_over_minutes_are_not() {
    let timings = Timings::default();
    let mut window = MalformedWindow::default();
    let start = Instant::now();
    for n in 0..30 {
        assert!(!window.record(start + secs(n * 7), &timings));
    }
}

#[test]
fn a_mismatch_names_both_numbers_and_the_action() {
    let state = HostState::Mismatch { app: 1, host: 2 };
    let problem = state.problem().unwrap();
    assert!(problem.contains("app protocol 1") && problem.contains("host protocol 2"));
    assert_eq!(state.action(), Some("Update Orbyn"));
    assert!(state.is_final());
    assert!(!HostState::Ready.is_final());
    assert!(
        !HostState::Restarting {
            attempt: 1,
            last_error: String::new()
        }
        .is_final()
    );
}

#[test]
fn an_exit_is_described_by_code_or_signal() {
    assert_eq!(
        ExitRecord {
            code: Some(3),
            signal: None
        }
        .describe(),
        "exit code 3"
    );
    assert_eq!(
        ExitRecord {
            code: None,
            signal: Some(9)
        }
        .describe(),
        "signal 9"
    );
}
