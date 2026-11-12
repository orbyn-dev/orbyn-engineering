//! The Providers list and the router's hard filters read one set of facts. Every status in a
//! matrix is run through both, and the label the list gives must be the refusal the router
//! gives: if either side grows a rule of its own, a case here fails.

use orbyn_contracts::{
    EngineStatus, ProviderEntry, ProviderGroup, ProviderLabel, RejectionWhy, RouteDecision,
};
use serde_json::json;

use super::fake::*;
use crate::agents::provider_list::list;
use crate::crew::EngineView;

fn status(
    installed: bool,
    auth: &str,
    compatibility: Option<&str>,
    managed: bool,
    approvals: bool,
) -> EngineStatus {
    let mut capabilities = caps();
    capabilities.managed = managed;
    capabilities.approvals = approvals;
    serde_json::from_value(json!({
        "id": "tested", "name": "Tested", "tier": "acp", "capabilities": capabilities,
        "installed": installed, "path": null, "version": null, "auth": auth,
        "installer": "unknown", "latest": null, "support": "unknown",
        "compatibility": compatibility.map(|state| json!({ "status": state, "message": "note" })),
        "update_command": null, "manual_hint": null, "sign_in_command": null,
        "updating": false, "problem": null
    }))
    .unwrap()
}

fn rejected(decision: &RouteDecision) -> Option<RejectionWhy> {
    decision
        .rejected
        .iter()
        .find(|rejection| rejection.engine.as_str() == "tested")
        .map(|rejection| rejection.why)
}

fn route_for(status: &EngineStatus, writes: bool) -> Option<RejectionWhy> {
    let mut view = EngineView::from_status(status, false).unwrap();
    view.models = engine("opencode").models;
    let brief = if writes {
        brief(json!({}))
    } else {
        reading(json!({}))
    };
    rejected(&routed(&brief, &[engine("claude"), view], &Ask::default()))
}

fn entry_of(status: &EngineStatus) -> Option<ProviderEntry> {
    list(std::slice::from_ref(status), &[])
        .providers
        .into_iter()
        .find(|entry| entry.id == "tested")
}

#[test]
fn the_list_and_the_router_agree_on_every_combination_of_facts() {
    let mut cases = 0;
    for installed in [true, false] {
        for auth in ["logged_in", "logged_out", "unknown", "not_applicable"] {
            for compatibility in [None, Some("broken"), Some("unsupported"), Some("graceful")] {
                for (managed, approvals) in [(true, true), (false, true), (true, false)] {
                    let status = status(installed, auth, compatibility, managed, approvals);
                    let (writes, reads) = (route_for(&status, true), route_for(&status, false));
                    let entry = entry_of(&status);
                    let signed_in = matches!(auth, "logged_in" | "not_applicable");
                    let at = format!("{installed} {auth} {compatibility:?} {managed} {approvals}");

                    // Not found: no row, and the router says not installed.
                    assert_eq!(entry.is_none(), !installed, "{at}");
                    assert_eq!(
                        reads == Some(RejectionWhy::NotInstalled),
                        !installed,
                        "{at}"
                    );
                    let Some(entry) = entry else { continue };
                    let has = |label| entry.labels.contains(&label);

                    // Can't start is a known-broken version, which the router refuses first
                    // once the engine is signed in. A limited one is never refused.
                    assert_eq!(
                        has(ProviderLabel::CantStart),
                        compatibility == Some("broken")
                    );
                    assert_eq!(
                        entry.group == ProviderGroup::CantStart,
                        has(ProviderLabel::CantStart),
                        "{at}"
                    );
                    if signed_in {
                        assert_eq!(
                            reads == Some(RejectionWhy::CompatibilityBroken),
                            has(ProviderLabel::CantStart),
                            "{at}"
                        );
                    }
                    assert_eq!(
                        has(ProviderLabel::LimitedSupport),
                        matches!(compatibility, Some("unsupported" | "graceful")),
                        "{at}"
                    );

                    // Unmanaged is exactly what keeps a writing brief out, and a reading
                    // brief is never kept out by it.
                    assert_eq!(
                        has(ProviderLabel::Unmanaged),
                        !(managed && approvals),
                        "{at}"
                    );
                    if signed_in && compatibility != Some("broken") {
                        let refused = matches!(
                            writes,
                            Some(RejectionWhy::NotManaged | RejectionWhy::NoApprovals)
                        );
                        assert_eq!(refused, has(ProviderLabel::Unmanaged), "{at}");
                        assert_eq!(reads, None, "{at}");
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 4 * 4 * 3, "every installed combination was checked");
}
