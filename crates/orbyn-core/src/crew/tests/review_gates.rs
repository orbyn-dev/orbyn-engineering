//! Code-owned lane Gates; fake engines report findings, never a pass/fail decision.

use orbyn_contracts::{
    CheckOutcome, CrewCommit, CrewObject, CrewRole, GateDecider, GateDecision, GateStage,
};
use serde_json::json;

use super::harness::Turn;
use super::review_support::{arc, built, crew, finding, reviewed};

/// Uses the real queue consumer but deliberately stops before any merge writer:
/// missing or held lane Evidence must be rejected even when task state passed.
async fn assert_local_merge_held(w: &super::harness::World, subject: &str) {
    let target = crate::crew::MergeTarget {
        repo: w.repo.clone(),
        branch: "HEAD".into(),
        owed: Some(vec!["correctness".into()]),
    };
    let before = super::autopilot_merge::gitq(&w.repo, &["rev-parse", "HEAD"]).await;
    w.core
        .crew_queue_merge(subject, &target, 1_800_000_000)
        .await
        .unwrap();
    let outcomes = w
        .core
        .crew_process_merges(&target, 1_800_000_001)
        .await
        .unwrap();
    assert!(
        matches!(outcomes.as_slice(), [crate::crew::MergeOutcome::Blocked { why, .. }]
        if why.contains("lane Gate")),
        "{outcomes:?}"
    );
    assert_eq!(
        super::autopilot_merge::gitq(&w.repo, &["rev-parse", "HEAD"]).await,
        before
    );
}

#[tokio::test]
async fn lane_gate_needs_finished_read_only_evidence_and_the_current_commit() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![]),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let review = w.core.store.crew_review(&lanes[0]).unwrap().unwrap();
    let gate = w
        .core
        .review_lane_gate(&subject, "correctness", &review.commit)
        .unwrap()
        .unwrap();
    assert_eq!(gate.stage, GateStage::Review);
    assert_eq!(gate.lane.as_ref().unwrap().as_str(), "correctness");
    assert_eq!(gate.author.role, CrewRole::Core);
    assert_eq!(gate.decided_by, GateDecider::Code);
    assert_eq!(gate.decision, GateDecision::Pass);
    assert!(!gate.evidence.is_empty());
    gate.validate().unwrap();
    assert!(
        w.core
            .review_lane_gate(&subject, "security", &review.commit)
            .unwrap()
            .is_none()
    );
    let newer = "1111111111111111111111111111111111111111";
    let stale = w
        .core
        .review_lane_gate(&subject, "correctness", newer)
        .unwrap()
        .unwrap();
    assert_eq!(stale.decision, GateDecision::Fail);

    let later = w.review(&subject, &["correctness"]).await;
    assert_eq!(w.task(&later[0]).state, "running");
    assert!(
        w.core
            .review_lane_gate(&subject, "correctness", &review.commit)
            .unwrap()
            .is_none(),
        "an older pass never stands for a new round without Evidence"
    );
}

#[tokio::test]
async fn unchecked_findings_hold_the_lane_gate_without_changing_global_blocked() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![finding("src/lib.txt", 1, "Check this claim.")]),
    }))
    .await;
    let subject = w.built_work().await;
    assert_local_merge_held(&w, &subject).await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let review = w.core.store.crew_review(&lanes[0]).unwrap().unwrap();
    let before = w
        .core
        .review_lane_gate(&subject, "correctness", &review.commit)
        .unwrap()
        .unwrap();
    assert_eq!(before.decision, GateDecision::Fail);
    assert_eq!(w.status(&subject)["blocked"], false);
    assert_local_merge_held(&w, &subject).await;
    let id = w.first_finding(&subject);
    let (answer, error) = w
        .lead(
            "crew_check_finding",
            json!({"finding":id,"repro":{
                "id":"repro-unconfirmed","kind":"command","params":{"run":"exit 0"},
                "proves":"The claim does not hold."
            }}),
        )
        .await;
    assert!(!error, "{answer}");
    let after = w
        .core
        .review_lane_gate(&subject, "correctness", &review.commit)
        .unwrap()
        .unwrap();
    assert_eq!(after.decision, GateDecision::Pass);
    assert_ne!(before.id, after.id);
    let stored = w.core.store.crew_gates(&subject).unwrap();
    assert!(
        stored
            .iter()
            .any(|g| g.stage == GateStage::Review && g.id == after.id)
    );
    let count = stored.len();
    w.status(&subject);
    w.status(&subject);
    assert_eq!(
        w.core.store.crew_gates(&subject).unwrap().len(),
        count,
        "status reads do not write Gate snapshots"
    );
}

#[tokio::test]
async fn a_failed_read_only_run_cannot_produce_a_passing_lane_gate() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => {
            std::fs::write(root.join("src/lib.txt"), "reviewer edit\n").unwrap();
            reviewed(vec![])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "stopped").await;
    let review = w.core.store.crew_review(&lanes[0]).unwrap().unwrap();
    let gate = w
        .core
        .review_lane_gate(&subject, "correctness", &review.commit)
        .unwrap();
    assert!(gate.is_none_or(|g| g.decision == GateDecision::Fail));
}

/// Synthetic later-stage core records must not become the candidate or the
/// original check set. They represent workflow activity after the task passed.
fn add_later_stage_records(w: &super::harness::World, task: &str) {
    let mut evidence = w
        .core
        .store
        .crew_evidence(task)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    evidence.commit = CrewCommit::new("1111111111111111111111111111111111111111").unwrap();
    evidence.command = crate::crew::verify::short("Later-stage synthetic check").unwrap();
    evidence.outcome = CheckOutcome::Pass;
    evidence.id = CrewObject::Evidence(evidence.clone())
        .computed_id()
        .unwrap();
    w.core
        .store
        .add_crew_object(task, &CrewObject::Evidence(evidence.clone()))
        .unwrap();
    let original = w
        .core
        .store
        .crew_gates(task)
        .unwrap()
        .into_iter()
        .find(|g| g.stage == GateStage::Checks)
        .unwrap();
    for stage in [GateStage::Review, GateStage::Merge, GateStage::Checks] {
        let mut later = original.clone();
        later.stage = stage;
        later.evidence = vec![evidence.id.clone()];
        later.lane = (stage == GateStage::Review)
            .then(|| orbyn_contracts::CrewToken::new("correctness").unwrap());
        later.id = CrewObject::Gate(later.clone()).computed_id().unwrap();
        later.validate().unwrap();
        w.core
            .store
            .add_crew_object(task, &CrewObject::Gate(later))
            .unwrap();
    }
}

#[tokio::test]
async fn later_review_and_merge_gates_never_repin_a_candidate_or_a_lane_verification() {
    let w = crew(arc(|n, root| {
        if n == 0 {
            built(root)
        } else {
            reviewed(vec![])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    w.until("the review has its core Result", |w| {
        w.core
            .store
            .crew_results(&lanes[0])
            .unwrap()
            .iter()
            .any(|r| r.state == orbyn_contracts::ResultState::Passed)
    })
    .await;
    let result = w
        .core
        .store
        .crew_results(&subject)
        .unwrap()
        .into_iter()
        .find(|r| r.state == orbyn_contracts::ResultState::Passed)
        .unwrap();
    let commit = result.commit.as_ref().unwrap().as_str();
    let before = w
        .core
        .review_lane_gate(&subject, "correctness", commit)
        .unwrap()
        .unwrap();
    assert_eq!(before.decision, GateDecision::Pass);
    // An actual lane snapshot already exists; then append unrelated core activity
    // to both the builder and reviewer tasks, including a later checks Gate.
    add_later_stage_records(&w, &subject);
    add_later_stage_records(&w, &lanes[0]);
    let after = w
        .core
        .review_lane_gate(&subject, "correctness", commit)
        .unwrap()
        .unwrap();
    assert_eq!(after.decision, GateDecision::Pass);
    assert_eq!(after.subject, before.subject);
    assert_eq!(after.evidence, before.evidence);
    let later_lane = w.review(&subject, &["performance"]).await;
    w.until_state(&later_lane[0], "passed").await;
    assert_eq!(
        w.core
            .store
            .crew_review(&later_lane[0])
            .unwrap()
            .unwrap()
            .commit,
        commit
    );
    let passed = result
        .checks
        .iter()
        .filter(|c| c.outcome == CheckOutcome::Pass)
        .count();
    let expected = format!("{passed} of {} checks passed", result.checks.len());
    assert!(
        w.sent_to(&w.worker(&later_lane[0]))
            .join("\n")
            .contains(&expected)
    );
}
