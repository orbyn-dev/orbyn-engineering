//! The check a person makes, on fake engines (issue #794): a `manual` check parks the task instead of skipping, a person's answer is what
//! the verifier reads, and the task carries on or fails on that answer. No model and no network: the scripted builder writes one file and
//! reports.

use orbyn_contracts::{GateDecider, GateDecision, ResultState};
use serde_json::json;

use super::harness::{World, brief};
use super::review_support::{arc, built, crew};

/// A key whose first check code can run and whose second only a person can answer.
fn manual_key() -> serde_json::Value {
    json!({
        "protected_paths": [],
        "checks": [
            { "id": "chk-scope", "kind": "file_scope", "params": { "only": ["src/**"] },
              "proves": "Only leased paths changed." },
            { "id": "chk-manual", "kind": "manual", "params": { "name": "The page renders" },
              "proves": "A person looked at the page." }
        ]
    })
}

/// Starts the builder on `alpha` with the manual key and waits until the task is not being worked on any more.
async fn started(w: &World) -> String {
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            manual_key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until("the task holds or ends", |w| {
        matches!(
            w.task(&id).state.as_str(),
            "passed" | "needs_person" | "stopped"
        )
    })
    .await;
    id
}

#[tokio::test]
async fn a_manual_check_parks_the_task_instead_of_skipping_it() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = started(&w).await;
    // The task does not pass on the checks around the one nobody made: it waits for a person.
    assert_eq!(
        w.task(&id).state,
        "needs_person",
        "{}",
        json!({ "results": w.core.store.crew_results(&id).unwrap(),
                "evidence": w.core.store.crew_evidence(&id).unwrap() })
    );
    assert_eq!(
        w.core.store.crew_manual_checks_waiting(&id).unwrap(),
        ["chk-manual"]
    );
    // The result says so, and the checks gate is not written while the wait is open.
    let results = w.core.store.crew_results(&id).unwrap();
    assert_eq!(
        results.last().map(|result| result.state),
        Some(ResultState::NeedsPerson)
    );
    let gates = w.core.store.crew_gates(&id).unwrap();
    assert!(gates.is_empty(), "{gates:?}");
    // Nothing a person decided is recorded yet.
    let held = w
        .core
        .store
        .crew_manual_check(&id, "chk-manual")
        .unwrap()
        .unwrap();
    assert_eq!(held.decided_unix, None);
}

#[tokio::test]
async fn a_persons_pass_resumes_the_task_and_its_gate_is_theirs() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = started(&w).await;
    assert_eq!(w.task(&id).state, "needs_person");
    let waiting = w
        .core
        .crew_decide_manual_check(
            &id,
            "chk-manual",
            true,
            Some("I looked at the page."),
            "user",
        )
        .await
        .unwrap();
    assert!(waiting.is_empty(), "{waiting:?}");
    w.until_state(&id, "passed").await;
    // The person's own gate at the checks stage, with the check as its subject and no Evidence of its own.
    let gates = w.core.store.crew_gates(&id).unwrap();
    let theirs = gates
        .iter()
        .find(|gate| gate.decided_by == GateDecider::Person)
        .expect("the person's gate");
    assert_eq!(theirs.decided_by, GateDecider::Person);
    assert_eq!(theirs.decision, GateDecision::Pass);
    assert!(theirs.evidence.is_empty());
    assert_eq!(Some(theirs.reason.as_str()), Some("I looked at the page."));
    // The decided check passed in the run, and no wait is left.
    let results = w.core.store.crew_results(&id).unwrap();
    let last = results.last().expect("a result");
    assert_eq!(last.state, ResultState::Passed);
    assert!(
        last.checks
            .iter()
            .any(|check| check.check.as_str() == "chk-manual"),
        "{:?}",
        last.checks
    );
    assert!(
        w.core
            .store
            .crew_manual_checks_waiting(&id)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_persons_denial_does_not_let_the_task_complete() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = started(&w).await;
    assert_eq!(w.task(&id).state, "needs_person");
    w.core
        .crew_decide_manual_check(&id, "chk-manual", false, Some("The page is blank."), "user")
        .await
        .unwrap();
    // A denial is a failed check: the worker is told the person's reason and gets its ordinary retry, and the task never passes.
    let worker = w.worker(&id);
    w.until("the worker is told the person's reason", |w| {
        w.sent_to(&worker)
            .iter()
            .any(|prompt| prompt.contains("The page is blank."))
    })
    .await;
    assert_ne!(w.task(&id).state, "passed");
    assert!(
        w.core
            .store
            .crew_manual_check(&id, "chk-manual")
            .unwrap()
            .unwrap()
            .state
            == crate::store::ManualCheckState::Failed,
    );
    assert!(
        w.core
            .store
            .crew_results(&id)
            .unwrap()
            .iter()
            .all(|result| result.state != ResultState::Passed),
    );
}

#[tokio::test]
async fn a_decision_whose_gate_and_resume_were_lost_is_finished_by_the_repeat() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = started(&w).await;
    assert_eq!(w.task(&id).state, "needs_person");
    // The stored half of a crash between the steps: the check is decided, and neither its gate nor the resume happened.
    w.core
        .store
        .decide_manual_check(
            &id,
            "chk-manual",
            true,
            Some("looked"),
            crate::util::unix_now(),
        )
        .unwrap();
    assert!(
        w.core
            .store
            .crew_gates(&id)
            .unwrap()
            .iter()
            .all(|gate| gate.decided_by != GateDecider::Person),
        "no person gate yet"
    );
    assert_eq!(w.task(&id).state, "needs_person");
    // The person's own call again finishes it: the gate is written and the task carries on, not refused and not stuck.
    let waiting = w
        .core
        .crew_decide_manual_check(&id, "chk-manual", true, Some("looked"), "user")
        .await
        .unwrap();
    assert!(waiting.is_empty(), "{waiting:?}");
    w.until_state(&id, "passed").await;
    assert_eq!(
        w.core
            .store
            .crew_gates(&id)
            .unwrap()
            .iter()
            .filter(|gate| gate.decided_by == GateDecider::Person)
            .count(),
        1
    );
}

#[tokio::test]
async fn a_repeat_answer_finishes_a_decision_and_other_answers_are_refused() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = started(&w).await;
    // A check the core runs is never the person's to decide.
    let refused = w
        .core
        .crew_decide_manual_check(&id, "chk-scope", true, None, "user")
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("a person makes"), "{refused}");
    // A check the task does not have at all.
    let refused = w
        .core
        .crew_decide_manual_check(&id, "chk-missing", true, None, "user")
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("no such check"), "{refused}");
    w.core
        .crew_decide_manual_check(&id, "chk-manual", true, None, "user")
        .await
        .unwrap();
    w.until_state(&id, "passed").await;
    // The same answer again is not a second decision: it answers with what still waits (nothing) and writes no second gate, so a
    // decision a crash interrupted is finished by repeating it rather than left stuck.
    let waiting = w
        .core
        .crew_decide_manual_check(&id, "chk-manual", true, None, "user")
        .await
        .unwrap();
    assert!(waiting.is_empty(), "{waiting:?}");
    let person_gates = w
        .core
        .store
        .crew_gates(&id)
        .unwrap()
        .into_iter()
        .filter(|gate| gate.decided_by == GateDecider::Person)
        .count();
    assert_eq!(person_gates, 1, "one decision, one gate");
    // The opposite answer on a decided check is still refused.
    let refused = w
        .core
        .crew_decide_manual_check(&id, "chk-manual", false, None, "user")
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("already"), "{refused}");
}

#[tokio::test]
async fn a_note_that_hides_text_is_refused_and_decides_nothing() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = started(&w).await;
    // The note reaches a worker's reason, so a zero-width character or a control one is refused like any text a person types.
    for note in ["a\u{200b}b", "a\u{0007}b", &"x".repeat(201)] {
        let refused = w
            .core
            .crew_decide_manual_check(&id, "chk-manual", false, Some(note), "user")
            .await;
        assert!(refused.is_err(), "{note:?} was accepted");
    }
    assert_eq!(w.task(&id).state, "needs_person");
    assert_eq!(
        w.core.store.crew_manual_checks_waiting(&id).unwrap(),
        ["chk-manual"]
    );
}
