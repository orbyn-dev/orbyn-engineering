//! An explicit engine from the lead or the person: it wins over the scores, is still
//! filtered, and is never swapped for another. Roles, lanes and the other-vendor rule.

use orbyn_contracts::{AuthState, RejectionWhy, RouteAssigner, RouteRole};
use serde_json::json;

use super::fake::*;

fn assign(id: &str, by: &str) -> serde_json::Value {
    json!({ "engine": { "id": id, "assigned_by": by, "tag": format!("@{id}") } })
}

#[test]
fn an_explicit_engine_wins_over_a_higher_score_and_the_card_says_who_assigned_it() {
    let mut busy = engine("codex");
    busy.used_percent = Some(90);
    let decision = routed(
        &brief(assign("codex", "person")),
        &[engine("claude"), busy],
        &Ask::default(),
    );
    assert_eq!(decision.engine.as_str(), "codex");
    assert_eq!(decision.assigned_by, RouteAssigner::Person);
    assert_eq!(
        decision.requested.as_ref().unwrap().engine.as_str(),
        "codex"
    );
    assert!(decision.reasons.iter().any(|r| {
        r.code.as_str() == "assigned_by_person" && r.text.as_str() == "Assigned by you (@codex)."
    }));
    // Claude would have scored higher and stays visible as an alternative.
    assert_eq!(decision.scores[0].engine.as_str(), "claude");
    assert_eq!(decision.scores.len(), 2);
    verified(&decision);

    let lead = routed(
        &brief(assign("claude", "lead")),
        &[engine("claude"), engine("codex")],
        &Ask::default(),
    );
    assert_eq!(lead.assigned_by, RouteAssigner::Lead);
    assert!(
        lead.reasons
            .iter()
            .any(|r| r.text.as_str() == "Assigned by the lead (@claude).")
    );
}

#[test]
fn an_explicit_engine_that_fails_a_filter_is_refused_and_never_swapped() {
    let mut out = engine("codex");
    out.auth = AuthState::LoggedOut;
    let refusal = refused(
        &brief(assign("codex", "person")),
        &[engine("claude"), out],
        &Ask::default(),
    );
    assert_eq!(refusal.message, "Codex isn't signed in.");
    assert_eq!(refusal.requested.as_ref().unwrap().engine.as_str(), "codex");
    assert_eq!(refusal.rejected.len(), 1);
    assert_eq!(refusal.rejected[0].why, RejectionWhy::NotSignedIn);

    let mut loose = engine("opencode");
    loose.capabilities.managed = false;
    let refusal = refused(
        &brief(assign("opencode", "lead")),
        &[engine("claude"), loose],
        &Ask::default(),
    );
    assert_eq!(refusal.rejected[0].why, RejectionWhy::NotManaged);
}

#[test]
fn an_engine_this_machine_does_not_have_is_never_routed_to() {
    let refusal = refused(
        &brief(assign("cursor", "person")),
        &[engine("claude")],
        &Ask::default(),
    );
    assert_eq!(refusal.rejected[0].why, RejectionWhy::NotInstalled);
    assert_eq!(refusal.rejected[0].engine.as_str(), "cursor");
    assert!(refusal.message.contains("isn't configured"));
}

#[test]
fn a_named_model_must_be_listed_and_a_tag_cannot_lower_a_lead_role() {
    let tagged = |model: &str| {
        brief(
            json!({ "kind": "review", "files": [], "allowed_effects": ["read_repo"],
            "engine": { "id": "claude", "model": model, "assigned_by": "person", "tag": "@claude" } }),
        )
    };
    let engines = [engine("claude"), engine("codex")];
    let ask = |role| Ask {
        role: Some(role),
        ..Ask::default()
    };
    // A model that matches by name resolves to the listed id.
    let build = routed(
        &brief(json!({ "engine": { "id": "claude", "model": "haiku", "assigned_by": "person" } })),
        &engines,
        &Ask::default(),
    );
    assert_eq!(build.model.as_str(), "claude-haiku-4");
    assert_eq!(build.tier, orbyn_contracts::RouteTier::Scout);
    // The lead can pin a worker to Haiku the same way: the lead's engine is {id, model, assigned_by}.
    let pinned = routed(
        &brief(json!({ "engine": { "id": "claude", "model": "haiku", "assigned_by": "lead" } })),
        &engines,
        &Ask::default(),
    );
    assert_eq!(pinned.model.as_str(), "claude-haiku-4");
    assert_eq!(pinned.assigned_by, RouteAssigner::Lead);
    // A model the engine does not list is refused.
    let missing = refused(&tagged("gpt-nope"), &engines, &ask(RouteRole::CodeReview));
    assert_eq!(missing.rejected[0].why, RejectionWhy::MissingCapability);
    // Security review stays lead tier: Sonnet is refused, Opus is allowed.
    let low = refused(&tagged("sonnet"), &engines, &ask(RouteRole::SecurityReview));
    assert_eq!(low.rejected[0].why, RejectionWhy::TierBelowLead);
    let high = routed(&tagged("opus"), &engines, &ask(RouteRole::SecurityReview));
    assert_eq!(high.model.as_str(), "claude-opus-4");
    assert_eq!(high.tier, orbyn_contracts::RouteTier::Lead);
    assert_eq!(high.role, RouteRole::SecurityReview);
}

#[test]
fn a_review_lane_is_named_exactly_when_the_role_is_review() {
    let engines = [engine("claude"), engine("codex")];
    let review = Ask {
        role: Some(RouteRole::Review),
        lane: Some("accessibility"),
        ..Ask::default()
    };
    let decision = routed(&reading(json!({ "kind": "review" })), &engines, &review);
    assert_eq!(
        decision.lane.as_ref().map(|l| l.as_str()),
        Some("accessibility")
    );
    verified(&decision);
    let no_lane = Ask {
        role: Some(RouteRole::Review),
        ..Ask::default()
    };
    assert!(matches!(
        run(&reading(json!({})), &engines, &no_lane),
        Err(crate::crew::RouteFailure::Invalid(_))
    ));
    let stray = Ask {
        lane: Some("docs"),
        ..Ask::default()
    };
    assert!(matches!(
        run(&reading(json!({})), &engines, &stray),
        Err(crate::crew::RouteFailure::Invalid(_))
    ));
}

#[test]
fn a_reviewer_is_from_the_other_vendor_by_default_and_a_second_key_always() {
    let both = [engine("claude"), engine("codex")];
    let review = Ask {
        author: Some("claude"),
        ..Ask::default()
    };
    let brief_r = reading(json!({ "kind": "review" }));
    let decision = routed(&brief_r, &both, &review);
    assert_eq!(decision.engine.as_str(), "codex");
    let why = decision
        .rejected
        .iter()
        .find(|r| r.engine.as_str() == "claude")
        .unwrap();
    assert_eq!(why.why, RejectionWhy::SameEngineRefused);

    // Only the author's vendor is left: a code review may use it, badged; a second key may not.
    let alone = [engine("claude")];
    let same = routed(&brief_r, &alone, &review);
    assert_eq!(same.engine.as_str(), "claude");
    assert!(
        same.reasons
            .iter()
            .any(|r| r.code.as_str() == "same_engine_as_author")
    );
    let key = Ask {
        author: Some("claude"),
        role: Some(RouteRole::SecondKey),
        ..Ask::default()
    };
    let refusal = refused(&brief_r, &alone, &key);
    assert_eq!(refusal.rejected[0].why, RejectionWhy::SameEngineRefused);

    // Tagging the author's own engine as reviewer is allowed and badged; as second key, refused.
    let own = reading(
        json!({ "kind": "review", "engine": { "id": "claude", "assigned_by": "person", "tag": "@claude review" } }),
    );
    let tagged = routed(&own, &both, &review);
    assert_eq!(tagged.engine.as_str(), "claude");
    assert!(
        tagged
            .reasons
            .iter()
            .any(|r| r.text.as_str() == "Same engine as author.")
    );
    assert_eq!(
        refused(&own, &both, &key).rejected[0].why,
        RejectionWhy::SameEngineRefused
    );
}

fn lane(name: &'static str, author: &'static str, locked: bool) -> Ask {
    Ask {
        role: Some(RouteRole::Review),
        lane: Some(name),
        author: Some(author),
        locked,
        ..Ask::default()
    }
}

#[test]
fn a_review_lane_defaults_to_the_worker_tier_on_another_vendor() {
    let both = [engine("claude"), engine("codex")];
    let work = reading(json!({ "kind": "review" }));
    let decision = routed(&work, &both, &lane("correctness", "claude", false));
    assert_eq!(decision.tier, orbyn_contracts::RouteTier::Worker);
    assert_eq!(decision.engine.as_str(), "codex", "the author was claude");
    assert_eq!(decision.model.as_str(), "terra-1");
    assert!(
        decision
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "review_lane_tier"
                && r.text.as_str().contains("worker tier"))
    );
    verified(&decision);
    let back = routed(&work, &both, &lane("correctness", "codex", false));
    assert_eq!(back.engine.as_str(), "claude");
    assert_eq!(back.model.as_str(), "claude-sonnet-4");
    // The reviewer is never the author's vendor while another is configured.
    assert_eq!(
        back.rejected
            .iter()
            .find(|r| r.engine.as_str() == "codex")
            .map(|r| r.why),
        Some(RejectionWhy::SameEngineRefused)
    );
}

#[test]
fn a_pinned_model_wins_over_the_default_lane_tier() {
    let both = [engine("claude"), engine("codex")];
    let pin = |model: &str| {
        reading(json!({ "kind": "review",
            "engine": { "id": "codex", "model": model, "assigned_by": "lead" } }))
    };
    let up = routed(&pin("sol"), &both, &lane("correctness", "claude", false));
    assert_eq!(up.model.as_str(), "sol-1");
    assert_eq!(up.tier, orbyn_contracts::RouteTier::Lead);
    assert_eq!(up.assigned_by, RouteAssigner::Lead);
    let down = routed(&pin("luna"), &both, &lane("correctness", "claude", false));
    assert_eq!(down.model.as_str(), "luna-1");
    assert_eq!(down.tier, orbyn_contracts::RouteTier::Scout);
}

#[test]
fn security_and_locked_lanes_stay_on_the_lead_tier_even_when_pinned_lower() {
    let both = [engine("claude"), engine("codex")];
    let work = reading(json!({ "kind": "review" }));
    let security = routed(&work, &both, &lane("security", "claude", false));
    assert_eq!(security.tier, orbyn_contracts::RouteTier::Lead);
    assert_eq!(security.model.as_str(), "sol-1");
    let locked = routed(&work, &both, &lane("correctness", "claude", true));
    assert_eq!(locked.tier, orbyn_contracts::RouteTier::Lead);
    assert_eq!(locked.model.as_str(), "sol-1");
    // A free-form lane that only mentions security is an ordinary lane.
    let named = routed(&work, &both, &lane("secrets", "claude", false));
    assert_eq!(named.tier, orbyn_contracts::RouteTier::Worker);
    // A pin below the lead tier is refused, never swapped.
    let low = reading(json!({ "kind": "review",
        "engine": { "id": "codex", "model": "terra", "assigned_by": "lead" } }));
    let refusal = refused(&low, &both, &lane("security", "claude", false));
    assert_eq!(refusal.rejected[0].why, RejectionWhy::TierBelowLead);
    let refusal = refused(&low, &both, &lane("correctness", "claude", true));
    assert_eq!(refusal.rejected[0].why, RejectionWhy::TierBelowLead);
}

#[test]
fn an_engine_with_no_tier_mapping_is_not_a_default_lane_reviewer() {
    // OpenCode has no mapping: it would fill the worker tier as a builder, but it never reviews.
    let engines = [engine("claude"), engine("opencode"), engine("codex")];
    let work = reading(json!({ "kind": "review" }));
    let decision = routed(&work, &engines, &lane("correctness", "claude", false));
    assert_eq!(decision.engine.as_str(), "codex");
    assert!(
        decision
            .rejected
            .iter()
            .any(|r| r.engine.as_str() == "opencode" && r.why == RejectionWhy::MissingCapability)
    );
}

#[test]
fn a_lane_keeps_the_lead_tier_when_only_the_authors_vendor_can_review() {
    let work = reading(json!({ "kind": "review" }));
    // Claude alone (OpenCode never reviews): the lane cannot have another vendor's eyes, so it stays on the lead tier.
    let alone = [engine("claude"), engine("opencode")];
    let decision = routed(&work, &alone, &lane("docs", "claude", false));
    assert_eq!(decision.engine.as_str(), "claude");
    assert_eq!(decision.tier, orbyn_contracts::RouteTier::Lead);
    assert_eq!(decision.model.as_str(), "claude-opus-4");
    assert!(
        decision
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "same_engine_as_author")
    );
    assert!(
        decision.reasons.iter().any(
            |r| r.code.as_str() == "review_lane_tier" && r.text.as_str().contains("own engine")
        )
    );
    verified(&decision);
}

#[test]
fn a_merge_owed_code_lane_is_pinned_lead_by_the_caller_and_others_stay_worker() {
    let both = [engine("claude"), engine("codex")];
    let work = reading(json!({ "kind": "review" }));
    let owed = Ask {
        locked: true,
        ..lane("correctness", "claude", false)
    };
    let decision = routed(&work, &both, &owed);
    assert_eq!(decision.tier, orbyn_contracts::RouteTier::Lead);
    assert_eq!(
        routed(&work, &both, &lane("performance", "claude", false)).tier,
        orbyn_contracts::RouteTier::Worker
    );
}
