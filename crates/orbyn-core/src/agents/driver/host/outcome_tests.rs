use super::*;
use crate::agents::driver::host::frames::{Incoming, decode};

fn done(json: &str) -> RunDone {
    let Incoming::RunDone(done) = decode(json).unwrap() else {
        panic!("not a runDone");
    };
    done
}

fn failure(outcome: EngineOutcome) -> Failure {
    match outcome {
        EngineOutcome::FailedWith(failure) => failure,
        other => panic!("not a failure: {other:?}"),
    }
}

#[test]
fn completed_and_cancelled_map_as_named() {
    let (outcome, events) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":true,"result":{"reason":"completed"}}"#,
    ));
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(events.is_empty());
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":true,"result":{"reason":"cancelled"}}"#,
    ));
    assert_eq!(outcome, EngineOutcome::Cancelled);
}

#[test]
fn max_turns_and_refused_become_a_plain_sentence() {
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":true,"result":{"reason":"maxTurns"}}"#,
    ));
    assert_eq!(
        failure(outcome).message,
        "The engine stopped because it reached its turn limit."
    );
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":true,"result":{"reason":"refused"}}"#,
    ));
    assert_eq!(
        failure(outcome).message,
        "The engine declined to continue this request."
    );
}

#[test]
fn a_failure_carries_its_message_retry_and_disposition() {
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":false,"error":{"method":"send","cause":{"name":"E","message":"overloaded","kind":"network"}},"result":{"reason":"failed","threadDisposition":"reusable","retry":{"afterMs":2000,"why":"overloaded"}}}"#,
    ));
    let failure = failure(outcome);
    assert_eq!(failure.message, "overloaded");
    assert_eq!(failure.disposition, ThreadDisposition::Reusable);
    assert_eq!(
        failure.retry,
        Some(RetryInfo {
            after_ms: Some(2000),
            why: RetryWhy::Overloaded
        })
    );
}

#[test]
fn a_rate_limit_with_a_reset_time_emits_limited() {
    let (outcome, events) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":false,"error":{"method":"send","cause":{"name":"E","message":"limit","kind":"rate_limit"}},"result":{"reason":"failed","threadDisposition":"broken","retry":{"why":"rate_limit","resetsAt":1700000000}}}"#,
    ));
    assert!(matches!(
        events.as_slice(),
        [EngineEvent::Limited {
            resets_at: Some(1_700_000_000)
        }]
    ));
    assert_eq!(failure(outcome).disposition, ThreadDisposition::Broken);
}

#[test]
fn session_lost_and_cancelled_kinds_win_over_the_failure() {
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":false,"error":{"method":"send","cause":{"name":"E","message":"gone","kind":"session_lost"}}}"#,
    ));
    assert_eq!(outcome, EngineOutcome::SessionLost);
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":false,"error":{"name":"E","message":"x","kind":"cancelled"}}"#,
    ));
    assert_eq!(outcome, EngineOutcome::Cancelled);
}

#[test]
fn a_failure_with_no_message_still_says_something_plain() {
    let (outcome, _) = map(&done(
        r#"{"type":"runDone","sendId":1,"ok":false,"result":{"reason":"failed"}}"#,
    ));
    assert_eq!(
        failure(outcome).message,
        "The engine could not finish this turn."
    );
}

#[test]
fn a_stopped_host_is_unknown_and_says_what_is_kept() {
    let failure = failure(host_stopped());
    assert_eq!(failure.disposition, ThreadDisposition::Unknown);
    assert_eq!(failure.message, HOST_STOPPED);
}
