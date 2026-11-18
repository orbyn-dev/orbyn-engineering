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
    answer["lanes"][0].clone()
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
    assert_eq!(view.review_consents[0].id, id);
    assert!(view.review_consents[0].can_answer);
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
        status["lanes"][0]["rounds"][0]["same_engine_as_author"],
        true
    );
    let stored = w.core.store.crew_review_controls(&subject).unwrap();
    assert_eq!(stored[0].actor, "person");
    assert_eq!(stored[0].state, "consumed");
    let view = w.core.crew_flow_task(&subject).unwrap();
    assert!(view.review_consents.is_empty());
    assert_eq!(
        view.lanes[0].same_engine_allowed_by,
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
    assert_eq!(routes[0].tier, orbyn_contracts::RouteTier::Lead);
    assert!(w.core.review_security_requires_high(&subject).unwrap());
    assert_eq!(
        w.status(&subject)["lanes"][0]["rounds"][0]["same_engine_as_author"],
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
    assert_eq!(
        view.lanes[0].same_engine_allowed_by,
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
    assert_eq!(removed["controls"][0]["state"], "removed");
    assert_eq!(removed["controls"][0]["actor"], "person");
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
