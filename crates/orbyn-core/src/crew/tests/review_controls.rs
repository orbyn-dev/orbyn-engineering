//! Consent and person-only lane waivers, with fake engines and no provider effects.

use serde_json::{Value, json};

use super::harness::{Turn, World, world_tiered};
use super::review_support::{arc, built, commit_all, crew, finding, repro, reviewed};

async fn single_vendor() -> World {
    world_tiered(
        true,
        arc(|n, root| {
            if n == 0 {
                built(root)
            } else {
                reviewed(vec![])
            }
        }),
        &[("alpha", false)],
        true,
    )
    .await
}

async fn request(w: &World, subject: &str, lane: &str) -> Value {
    let (answer, error) = w
        .lead("crew_review", json!({"task":subject,"lanes":[lane]}))
        .await;
    assert!(!error, "{answer}");
    // A review answer lists its lanes in start order and names each one, so the lane asked for is found by name.
    answer["lanes"]
        .as_array()
        .expect("the review answer lists its lanes")
        .iter()
        .find(|lane_answer| lane_answer["lane"] == lane)
        .unwrap_or_else(|| panic!("the review answer has the {lane} lane: {answer}"))
        .clone()
}

/// The review task of the one lane a test asks for. A review answer lists its lanes in start order, and a lane a repo
/// rule adds must not become the lane the test waits on.
fn one_lane(mut lanes: Vec<String>) -> String {
    assert_eq!(
        lanes.len(),
        1,
        "the test asks for one lane, and no rule adds another: {lanes:?}"
    );
    lanes.pop().expect("the lane's review task")
}

/// The status row of the `round` of `lane`, found by lane name and the round number the status records: the status
/// orders its lanes by name, so neither the lane nor the round under test is a place in a list.
fn lane_round<'a>(status: &'a Value, lane: &str, round: u32) -> &'a Value {
    let row = status["lanes"]
        .as_array()
        .expect("the review status lists its lanes")
        .iter()
        .find(|row| row["lane"].as_str() == Some(lane))
        .unwrap_or_else(|| panic!("the review status has the {lane} lane"));
    row["rounds"]
        .as_array()
        .expect("the review status lists a lane's rounds")
        .iter()
        .find(|row| row["round"] == round)
        .unwrap_or_else(|| panic!("the review status has the {lane} lane's round {round}"))
}

#[tokio::test]
async fn same_engine_requires_the_person_once_then_asks_again_for_the_next_round() {
    let w = single_vendor().await;
    let subject = w.built_work().await;
    let waiting = request(&w, &subject, "correctness").await;
    assert_eq!(waiting["started"], false);
    assert_eq!(waiting["state"], "needs_person");
    assert!(w.core.store.crew_reviews_of(&subject).unwrap().is_empty());
    let id = waiting["same_engine_consent"]["id"].as_str().unwrap();
    let view = w.core.crew_flow_task(&subject).unwrap();
    assert_eq!(view.review_consents.len(), 1);
    let consent = view.review_consents.first().expect("the pending consent");
    assert_eq!(consent.id, id);
    assert_eq!(
        consent.lane, "correctness",
        "the consent names the waiting lane"
    );
    assert!(consent.can_answer);
    assert_eq!(
        view.row
            .cells
            .iter()
            .find(|c| c.stage == orbyn_contracts::FlowStage::Code)
            .unwrap()
            .state,
        orbyn_contracts::FlowCellState::NeedsYou
    );
    let injected = json!({"request":id,"choice":"once","actor":"person"});
    assert!(
        w.core
            .crew_review_consent(&subject, &injected)
            .await
            .is_err()
    );
    let waited = w
        .core
        .crew_review_consent(&subject, &json!({"request":id,"choice":"wait"}))
        .await
        .unwrap();
    assert_eq!(waited["started"], false);
    assert!(w.core.store.crew_reviews_of(&subject).unwrap().is_empty());
    let started = w
        .core
        .crew_review_consent(&subject, &json!({"request":id,"choice":"once"}))
        .await
        .unwrap();
    let reviewer = started["task"].as_str().unwrap();
    w.until_state(reviewer, "passed").await;
    let status = w.status(&subject);
    assert_eq!(
        lane_round(&status, "correctness", 1)["same_engine_as_author"],
        true
    );
    let stored = w.core.store.crew_review_controls(&subject).unwrap();
    let consumed = stored
        .iter()
        .find(|control| control.id.as_str() == id)
        .expect("the answered consent's row");
    assert_eq!(consumed.actor, "person");
    assert_eq!(consumed.state, "consumed");
    let view = w.core.crew_flow_task(&subject).unwrap();
    assert!(view.review_consents.is_empty());
    let lane = view
        .lanes
        .iter()
        .find(|lane| lane.lane == "correctness")
        .expect("the correctness lane's row");
    assert_eq!(
        lane.same_engine_allowed_by,
        Some(orbyn_contracts::CrewSameEngineAllowedBy::PersonOnce)
    );
    let next = request(&w, &subject, "correctness").await;
    assert_eq!(next["started"], false);
    assert_eq!(next["same_engine_consent"]["round"], 2);
    assert_ne!(next["same_engine_consent"]["id"], id);
    assert!(
        w.core
            .crew_review_consent(&subject, &json!({"request":id,"choice":"once"}))
            .await
            .is_err()
    );
    let (forged, error) = w
        .lead(
            "crew_review",
            json!({"task":subject,"lanes":["correctness"],"consent":true}),
        )
        .await;
    assert!(error, "{forged}");
}

#[tokio::test]
async fn stale_head_and_expired_consent_never_launch_a_reviewer() {
    let w = single_vendor().await;
    let subject = w.built_work().await;
    let waiting = request(&w, &subject, "correctness").await;
    let id = waiting["same_engine_consent"]["id"].as_str().unwrap();
    let expiry = waiting["same_engine_consent"]["expires_unix"]
        .as_i64()
        .unwrap();
    assert!(
        w.core
            .store
            .answer_review_consent(id, true, expiry)
            .is_err()
    );
    commit_all(&w.repo, "new head");
    assert!(
        w.core
            .crew_review_consent(&subject, &json!({"request":id,"choice":"once"}))
            .await
            .is_err()
    );
    assert!(w.core.store.crew_reviews_of(&subject).unwrap().is_empty());
}

#[tokio::test]
async fn full_security_exception_uses_a_separate_lead_run_badge_and_high_risk() {
    let w = single_vendor().await;
    let subject = w.built_work().await;
    let started = request(&w, &subject, "security").await;
    let reviewer = started["task"].as_str().unwrap();
    w.until_state(reviewer, "passed").await;
    assert_ne!(w.worker(reviewer), w.worker(&subject));
    let routes = w.core.store.crew_route_decisions(reviewer).unwrap();
    let route = routes
        .iter()
        .find(|route| route.role == orbyn_contracts::RouteRole::Review)
        .expect("the reviewer's route decision");
    assert_eq!(
        route.lane.as_ref().map(|lane| lane.as_str()),
        Some("security"),
        "the security lane's own decision"
    );
    assert_eq!(route.tier, orbyn_contracts::RouteTier::Lead);
    assert!(w.core.review_security_requires_high(&subject).unwrap());
    let status = w.status(&subject);
    assert_eq!(
        lane_round(&status, "security", 1)["same_engine_as_author"],
        true
    );
    assert!(
        w.core
            .store
            .crew_review_controls(&subject)
            .unwrap()
            .is_empty()
    );
    w.core
        .store
        .set_crew_autopilot_mode("default", "off")
        .unwrap();
    let view = w.core.crew_flow_task(&subject).unwrap();
    assert!(view.same_engine_security_high);
    let lane = view
        .lanes
        .iter()
        .find(|lane| lane.lane == "security")
        .expect("the security lane's row");
    assert_eq!(
        lane.same_engine_allowed_by,
        Some(orbyn_contracts::CrewSameEngineAllowedBy::AutopilotFull),
        "the receipt comes from launch, not the later current mode"
    );
}

#[tokio::test]
async fn late_same_engine_security_raises_queued_risk_without_replacing_merge_intent() {
    let w = single_vendor().await;
    let subject = w.built_work().await;
    let target = crate::crew::MergeTarget {
        repo: w.repo.clone(),
        branch: "HEAD".into(),
        owed: Some(vec!["correctness".into()]),
    };
    let entry = w
        .core
        .crew_queue_merge(&subject, &target, 1_800_000_000)
        .await
        .unwrap();
    assert_ne!(entry.risk, "high", "the fixture starts below High");
    let started = request(&w, &subject, "security").await;
    w.until_state(started["task"].as_str().unwrap(), "passed")
        .await;
    w.until("Security raises the queued risk", |w| {
        w.core
            .store
            .merge_entry(&subject)
            .unwrap()
            .is_some_and(|e| e.risk == "high")
    })
    .await;
    let raised = w.core.store.merge_entry(&subject).unwrap().unwrap();
    assert_eq!(
        (
            &raised.candidate,
            &raised.branch,
            &raised.repo,
            raised.released
        ),
        (&entry.candidate, &entry.branch, &entry.repo, entry.released)
    );
    assert_eq!(
        (
            &raised.state,
            &raised.mode,
            &raised.note,
            raised.queued_unix
        ),
        (&entry.state, &entry.mode, &entry.note, entry.queued_unix)
    );
    assert_eq!(
        w.core.refresh_merge_review_risk(&entry).unwrap(),
        crate::crew::Risk::High,
        "a stale in-memory queue entry must not lower risk"
    );
    let again = w.core.store.merge_entry(&subject).unwrap().unwrap();
    assert_eq!(again.reasons, raised.reasons, "refresh is idempotent");
    w.core
        .store
        .set_crew_autopilot_mode("default", "off")
        .unwrap();
    assert_eq!(
        w.core.refresh_merge_review_risk(&entry).unwrap(),
        crate::crew::Risk::High,
        "changing mode cannot erase the launch receipt"
    );
}

#[tokio::test]
async fn on_and_off_ask_for_same_engine_security() {
    for mode in ["on", "off"] {
        let w = single_vendor().await;
        w.core
            .store
            .set_crew_autopilot_mode("default", mode)
            .unwrap();
        let subject = w.built_work().await;
        let waiting = request(&w, &subject, "security").await;
        assert_eq!(waiting["started"], false, "{mode}: {waiting}");
        assert!(w.core.store.crew_reviews_of(&subject).unwrap().is_empty());
    }
}

#[tokio::test]
async fn person_lane_removal_is_reasoned_never_locked_and_uses_pinned_rules() {
    let w = crew(arc(|n, root| {
        if n == 0 {
            built(root)
        } else {
            reviewed(vec![])
        }
    }))
    .await;
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    let rules = r#"{"lanes":[{"lane":"security","locked":true},{"lane":"licence"}]}"#;
    std::fs::write(w.repo.join(".orbyn/crew.json"), rules).unwrap();
    commit_all(&w.repo, "rules");
    let subject = w.built_work().await;
    assert!(
        w.core
            .crew_remove_review_lane(&subject, &json!({"lane":"licence","reason":" "}))
            .await
            .is_err()
    );
    assert!(
        w.core
            .crew_remove_review_lane(
                &subject,
                &json!({"lane":"licence","reason":"Reviewed manually.","actor":"person"})
            )
            .await
            .is_err()
    );
    // A checkout edit never unlocks the base's rule.
    std::fs::write(w.repo.join(".orbyn/crew.json"), r#"{"lanes":[]}"#).unwrap();
    let before = w.core.store.crew_review_controls(&subject).unwrap();
    let view = w.core.crew_review_control_view(&subject).await.unwrap();
    assert!(
        view.lanes
            .iter()
            .any(|l| l.lane == "security" && l.locked && !l.can_remove)
    );
    assert!(
        view.lanes
            .iter()
            .any(|l| l.lane == "licence" && !l.locked && l.can_remove)
    );
    assert_eq!(
        w.core.store.crew_review_controls(&subject).unwrap().len(),
        before.len()
    );
    assert!(
        w.core
            .crew_remove_review_lane(&subject, &json!({"lane":"security","reason":"Skip it."}))
            .await
            .is_err()
    );
    std::fs::write(w.repo.join(".orbyn/crew.json"), rules).unwrap();
    let removed = w
        .core
        .crew_remove_review_lane(
            &subject,
            &json!({"lane":"licence","reason":"Reviewed manually."}),
        )
        .await
        .unwrap();
    let removal = removed["controls"]
        .as_array()
        .expect("the removal answer lists its controls")
        .iter()
        .find(|control| control["lane"] == "licence")
        .expect("the licence lane's removal receipt");
    assert_eq!(removal["kind"], "remove_lane");
    assert_eq!(removal["state"], "removed");
    assert_eq!(removal["actor"], "person");
    let view = w.core.crew_review_control_view(&subject).await.unwrap();
    let receipt = view.lanes.iter().find(|l| l.lane == "licence").unwrap();
    assert!(!receipt.can_remove);
    assert_eq!(
        receipt.removal.as_ref().unwrap().reason,
        "Reviewed manually."
    );
    let repeated = w
        .core
        .crew_remove_review_lane(
            &subject,
            &json!({"lane":"licence","reason":"A replacement reason"}),
        )
        .await
        .unwrap();
    assert_eq!(repeated["receipt"], removed["receipt"]);
    let (answer, error) = w
        .lead(
            "crew_review",
            json!({"task":subject,"lanes":["correctness"]}),
        )
        .await;
    assert!(!error, "{answer}");
    assert!(
        !answer["lanes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["lane"] == "licence")
    );
    assert_eq!(
        std::fs::read_to_string(w.repo.join(".orbyn/crew.json")).unwrap(),
        rules
    );
    let (_, error) = w
        .lead(
            "crew_remove_review_lane",
            json!({"task":subject,"lane":"security","reason":"Skip."}),
        )
        .await;
    assert!(error, "no model tool removes a lane");
}

#[tokio::test]
async fn a_lane_waiver_never_clears_a_confirmed_finding() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 1, "The fix is missing.")]),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let (checked, error) = w
        .lead(
            "crew_check_finding",
            json!({"finding":id,"repro":repro("confirm")}),
        )
        .await;
    assert!(!error, "{checked}");
    assert_eq!(w.status(&subject)["blocked"], true);
    w.core
        .crew_remove_review_lane(
            &subject,
            &json!({"lane":"correctness","reason":"I will review this lane."}),
        )
        .await
        .unwrap();
    assert_eq!(w.status(&subject)["blocked"], true);
    assert!(w.core.review_lane_removed(&subject, "correctness").unwrap());
}

#[tokio::test]
async fn a_once_grant_is_consumed_atomically_and_rejects_candidate_drift() {
    let w = single_vendor().await;
    let subject = w.built_work().await;
    let waiting = request(&w, &subject, "correctness").await;
    let control = &waiting["same_engine_consent"];
    let id = control["id"].as_str().unwrap();
    let now = crate::util::unix_now();
    w.core.store.answer_review_consent(id, true, now).unwrap();
    let scope = crate::store::ReviewConsent {
        subject: &subject,
        lane: "correctness",
        commit: control["commit"].as_str().unwrap(),
        round: 1,
        engine: "alpha",
        model: "wrong-model",
    };
    assert!(!w.core.store.consume_review_consent(&scope, now).unwrap());
    let scope = crate::store::ReviewConsent {
        model: control["model"].as_str().unwrap(),
        ..scope
    };
    // The serialized store transactions have one winner even if called together.
    let (a, b) = std::thread::scope(|threads| {
        let a = threads.spawn(|| w.core.store.consume_review_consent(&scope, now).unwrap());
        let b = threads.spawn(|| w.core.store.consume_review_consent(&scope, now).unwrap());
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_ne!(a, b);
    assert!(!w.core.store.consume_review_consent(&scope, now).unwrap());
}

#[tokio::test]
async fn cancelling_a_reviewer_cannot_reuse_its_once_grant() {
    let w = world_tiered(
        true,
        arc(|n, root| if n == 0 { built(root) } else { Turn::Hang }),
        &[("alpha", false)],
        true,
    )
    .await;
    let subject = w.built_work().await;
    let waiting = request(&w, &subject, "correctness").await;
    let id = waiting["same_engine_consent"]["id"].as_str().unwrap();
    let started = w
        .core
        .crew_review_consent(&subject, &json!({"request":id,"choice":"once"}))
        .await
        .unwrap();
    let task = started["task"].as_str().unwrap();
    w.core
        .crew_cancel_task(task, Some("End this review."), "user")
        .await
        .unwrap();
    assert_eq!(w.task(task).state, "cancelled");
    let waiting_again = request(&w, &subject, "correctness").await;
    assert_eq!(waiting_again["started"], false);
    assert_eq!(waiting_again["same_engine_consent"]["round"], 2);
    assert!(
        w.core
            .crew_review_consent(&subject, &json!({"request":id,"choice":"once"}))
            .await
            .is_err()
    );
}
