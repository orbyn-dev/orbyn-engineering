//! Each hard filter, in the order of the plan, and the rule that every engine is either
//! scored or rejected with a reason.

use orbyn_contracts::{
    AuthState, CompatibilityStatus, RejectionWhy, RouteDecision, RouteRejection, RouteRole,
};
use serde_json::json;

use super::fake::*;

fn why<'a>(decision: &'a RouteDecision, engine: &str) -> &'a RouteRejection {
    decision
        .rejected
        .iter()
        .find(|r| r.engine.as_str() == engine)
        .unwrap_or_else(|| panic!("{engine} was not rejected: {:?}", decision.rejected))
}

#[test]
fn an_engine_that_is_not_installed_or_turned_off_is_rejected_with_its_name() {
    let mut gone = engine("codex");
    gone.installed = false;
    let mut off = engine("opencode");
    off.turned_off = true;
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), gone, off],
        &Ask::default(),
    );
    assert_eq!(decision.engine.as_str(), "claude");
    assert_eq!(why(&decision, "codex").why, RejectionWhy::NotInstalled);
    assert_eq!(
        why(&decision, "codex").text.as_str(),
        "Codex isn't installed."
    );
    assert_eq!(why(&decision, "opencode").why, RejectionWhy::NotInstalled);
    assert!(
        why(&decision, "opencode")
            .text
            .as_str()
            .contains("turned off")
    );
    verified(&decision);
}

#[test]
fn signed_out_and_unconfirmed_engines_are_not_signed_in() {
    let mut out = engine("codex");
    out.auth = AuthState::LoggedOut;
    let mut unsure = engine("opencode");
    unsure.auth = AuthState::Unknown;
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), out, unsure],
        &Ask::default(),
    );
    assert_eq!(why(&decision, "codex").why, RejectionWhy::NotSignedIn);
    assert_eq!(
        why(&decision, "codex").text.as_str(),
        "Codex isn't signed in."
    );
    assert_eq!(why(&decision, "opencode").why, RejectionWhy::NotSignedIn);
    assert!(
        why(&decision, "opencode")
            .text
            .as_str()
            .contains("couldn't be confirmed")
    );
}

#[test]
fn a_broken_version_is_rejected_but_an_unsupported_one_is_not() {
    let mut broken = engine("codex");
    broken.broken = Some("0.1.2 corrupts sessions".into());
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), broken],
        &Ask::default(),
    );
    assert_eq!(
        why(&decision, "codex").why,
        RejectionWhy::CompatibilityBroken
    );
    assert!(
        why(&decision, "codex")
            .text
            .as_str()
            .contains("corrupts sessions")
    );
    // Only the manifest's `Broken` status builds a view with a reason (the plan's filter).
    let status = serde_json::from_value::<orbyn_contracts::EngineStatus>(json!({
        "id": "codex", "name": "Codex", "tier": "native", "capabilities": caps(),
        "installed": true, "path": "/bin/codex", "version": "0.1.0", "auth": "logged_in",
        "installer": "npm", "latest": null, "support": "below_minimum",
        "compatibility": { "status": "unsupported", "message": "old" },
        "update_command": null, "manual_hint": null, "sign_in_command": null,
        "updating": false, "problem": null
    }))
    .unwrap();
    assert_eq!(
        status.compatibility.as_ref().unwrap().status,
        CompatibilityStatus::Unsupported
    );
    assert!(
        crate::crew::EngineView::from_status(&status, false)
            .unwrap()
            .broken
            .is_none()
    );
}

#[test]
fn a_writing_brief_needs_managed_and_approvals_but_a_reading_one_does_not() {
    let mut loose = engine("opencode");
    loose.capabilities.managed = false;
    let mut mute = engine("codex");
    mute.capabilities.approvals = false;
    let engines = [engine("claude"), loose, mute];

    let writes = routed(&brief(json!({})), &engines, &Ask::default());
    assert_eq!(why(&writes, "opencode").why, RejectionWhy::NotManaged);
    assert_eq!(why(&writes, "codex").why, RejectionWhy::NoApprovals);

    let reads = routed(&reading(json!({})), &engines, &Ask::default());
    assert!(reads.rejected.is_empty(), "{:?}", reads.rejected);
    assert_eq!(reads.scores.len(), 3);
}

#[test]
fn a_write_grant_alone_counts_as_writing() {
    let mut loose = engine("opencode");
    loose.capabilities.managed = false;
    let sneaky = brief(json!({ "allowed_effects": ["read_repo"] }));
    let decision = routed(&sneaky, &[engine("claude"), loose], &Ask::default());
    assert_eq!(why(&decision, "opencode").why, RejectionWhy::NotManaged);
}

#[test]
fn needs_must_match_capabilities() {
    let mut no_mcp = engine("codex");
    no_mcp.capabilities.mcp = false;
    let decision = routed(
        &brief(json!({ "needs": ["mcp"] })),
        &[engine("claude"), no_mcp],
        &Ask::default(),
    );
    assert_eq!(why(&decision, "codex").why, RejectionWhy::MissingCapability);
    assert!(why(&decision, "codex").text.as_str().contains("mcp"));

    // `image` is a model input: only an engine with an image-capable model passes.
    let mut seeing = engine("codex");
    seeing.models.as_mut().unwrap()[1] = with_image(model("terra-1", "Terra", true));
    let decision = routed(
        &brief(json!({ "needs": ["image"] })),
        &[engine("claude"), seeing],
        &Ask::default(),
    );
    assert_eq!(decision.engine.as_str(), "codex");
    assert_eq!(decision.model.as_str(), "terra-1");
    assert_eq!(
        why(&decision, "claude").why,
        RejectionWhy::MissingCapability
    );
    assert!(why(&decision, "claude").text.as_str().contains("image"));
}

#[test]
fn a_need_the_router_does_not_know_rejects_every_engine_with_that_reason() {
    let refusal = refused(
        &brief(json!({ "needs": ["telepathy"] })),
        &[engine("claude"), engine("codex")],
        &Ask::default(),
    );
    assert_eq!(refusal.rejected.len(), 2);
    assert!(
        refusal
            .rejected
            .iter()
            .all(|r| r.why == RejectionWhy::MissingCapability
                && r.text.as_str().contains("telepathy"))
    );
}

#[test]
fn an_engine_with_no_usage_left_is_rejected() {
    let mut full = engine("codex");
    full.used_percent = Some(100);
    let mut nearly = engine("opencode");
    nearly.used_percent = Some(99);
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), full, nearly],
        &Ask::default(),
    );
    assert_eq!(why(&decision, "codex").why, RejectionWhy::QuotaExhausted);
    assert!(
        decision
            .scores
            .iter()
            .any(|s| s.engine.as_str() == "opencode")
    );
}

#[test]
fn engines_without_a_model_for_the_tier_are_rejected_by_tier() {
    let engines = [engine("claude"), engine("codex"), engine("opencode")];
    // A code review is lead tier. OpenCode has no mapping, so it is offered only as a builder.
    let review = routed(
        &reading(json!({ "kind": "review" })),
        &engines,
        &Ask::default(),
    );
    assert_eq!(review.role, RouteRole::CodeReview);
    assert_eq!(why(&review, "opencode").why, RejectionWhy::TierBelowLead);
    assert!(["claude-opus-4", "sol-1"].contains(&review.model.as_str()));

    // No Opus listed, so Claude cannot take a lead-tier role.
    let mut no_opus = engine("claude");
    no_opus.models.as_mut().unwrap().remove(0);
    let review = routed(
        &reading(json!({ "kind": "review" })),
        &[no_opus, engine("codex")],
        &Ask::default(),
    );
    assert_eq!(why(&review, "claude").why, RejectionWhy::TierBelowLead);
    assert_eq!(review.engine.as_str(), "codex");

    // A scout role on an unmapped engine has no scout model; a builder role uses its default.
    let scout = routed(
        &reading(json!({ "kind": "research" })),
        &engines,
        &Ask::default(),
    );
    assert_eq!(scout.role, RouteRole::Scout);
    assert_eq!(why(&scout, "opencode").why, RejectionWhy::MissingCapability);
    let build = routed(&brief(json!({})), &[engine("opencode")], &Ask::default());
    assert_eq!(build.model.as_str(), "mini-1");
}

#[test]
fn an_engine_with_no_models_listed_is_rejected() {
    let mut empty = engine("codex");
    empty.models = Some(Vec::new());
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), empty],
        &Ask::default(),
    );
    assert_eq!(why(&decision, "codex").why, RejectionWhy::MissingCapability);
    assert!(why(&decision, "codex").text.as_str().contains("no models"));
}

#[test]
fn every_engine_is_scored_or_rejected_with_a_reason_and_none_is_lost() {
    let mut out = engine("codex");
    out.auth = AuthState::LoggedOut;
    let mut gone = engine("opencode");
    gone.installed = false;
    let engines = [engine("claude"), out, gone, engine("zed")];
    let decision = routed(&brief(json!({})), &engines, &Ask::default());
    assert_eq!(
        decision.scores.len() + decision.rejected.len(),
        engines.len()
    );
    assert!(
        decision
            .rejected
            .iter()
            .all(|r| !r.text.as_str().is_empty())
    );
    for one in &engines {
        let scored = decision.scores.iter().any(|s| s.engine == one.id);
        let rejected = decision.rejected.iter().any(|r| r.engine == one.id);
        assert!(scored ^ rejected, "{} must be either", one.id);
    }
    // The decision names a reason for the choice and one for each score.
    assert!(
        decision
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "highest_score")
    );
    assert_eq!(
        decision
            .reasons
            .iter()
            .filter(|r| r.code.as_str().starts_with("score_"))
            .count(),
        decision.scores.len()
    );
}

#[test]
fn with_nothing_usable_the_refusal_lists_every_rejection() {
    let mut a = engine("claude");
    a.installed = false;
    let mut b = engine("codex");
    b.auth = AuthState::LoggedOut;
    let refusal = refused(&brief(json!({})), &[a, b], &Ask::default());
    assert!(refusal.requested.is_none());
    assert_eq!(refusal.rejected.len(), 2);
    assert!(refusal.message.contains("No engine"));
    let none = refused(&brief(json!({})), &[], &Ask::default());
    assert!(none.rejected.is_empty());
}

#[test]
fn tier_words_match_whole_words_only() {
    let mut odd = engine("codex");
    odd.models = Some(vec![
        model("console-1", "Console", true),
        model("solid-2", "Solid", false),
    ]);
    let refusal = refused(
        &reading(json!({ "kind": "review" })),
        &[odd],
        &Ask::default(),
    );
    assert_eq!(refusal.rejected[0].why, RejectionWhy::TierBelowLead);
    // A named model is a whole word too: `son` is not `sonnet`.
    let named =
        brief(json!({ "engine": { "id": "claude", "model": "son", "assigned_by": "person" } }));
    assert_eq!(
        refused(&named, &[engine("claude")], &Ask::default()).rejected[0].why,
        RejectionWhy::MissingCapability
    );
    let exact = brief(
        json!({ "engine": { "id": "claude", "model": "claude-sonnet-4", "assigned_by": "person" } }),
    );
    assert_eq!(
        routed(&exact, &[engine("claude")], &Ask::default())
            .model
            .as_str(),
        "claude-sonnet-4"
    );
}
