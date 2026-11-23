use std::time::Duration;

use super::*;
use crate::agents::driver::host::outcome::HOST_STOPPED;
use crate::agents::driver::{Failure, SteerMessage, ThreadDisposition};
use crate::error::CoreError;

fn failure(outcome: EngineOutcome) -> Failure {
    match outcome {
        EngineOutcome::FailedWith(failure) => failure,
        other => panic!("not a failure: {other:?}"),
    }
}

fn texts(events: &[EngineEvent]) -> String {
    use orbyn_contracts::AgUiEvent;
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::TextMessageContent { delta, .. }) => Some(delta.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_turn_streams_deltas_names_its_session_and_finishes_once() {
    let Some(pool) = real_fake(fast()) else {
        return;
    };
    let driver = driver(&pool);
    let (events, outcome) = start(&driver, spec("hi")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert_eq!(texts(&events), "echo: hi");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::Session(id) if id.starts_with("fake-")))
    );
    assert!(events.iter().any(|e| matches!(e, EngineEvent::Turn(_))));
    pool.shutdown().await;
}

#[tokio::test]
async fn only_the_first_run_done_counts() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (_, finished) = start(&rig.driver(), spec("[dup]"))
        .await
        .unwrap()
        .finish()
        .await;
    assert_eq!(finished, vec![EngineOutcome::Completed]);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cancel_asks_the_host_and_ends_cancelled() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let started = start(&rig.driver(), spec("[hang]")).await.unwrap();
    started.handle.cancel.cancel();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    assert!(rig.log().iter().any(|r| r["type"] == "cancel"));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_cancelled_run_that_the_host_never_answers_still_ends() {
    let timings = Timings {
        cancel_grace: Duration::from_millis(200),
        ..fast()
    };
    let Some(rig) = standin(&[("STANDIN_CAPS", "")], timings) else {
        return;
    };
    let started = start(&rig.driver(), spec("[stall]")).await.unwrap();
    started.handle.cancel.cancel();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_rejected_steer_is_reported_against_its_message() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let started = start(&rig.driver(), spec("[hang]")).await.unwrap();
    let input = started.handle.input.clone().unwrap();
    input
        .send(SteerMessage {
            message_id: "m-ok".into(),
            prompt: "fine".into(),
        })
        .unwrap();
    input
        .send(SteerMessage {
            message_id: "m-bad".into(),
            prompt: "please reject".into(),
        })
        .unwrap();
    let mut events = started.events;
    let failed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(EngineEvent::SteerFailed(steer)) = events.recv().await {
                return steer;
            }
        }
    })
    .await
    .expect("no SteerFailed");
    assert_eq!(failed.message_id, "m-bad");
    started.handle.cancel.cancel();
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_steer_the_host_cannot_take_is_reported_without_asking() {
    let Some(rig) = standin(&[("STANDIN_CAPS", "")], fast()) else {
        return;
    };
    let started = start(&rig.driver(), spec("[hang]")).await.unwrap();
    started
        .handle
        .input
        .as_ref()
        .unwrap()
        .send(SteerMessage {
            message_id: "m".into(),
            prompt: "x".into(),
        })
        .unwrap();
    let mut events = started.events;
    let seen = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(EngineEvent::SteerFailed(_)) = events.recv().await {
                return;
            }
        }
    })
    .await;
    assert!(seen.is_ok());
    assert!(
        !rig.log().iter().any(|r| r["type"] == "steer"),
        "steer was sent"
    );
    started.handle.cancel.cancel();
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_resume_the_engine_lost_is_session_lost() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut resume = spec("hi");
    resume.resume = Some("gone".into());
    let error = start(&rig.driver(), resume)
        .await
        .err()
        .expect("resume must fail");
    assert!(matches!(error, CoreError::SessionLost), "{error:?}");
    let mut found = spec("hi");
    found.resume = Some("kept".into());
    let (_, outcome) = start(&rig.driver(), found).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_run_lost_mid_turn_by_the_engine_is_session_lost() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (_, outcome) = start(&rig.driver(), spec("[lost]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::SessionLost);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_confined_run_the_host_cannot_confine_is_refused_and_never_sent() {
    let Some(rig) = standin(&[("STANDIN_UNSUPPORTED", "1")], fast()) else {
        return;
    };
    let (_dir, mut confined) = support::confined_spec();
    confined.user_prompt = "hi".into();
    let error = start(&rig.driver(), confined)
        .await
        .err()
        .expect("must be refused");
    assert!(
        matches!(error, CoreError::EngineUnavailable(_)),
        "{error:?}"
    );
    let log = rig.log();
    assert!(
        !log.iter().any(|r| r["type"] == "send"),
        "a refused run was sent: {log:?}"
    );
    // On macOS the frame was built and the host refused it; elsewhere the core refused first.
    if cfg!(target_os = "macos") && crate::host::confine::unavailable().is_none() {
        assert!(
            log.iter()
                .any(|r| r["type"] == "open" && r["confinement"] == true)
        );
    }
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_crash_mid_run_fails_the_run_and_the_host_restarts_with_backoff() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let (_, outcome) = start(&driver, spec("[crash]"))
        .await
        .unwrap()
        .outcome()
        .await;
    let failure = failure(outcome);
    assert_eq!(failure.message, HOST_STOPPED);
    assert_eq!(failure.disposition, ThreadDisposition::Unknown);
    wait_for(&rig.pool, |s| *s == HostState::Ready, "running again").await;
    let status = rig.pool.status().await;
    assert_eq!(status.restart_count, 1);
    let (crash, at) = status.last_crash.unwrap();
    assert_eq!(crash.exit.unwrap().code, Some(3));
    assert!(at > 0);
    // the next message runs on the new host
    let (_, outcome) = start(&driver, spec("again")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_rate_limit_reports_its_reset_and_a_turn_limit_says_so_plainly() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let (events, outcome) = start(&driver, spec("[limit]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::Limited {
            resets_at: Some(1_900_000_000)
        }
    )));
    let failure = failure(outcome);
    assert_eq!(failure.disposition, ThreadDisposition::Reusable);
    let (_, outcome) = start(&driver, spec("[maxturns]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        failure_message(outcome),
        "The engine stopped because it reached its turn limit."
    );
    rig.pool.shutdown().await;
}

fn failure_message(outcome: EngineOutcome) -> String {
    failure(outcome).message
}

#[tokio::test]
async fn the_hosts_background_task_maps_a_stop_request() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let started = start(&rig.driver(), spec("[bg]")).await.unwrap();
    let control = started.handle.control.clone().unwrap();
    // the run is already over by the time the stand-in answers; the control must not break it
    let _ = control.send(crate::agents::driver::RunControl::StopWorkflow {
        tool_call_id: "call-1".into(),
    });
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    rig.pool.shutdown().await;
}

fn leftover_texts(events: &[EngineEvent]) -> String {
    use orbyn_contracts::AgUiEvent;
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Leftover(inner) => match &**inner {
                EngineEvent::Agui(AgUiEvent::TextMessageContent { delta, .. }) => {
                    Some(delta.as_str())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn what_a_leftover_turn_says_is_shown_but_a_silent_prompt_has_no_words_of_its_own() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (events, outcome) = start(&rig.driver(), spec("[leftover]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    // Shown, marked as not the prompt's...
    assert_eq!(leftover_texts(&events), "the old agent finished");
    // ...and nothing the core would count as the engine answering.
    assert_eq!(texts(&events), "", "{events:?}");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn the_prompts_own_words_after_a_leftover_turn_are_not_marked() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (events, _) = start(&rig.driver(), spec("[leftover-answer]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(leftover_texts(&events), "the old agent finished");
    assert_eq!(texts(&events), "here is your answer");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_claude_run_keeps_the_prompts_message_id_as_the_engine_turn() {
    let Some(node) = node() else {
        return;
    };
    let Some(rig) = standin_family("claude", &[], fast()) else {
        return;
    };
    let driver = driver_on_node(&rig.pool, &node);
    let (events, outcome) = start(&driver, spec("hi")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let sent = rig
        .log()
        .into_iter()
        .find(|line| line["type"] == "send")
        .expect("a send request");
    let message_id = sent["messageId"].as_str().expect("send carries an id");
    let turns: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Turn(id) => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert!(!turns.is_empty(), "{events:?}");
    assert!(
        turns.iter().all(|id| *id == message_id),
        "the host's synthetic turn must not replace the prompt id: {turns:?}"
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn the_prompt_goes_to_the_host_under_an_id_the_engines_frames_can_name() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (events, _) = start(&rig.driver(), spec("hi"))
        .await
        .unwrap()
        .outcome()
        .await;
    // An engine that says nothing about the prompt has nothing marked.
    assert_eq!(texts(&events), "echo: hi");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EngineEvent::Leftover(_)))
    );
    let sent = rig
        .log()
        .into_iter()
        .find(|line| line["type"] == "send")
        .unwrap();
    let id = sent["messageId"]
        .as_str()
        .expect("send carries a messageId");
    assert_eq!(id.len(), 36);
    assert_eq!(id.matches('-').count(), 4);
    rig.pool.shutdown().await;
}
