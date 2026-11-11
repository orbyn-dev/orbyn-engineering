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
