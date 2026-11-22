//! An engine Orbyn cannot ask before it acts is never a Crew writer, by what the engine is (`managed: false`), not by name:
//! the role answer says reviewer only with a reason, the router refuses a writing pick with the same reason (a pinned
//! pick is refused and never swapped, which is also how a Crew saved with such a worker is refused at start), and a
//! reviewer lane may still use it.

use orbyn_contracts::{EngineCrewRole, RejectionWhy};
use serde_json::json;

use super::fake::*;
use crate::agents::driver::EngineDriver;
use crate::agents::driver::acp::AcpDriver;
use crate::agents::engines::known::{KNOWN_ENGINES, known_engine};
use crate::agents::engines::notice::{UNMANAGED_REASON, crew_role};

/// The engine as the real known-engine driver describes it, with the role the status gives it.
fn real_view(id: &str) -> crate::crew::EngineView {
    let known = known_engine(id).expect("a known engine");
    let capabilities = AcpDriver::known(known.as_custom(), known)
        .descriptor()
        .capabilities;
    let (role, reason) = crew_role(
        &capabilities.permission_modes,
        capabilities.managed && capabilities.approvals,
    );
    let mut view = engine(id);
    view.capabilities = capabilities;
    view.crew_role = role;
    view.crew_role_reason = reason;
    view
}

const UNMANAGED: [&str; 7] = [
    "kiro",
    "droid",
    "glm",
    "mistral-vibe",
    "omp",
    "deepseek",
    "fx",
];

#[test]
fn every_known_engine_that_cannot_ask_is_in_the_list_below_and_reviewer_only() {
    let silent: Vec<_> = KNOWN_ENGINES
        .iter()
        .filter(|e| !e.asks)
        .map(|e| e.id)
        .collect();
    for id in &silent {
        assert!(UNMANAGED.contains(id), "{id} is new: add it to the list");
    }
    for id in UNMANAGED {
        let view = real_view(id);
        assert!(!view.capabilities.managed, "{id}");
        assert_eq!(view.crew_role, EngineCrewRole::ReviewerOnly, "{id}");
        assert_eq!(
            view.crew_role_reason.as_deref(),
            Some(UNMANAGED_REASON),
            "{id}"
        );
    }
}

#[test]
fn a_worker_pick_of_such_an_engine_is_refused_with_the_reason_and_never_swapped() {
    for id in UNMANAGED {
        let pinned = brief(json!({
            "engine": { "id": id, "assigned_by": "person", "tag": format!("@{id}") }
        }));
        let refusal = refused(&pinned, &[real_view(id), engine("claude")], &Ask::default());
        assert!(
            refusal.message.contains("can't be gated by Orbyn")
                || refusal.message.contains("reviewer only"),
            "{id}: {}",
            refusal.message
        );
        assert_eq!(refusal.rejected[0].why, RejectionWhy::NotManaged, "{id}");
    }
}

#[test]
fn an_unpinned_write_never_lands_on_such_an_engine_even_as_the_default() {
    for id in UNMANAGED {
        let ask = Ask {
            default_engine: Some(id),
            ..Ask::default()
        };
        let decision = routed(&brief(json!({})), &[real_view(id), engine("claude")], &ask);
        assert_eq!(decision.engine.as_str(), "claude", "{id}");
    }
}
