//! An engine whose worker run would be refused at its start (`host::confine::worker_refusal`) is never assigned a worker. The
//! router leaves it out, a start with no other engine is refused up front with one plain reason and stores nothing, a pinned
//! engine is refused the same way, and the dry run agrees.

use orbyn_contracts::{RejectionWhy, RouteRole};
use serde_json::json;

use super::fake::*;
use crate::host::confine::WorkerRefusal;

fn refusing(mut view: crate::crew::EngineView) -> crate::crew::EngineView {
    view.crew_refusal = Some(WorkerRefusal {
        reason: "Crew workers on Codex are not available yet.".into(),
        machine: false,
    });
    view
}

fn pin(id: &str) -> serde_json::Value {
    json!({ "engine": { "id": id, "assigned_by": "person", "tag": format!("@{id}") } })
}

#[test]
fn default_routing_picks_the_next_allowed_engine_at_the_same_tier() {
    // Codex is the person's default and would win the tie, but a worker on it would be refused.
    let ask = Ask {
        default_engine: Some("codex"),
        ..Ask::default()
    };
    // Codex also has the most quota headroom, which alone would decide the score.
    let mut busy = engine("claude");
    busy.used_percent = Some(60);
    let mut idle = engine("codex");
    idle.used_percent = Some(0);
    let engines = [refusing(idle.clone()), busy.clone()];
    let decision = routed(&brief(json!({})), &engines, &ask);
    assert_eq!(decision.engine.as_str(), "claude");
    assert_eq!(decision.tier, orbyn_contracts::RouteTier::Worker);
    let why = decision
        .rejected
        .iter()
        .find(|r| r.engine.as_str() == "codex")
        .expect("the refusal is shown");
    assert_eq!(why.why, RejectionWhy::MissingCapability);
    assert!(
        why.text
            .as_str()
            .starts_with("Codex can't run Crew workers."),
        "{}",
        why.text.as_str()
    );
    assert!(decision.scores.iter().all(|s| s.engine.as_str() != "codex"));
    verified(&decision);
    // Without the refusal the same machine does pick Codex.
    let free = routed(&brief(json!({})), &[idle, busy], &ask);
    assert_eq!(free.engine.as_str(), "codex");
}

#[test]
fn with_only_the_refused_engine_the_route_is_refused_with_its_reason() {
    let refusal = refused(
        &brief(json!({})),
        &[refusing(engine("codex"))],
        &Ask::default(),
    );
    assert!(refusal.message.contains("No engine"), "{}", refusal.message);
    let one = &refusal.rejected[0];
    assert!(one.text.as_str().contains("not available yet"));
    assert!(one.text.as_str().contains("another engine"));
}

#[test]
fn a_pinned_refused_engine_is_refused_and_never_swapped() {
    let refusal = refused(
        &brief(pin("codex")),
        &[refusing(engine("codex")), engine("claude")],
        &Ask::default(),
    );
    assert!(
        refusal.message.starts_with("Codex can't run Crew workers."),
        "{}",
        refusal.message
    );
    assert_eq!(refusal.requested.unwrap().engine.as_str(), "codex");
}

#[test]
fn review_lanes_do_not_pick_a_refused_engine() {
    let ask = Ask {
        role: Some(RouteRole::Review),
        lane: Some("correctness"),
        author: Some("claude"),
        ..Ask::default()
    };
    let engines = [
        refusing(engine("codex")),
        engine("claude"),
        engine("opencode"),
    ];
    let reading = reading(json!({ "kind": "review" }));
    let decision = routed(&reading, &engines, &ask);
    assert_ne!(decision.engine.as_str(), "codex");
    assert!(
        decision
            .rejected
            .iter()
            .any(|r| r.engine.as_str() == "codex" && r.text.as_str().contains("Crew workers"))
    );
}

#[test]
fn a_machine_wide_refusal_says_no_engine_would_do() {
    let mut view = engine("claude");
    view.crew_refusal = Some(WorkerRefusal {
        reason: "Crew runs on macOS in the beta; Windows later.".into(),
        machine: true,
    });
    let refusal = refused(&brief(json!({})), &[view], &Ask::default());
    let text = refusal.rejected[0].text.as_str();
    assert!(
        text.starts_with("Crew can't run workers on this machine."),
        "{text}"
    );
    assert!(!text.contains("Choose another engine"), "{text}");
}

fn headroom(id: &str, used: u32) -> crate::crew::EngineView {
    let mut view = engine(id);
    view.used_percent = Some(used);
    view
}

fn has(decision: &orbyn_contracts::RouteDecision, code: &str) -> bool {
    decision.reasons.iter().any(|r| r.code.as_str() == code)
}

#[test]
fn a_worker_stays_on_the_leads_provider_even_when_another_has_more_headroom() {
    let ask = Ask {
        lead: Some("claude"),
        ..Ask::default()
    };
    let engines = [headroom("claude", 80), headroom("codex", 0)];
    let decision = routed(&brief(json!({})), &engines, &ask);
    assert_eq!(decision.engine.as_str(), "claude");
    assert!(has(&decision, "lead_provider"));
    // Without a lead engine the better headroom still wins, as before.
    let free = routed(&brief(json!({})), &engines, &Ask::default());
    assert_eq!(free.engine.as_str(), "codex");
    // The seam: `any` lets every provider compete.
    let any = crate::crew::RouterConfig {
        worker_provider: crate::crew::WorkerProvider::Any,
        ..crate::crew::RouterConfig::default()
    };
    let input = crate::crew::RouteInput {
        brief: &brief(json!({})),
        run_id: orbyn_contracts::CrewId::new("run-01").unwrap(),
        created_at: orbyn_contracts::CrewTimestamp::new("2026-11-12T09:00:00Z").unwrap(),
        role: None,
        lane: None,
        lane_keeps_lead: false,
        author_engine: None,
        lead_engine: Some(orbyn_contracts::CrewToken::new("claude").unwrap()),
        default_engine: None,
        classes: None,
    };
    let any =
        crate::crew::route(&input, &engines, &any, &crate::crew::EvalHistory::default()).unwrap();
    assert_eq!(any.engine.as_str(), "codex");
}

#[test]
fn a_refused_lead_provider_falls_back_with_a_plain_reason() {
    let ask = Ask {
        lead: Some("codex"),
        ..Ask::default()
    };
    let engines = [refusing(engine("codex")), engine("claude")];
    let decision = routed(&brief(json!({})), &engines, &ask);
    assert_eq!(decision.engine.as_str(), "claude");
    let note = decision
        .reasons
        .iter()
        .find(|r| r.code.as_str() == "lead_provider_unavailable")
        .expect("the fallback says why");
    assert_eq!(
        note.text.as_str(),
        "Codex workers are not available here, so Claude takes this."
    );
    // A lead provider that is not installed falls back the same way.
    let mut gone = engine("codex");
    gone.installed = false;
    let decision = routed(&brief(json!({})), &[gone, engine("claude")], &ask);
    assert_eq!(decision.engine.as_str(), "claude");
    assert!(has(&decision, "lead_provider_unavailable"));
}

#[test]
fn a_review_lane_still_goes_to_another_vendor_and_a_pin_is_honoured() {
    let lane = Ask {
        role: Some(RouteRole::Review),
        lane: Some("correctness"),
        author: Some("claude"),
        lead: Some("claude"),
        ..Ask::default()
    };
    let engines = [engine("claude"), engine("codex")];
    let decision = routed(&reading(json!({ "kind": "review" })), &engines, &lane);
    assert_eq!(decision.engine.as_str(), "codex");
    assert!(!has(&decision, "lead_provider"));
    let ask = Ask {
        lead: Some("claude"),
        ..Ask::default()
    };
    let pinned = routed(&brief(pin("codex")), &engines, &ask);
    assert_eq!(pinned.engine.as_str(), "codex");
    let refused_pin = refused(
        &brief(pin("codex")),
        &[engine("claude"), refusing(engine("codex"))],
        &ask,
    );
    assert!(refused_pin.message.contains("can't run Crew workers"));
}

#[test]
fn a_single_vendor_machine_keeps_the_lead_tier_when_the_other_vendor_is_refused() {
    let ask = Ask {
        role: Some(RouteRole::Review),
        lane: Some("correctness"),
        author: Some("claude"),
        lead: Some("claude"),
        ..Ask::default()
    };
    let engines = [engine("claude"), refusing(engine("codex"))];
    let decision = routed(&reading(json!({ "kind": "review" })), &engines, &ask);
    assert_eq!(decision.engine.as_str(), "claude");
    assert_eq!(decision.tier, orbyn_contracts::RouteTier::Lead);
    assert!(has(&decision, "same_engine_as_author"));
}

#[test]
fn a_second_key_is_refused_when_its_only_other_vendor_is_refused() {
    let ask = Ask {
        role: Some(RouteRole::SecondKey),
        author: Some("claude"),
        lead: Some("claude"),
        ..Ask::default()
    };
    let engines = [engine("claude"), refusing(engine("codex"))];
    let refusal = refused(&reading(json!({ "kind": "review" })), &engines, &ask);
    assert!(
        refusal
            .rejected
            .iter()
            .any(|r| r.engine.as_str() == "claude" && r.why == RejectionWhy::SameEngineRefused)
    );
    assert!(refusal.rejected.iter().any(
        |r| r.engine.as_str() == "codex" && r.text.as_str().contains("can't run Crew workers")
    ));
}

// The rest runs a real core with the restriction on, so it needs the macOS one to say yes to the other engine.
#[cfg(target_os = "macos")]
mod core {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::pin;
    use crate::crew::tests::harness::{Turn, World, brief, key, world_confined_with};

    async fn two() -> World {
        // `acodex` sorts first and is of the Codex family, so it would win a tie if it were allowed.
        world_confined_with(
            Arc::new(|_, _| Turn::Hang),
            &[("alpha", false), ("acodex", false)],
        )
        .await
    }

    fn count(w: &World, table: &str) -> i64 {
        w.core
            .store
            .reader()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn nothing_stored(w: &World) {
        for table in [
            "crew_tasks",
            "crew_runs",
            "crew_leases",
            "crew_route_decisions",
            "crew_briefs",
        ] {
            assert_eq!(count(w, table), 0, "{table}");
        }
        assert!(w.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn crew_start_assigns_the_allowed_engine() {
        let w = two().await;
        let (answer, error) = w.start(brief(json!({})), key()).await;
        assert!(!error, "{answer}");
        assert_eq!(answer["route"]["engine"], "alpha");
        assert!(
            answer["route"]["rejected"]
                .to_string()
                .contains("can't run Crew workers")
        );
        assert_eq!(count(&w, "crew_tasks"), 1);
    }

    #[tokio::test]
    async fn crew_start_with_only_the_refused_engine_stores_nothing() {
        let w = world_confined_with(Arc::new(|_, _| Turn::Hang), &[("acodex", false)]).await;
        let (answer, error) = w.start(brief(json!({})), key()).await;
        assert!(error, "{answer}");
        let text = answer["message"].as_str().unwrap_or_default().to_owned();
        assert!(text.contains("can't run Crew workers"), "{answer}");
        assert!(text.contains("not available yet"), "{answer}");
        nothing_stored(&w);
    }

    #[tokio::test]
    async fn a_pinned_refused_engine_is_refused_up_front() {
        let w = two().await;
        let (answer, error) = w.start(brief(pin("acodex")), key()).await;
        assert!(error, "{answer}");
        assert!(
            answer.to_string().contains("can't run Crew workers"),
            "{answer}"
        );
        nothing_stored(&w);
    }

    #[tokio::test]
    async fn the_dry_run_agrees_with_the_start() {
        let w = two().await;
        let (open, _) = w
            .call(
                &w.lead_token,
                "crew_route",
                json!({ "brief": brief(json!({})) }),
            )
            .await;
        assert_eq!(open["decision"]["engine"], "alpha", "{open}");
        assert!(
            open["decision"]["rejected"]
                .to_string()
                .contains("can't run Crew workers")
        );
        let (pinned, _): (Value, bool) = w
            .call(
                &w.lead_token,
                "crew_route",
                json!({ "brief": brief(pin("acodex")) }),
            )
            .await;
        assert_eq!(pinned["routed"], false, "{pinned}");
        assert!(
            pinned["refusal"]["message"]
                .as_str()
                .unwrap()
                .contains("can't run Crew workers")
        );
    }
}
