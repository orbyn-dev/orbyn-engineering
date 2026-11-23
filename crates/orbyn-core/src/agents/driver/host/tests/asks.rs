//! `permission` and `question` frames reach the core's gate and desk as events, and the answer
//! goes back as `respond`. Policy is the core's: the host only waits.

use std::time::Duration;

use orbyn_contracts::{ActionKind, Question};

use super::*;
use crate::agents::driver::{PermissionRequest, QuestionAsk};
use crate::conversation::questions::Answers;
use crate::governance::approvals::Verdict;

fn responds(rig: &Rig) -> Vec<Value> {
    rig.log()
        .into_iter()
        .filter(|line| line["type"] == "respond")
        .collect()
}

/// The next permission the run raises.
async fn next_permission(started: &mut Started) -> PermissionRequest {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let EngineEvent::Permission(request) =
                started.events.recv().await.expect("the run ended first")
            {
                return request;
            }
        }
    })
    .await
    .expect("no permission was raised")
}

async fn next_question(started: &mut Started) -> QuestionAsk {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let EngineEvent::Question(ask) =
                started.events.recv().await.expect("the run ended first")
            {
                return ask;
            }
        }
    })
    .await
    .expect("no question was raised")
}

#[tokio::test]
async fn a_permission_is_the_gates_to_decide_and_an_allow_is_once_only() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut started = start(&rig.driver(), spec("[perm]")).await.unwrap();
    let request = next_permission(&mut started).await;
    assert_eq!(request.kind, ActionKind::FileChange);
    assert_eq!(request.target, "/workspace/repo/app/a.txt");
    assert_eq!(request.detail["tool"], "Edit");
    assert_eq!(request.detail["input"]["old_string"], "x");
    request.reply.send(Verdict::Allow).unwrap();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let sent = responds(&rig);
    assert_eq!(sent.len(), 1, "{sent:?}");
    // Nothing but the behaviour: no updated permissions, no always.
    assert_eq!(
        sent[0]["decision"],
        serde_json::json!({ "behavior": "allow" })
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_denial_carries_the_gates_words_to_the_engine() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut started = start(&rig.driver(), spec("[perm]")).await.unwrap();
    let request = next_permission(&mut started).await;
    request
        .reply
        .send(Verdict::Deny("Plan mode changes nothing.".into()))
        .unwrap();
    started.outcome().await;
    assert_eq!(
        responds(&rig)[0]["decision"],
        serde_json::json!({ "behavior": "deny", "message": "Plan mode changes nothing." })
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn the_engines_options_and_input_reach_the_gate_untouched_so_approve_once_binds_them() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut started = start(&rig.driver(), spec("[perm-opts]")).await.unwrap();
    let request = next_permission(&mut started).await;
    assert_eq!(
        request.detail["options"],
        serde_json::json!([
            { "kind": "allow_once", "name": "Allow once" },
            { "kind": "reject_once", "name": "Reject" },
        ])
    );
    assert_eq!(
        request.detail["input"],
        serde_json::json!({ "file_path": "/workspace/repo/app/a.txt", "old_string": "x", "new_string": "y" })
    );
    // The frame's own bookkeeping stays outside what the gate binds.
    assert!(request.detail.get("requestId").is_none() && request.detail.get("sendId").is_none());
    request.reply.send(Verdict::Allow).unwrap();
    started.outcome().await;
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn orbyns_own_tools_are_allowed_without_asking_as_on_the_cli_path() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let started = start(&rig.driver(), spec("[perm-own]")).await.unwrap();
    let (events, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EngineEvent::Permission(_)))
    );
    assert_eq!(
        responds(&rig)[0]["decision"],
        serde_json::json!({ "behavior": "allow" })
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_question_goes_to_the_desk_and_the_answers_come_back_by_question_text() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut started = start(&rig.driver(), spec("[question-1]")).await.unwrap();
    let ask = next_question(&mut started).await;
    let questions: &Vec<Question> = &ask.questions;
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].question, "Which parts?");
    assert!(questions[0].multi_select);
    assert_eq!(questions[0].options[1].label, "B");
    let mut answers = Answers::new();
    answers.insert("Which parts?".into(), vec!["A".into(), "B".into()]);
    ask.reply.send(Some(answers)).unwrap();
    started.outcome().await;
    assert_eq!(
        responds(&rig)[0]["decision"],
        serde_json::json!({ "answers": { "Which parts?": "A, B" } })
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_dismissed_question_is_a_denial_and_one_with_nothing_to_show_is_never_asked() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut started = start(&rig.driver(), spec("[question-1]")).await.unwrap();
    let ask = next_question(&mut started).await;
    ask.reply.send(None).unwrap();
    started.outcome().await;
    let (events, _) = start(&rig.driver(), spec("[question]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EngineEvent::Question(_)))
    );
    let sent = responds(&rig);
    assert_eq!(sent.len(), 2, "{sent:?}");
    for line in sent {
        assert_eq!(line["decision"], serde_json::json!({ "behavior": "deny" }));
    }
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn an_answer_that_comes_after_the_run_is_over_is_dropped_and_never_sent() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let mut started = start(&rig.driver(), spec("[perm]")).await.unwrap();
    let request = next_permission(&mut started).await;
    // The person stops the run while the ask is open.
    started.handle.cancel.cancel();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    // The hold is gone: the gate's answer finds nobody, and the host hears nothing more.
    assert!(request.reply.send(Verdict::Allow).is_err());
    // The host reads its input in order, so once the next run has finished on the same host, any
    // late answer would already be in what it received.
    let (_, outcome) = start(&rig.driver(), spec("again"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(responds(&rig).is_empty(), "{:?}", responds(&rig));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_host_that_crashes_with_an_ask_open_drops_the_hold_and_is_never_answered_after_restart() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let mut started = start(&driver, spec("[perm-crash]")).await.unwrap();
    let request = next_permission(&mut started).await;
    let (_, outcome) = started.outcome().await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    assert!(
        request.reply.send(Verdict::Allow).is_err(),
        "the hold stayed"
    );
    wait_for(&rig.pool, |s| *s == HostState::Ready, "running again").await;
    // The next message runs on the new host; nothing is retried onto it.
    let (_, outcome) = start(&driver, spec("again")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(responds(&rig).is_empty(), "{:?}", responds(&rig));
    rig.pool.shutdown().await;
}
