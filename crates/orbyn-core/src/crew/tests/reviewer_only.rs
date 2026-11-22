//! An engine whose writing work is not confined yet (Cursor on the host) is `reviewer_only`: the router never picks it as a
//! writer, and a reviewer lane may still use it (docs/cursor-thread-contract.md, "Crew").

use orbyn_contracts::{EngineCrewRole, RejectionWhy, RouteRole};
use serde_json::json;

use super::fake::*;

const REASON: &str = "Not yet confined for writing work";

fn reviewer_only(id: &str) -> crate::crew::EngineView {
    let mut view = engine(id);
    view.crew_role = EngineCrewRole::ReviewerOnly;
    view.crew_role_reason = Some(REASON.to_owned());
    view
}

#[test]
fn a_reviewer_only_engine_is_never_picked_as_a_writer() {
    let ask = Ask {
        default_engine: Some("codex"),
        ..Ask::default()
    };
    let decision = routed(
        &brief(json!({})),
        &[reviewer_only("codex"), engine("claude")],
        &ask,
    );
    assert_eq!(decision.engine.as_str(), "claude");
    let why = decision
        .rejected
        .iter()
        .find(|r| r.engine.as_str() == "codex")
        .expect("the refusal is shown");
    assert_eq!(why.why, RejectionWhy::MissingCapability);
    assert_eq!(
        why.text.as_str(),
        "Codex is reviewer only: Not yet confined for writing work."
    );
    verified(&decision);
}

#[test]
fn a_machine_with_only_a_reviewer_only_engine_refuses_the_writer() {
    let refusal = refused(
        &brief(json!({})),
        &[reviewer_only("claude")],
        &Ask::default(),
    );
    assert!(
        refusal.rejected[0].text.as_str().contains("reviewer only"),
        "{}",
        refusal.rejected[0].text.as_str()
    );
}

#[test]
fn a_pinned_reviewer_only_engine_is_refused_for_writing_and_never_swapped() {
    let pinned = brief(json!({
        "engine": { "id": "codex", "assigned_by": "person", "tag": "@codex" }
    }));
    let refusal = refused(
        &pinned,
        &[reviewer_only("codex"), engine("claude")],
        &Ask::default(),
    );
    assert!(
        refusal.message.contains("reviewer only"),
        "{}",
        refusal.message
    );
}

#[test]
fn a_reviewer_only_engine_may_review() {
    let ask = Ask {
        role: Some(RouteRole::Review),
        lane: Some("correctness"),
        author: Some("codex"),
        ..Ask::default()
    };
    let reading = reading(json!({ "kind": "review" }));
    let decision = routed(&reading, &[reviewer_only("claude"), engine("codex")], &ask);
    assert_eq!(
        decision.engine.as_str(),
        "claude",
        "the reviewer-only engine reviews another engine's work"
    );
}

#[test]
fn an_engine_with_no_role_limit_still_writes() {
    let decision = routed(&brief(json!({})), &[engine("claude")], &Ask::default());
    assert_eq!(decision.engine.as_str(), "claude");
}
