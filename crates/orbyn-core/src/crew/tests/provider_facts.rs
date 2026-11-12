//! The Providers list and the router's hard filters read one set of facts. Every status in a
//! matrix is run through both, and the label the list gives must be the refusal the router
//! gives: if either side grows a rule of its own, a case here fails.

use orbyn_contracts::{
    EngineStatus, ProviderEntry, ProviderGroup, ProviderLabel, RejectionWhy, RouteDecision,
};
use serde_json::json;

use super::fake::*;
use crate::agents::provider_list::list;

fn only_list(status: &EngineStatus) -> Vec<ProviderEntry> {
    list(std::slice::from_ref(status), &[], &[]).providers
}
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
    only_list(status)
        .into_iter()
        .find(|entry| entry.id == "tested")
}

#[test]
fn the_list_and_the_router_agree_on_every_combination_of_facts() {
    let mut cases = 0;
    for installed in [true, false] {
        for auth in ["logged_in", "logged_out", "unknown", "not_applicable"] {
            for compatibility in [None, Some("broken"), Some("unsupported"), Some("graceful")] {
                for (managed, approvals) in
                    [(true, true), (false, true), (true, false), (false, false)]
                {
                    let status = status(installed, auth, compatibility, managed, approvals);
                    let (writes, reads) = (route_for(&status, true), route_for(&status, false));
                    let entry = entry_of(&status);
                    let signed_in = matches!(auth, "logged_in" | "not_applicable");
                    let at = format!("{installed} {auth} {compatibility:?} {managed} {approvals}");

                    // Not found: the row stays, and the router says not installed.
                    let entry = entry.expect("every registered engine has a row");
                    let has = |label| entry.labels.contains(&label);
                    assert_eq!(has(ProviderLabel::NotFound), !installed, "{at}");
                    assert_eq!(entry.group == ProviderGroup::NotFound, !installed, "{at}");
                    assert_eq!(
                        reads == Some(RejectionWhy::NotInstalled),
                        !installed,
                        "{at}"
                    );
                    if !installed {
                        continue;
                    }

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
    assert_eq!(cases, 4 * 4 * 4, "every installed combination was checked");
}

/// The router's reasons, pinned as text and built without `ProviderFacts`: a change to the
/// shared facts cannot quietly change what a person reads.
#[test]
fn the_router_gives_these_reasons_for_these_engines() {
    fn view(edit: impl FnOnce(&mut EngineView)) -> EngineView {
        let mut view = engine("opencode");
        view.name = "Tool".to_owned();
        view.id = orbyn_contracts::CrewToken::new("tool").unwrap();
        edit(&mut view);
        view
    }
    let cases: Vec<(EngineView, bool, RejectionWhy, &str)> = vec![
        (
            view(|v| v.installed = false),
            false,
            RejectionWhy::NotInstalled,
            "Tool isn't installed.",
        ),
        (
            view(|v| v.auth = orbyn_contracts::AuthState::LoggedOut),
            false,
            RejectionWhy::NotSignedIn,
            "Tool isn't signed in.",
        ),
        (
            view(|v| v.auth = orbyn_contracts::AuthState::Unknown),
            false,
            RejectionWhy::NotSignedIn,
            "Tool's sign-in couldn't be confirmed.",
        ),
        (
            view(|v| v.broken = Some("0.1.2 corrupts sessions".into())),
            false,
            RejectionWhy::CompatibilityBroken,
            "Tool's installed version is known to be broken: 0.1.2 corrupts sessions",
        ),
        (
            view(|v| v.capabilities.managed = false),
            true,
            RejectionWhy::NotManaged,
            "Tool can't be gated by Orbyn, so it can't write.",
        ),
        (
            view(|v| v.capabilities.approvals = false),
            true,
            RejectionWhy::NoApprovals,
            "Tool can't ask for approval, so it can't write.",
        ),
        // Both missing: not being managed is the reason given.
        (
            view(|v| {
                v.capabilities.managed = false;
                v.capabilities.approvals = false;
            }),
            true,
            RejectionWhy::NotManaged,
            "Tool can't be gated by Orbyn, so it can't write.",
        ),
    ];
    for (tool, writes, why, text) in cases {
        let brief = if writes {
            brief(json!({}))
        } else {
            reading(json!({}))
        };
        let decision = routed(&brief, &[engine("claude"), tool], &Ask::default());
        let rejection = decision
            .rejected
            .iter()
            .find(|r| r.engine.as_str() == "tool")
            .expect("rejected");
        assert_eq!(rejection.why, why, "{text}");
        assert_eq!(rejection.text.as_str(), text);
    }
}
