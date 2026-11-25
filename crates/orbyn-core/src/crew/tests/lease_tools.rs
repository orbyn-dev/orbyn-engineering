//! The member lease tools (issue #439): a worker asks for a lease and releases it through MCP, and
//! with the Crew Preview flag off neither tool is listed or callable. The write refusal at the gate is
//! unchanged and tested in `tests/autopilot.rs`.

use std::path::Path;
use std::sync::Arc;

use orbyn_contracts::{CrewPathPattern, CrewRole, ErrorCode, LeaseMode, LeaseState};
use serde_json::{Value, json};

use super::harness::{Turn, World, bind, brief, key, report, world};

const DOCS: &str = "docs/**";
/// Later than any clock this test runs on; whole seconds in UTC, as a lease expiry must be.
const FUTURE: &str = "2099-01-01T00:00:00Z";

fn docs_leases(w: &World, task: &str, state: LeaseState) -> Vec<orbyn_contracts::Lease> {
    w.core
        .store
        .crew_leases(task)
        .unwrap()
        .into_iter()
        .filter(|lease| {
            lease.state == state && lease.paths.iter().any(|path| path.as_str() == DOCS)
        })
        .collect()
}

async fn listed(w: &World) -> Vec<Value> {
    let reply = w
        .core
        .mcp
        .handle(
            &w.lead_token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await
        .unwrap()
        .unwrap();
    reply["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A worker's own MCP token, as its run would get.
fn worker_token(w: &World, task: &str) -> String {
    let thread = w.worker(task);
    bind(&w.core, &thread, "run-lease")
}

/// A worker asks for a read lease outside its brief's files, then releases it; the core stamps the
/// grant with the caller as holder and the core as author, and the release ends it for later reads.
#[tokio::test]
async fn a_worker_requests_and_releases_a_lease() {
    let script = Arc::new(|turn: usize, _: &Path| match turn {
        0 => Turn::Steps(vec![
            (
                crate::crew::LEASE_REQUEST_TOOL,
                json!({ "paths": [DOCS], "mode": "read", "expires_at": FUTURE }),
            ),
            (crate::crew::LEASE_RELEASE_TOOL, json!({ "paths": [DOCS] })),
            ("crew_report", report("done")),
        ]),
        _ => Turn::Hang,
    });
    let w = world(true, script).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    let member = w.core.store.crew_run(&id).unwrap().unwrap().member;
    let crew = w.task(&id).crew_id;

    w.until("the requested lease is granted and released", |w| {
        !docs_leases(w, &id, LeaseState::Granted).is_empty()
            && !docs_leases(w, &id, LeaseState::Released).is_empty()
    })
    .await;

    let granted = docs_leases(&w, &id, LeaseState::Granted);
    let granted = granted.first().expect("the lease the worker asked for");
    assert_eq!(granted.holder.as_str(), member.as_str());
    assert_eq!(granted.mode, LeaseMode::Read);
    assert_eq!(granted.expires_at.as_str(), FUTURE);
    assert_eq!(granted.author.role, CrewRole::Core);
    assert_eq!(granted.author.engine.as_str(), "orbyn");

    let released = docs_leases(&w, &id, LeaseState::Released);
    let released = released.first().expect("the lease the worker released");
    assert_eq!(
        released.paths, granted.paths,
        "the released lease is the one granted"
    );
    assert_eq!(released.mode, LeaseMode::Read);
    assert!(released.reason.is_some(), "a release says why");

    // The release ended it: the path is covered by no lease now. The brief's own write lease is
    // still held, so the refusal is the one for a path outside every lease, not a missing task.
    let pattern = CrewPathPattern::new("docs/guide.md").unwrap();
    assert_eq!(
        w.core
            .crew_check_lease(&id, &member, &pattern, LeaseMode::Read)
            .unwrap_err()
            .code,
        ErrorCode::NotAuthorized
    );
    let live = w
        .core
        .live_crew_leases(&crew, crate::util::unix_now())
        .unwrap();
    assert!(
        live.iter()
            .all(|(_, lease)| lease.paths.iter().all(|path| path.as_str() != DOCS)),
        "{live:?}"
    );
}

/// Off, neither tool is listed, and a call is the MCP layer's unknown tool.
#[tokio::test]
async fn the_lease_tools_hide_with_the_preview_flag_off() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let specs = listed(&w).await;
    for tool in crate::crew::LEASE_TOOLS {
        assert!(
            !specs.iter().any(|spec| spec["name"] == tool),
            "{tool} is listed"
        );
        let (answer, error) = w.call(&w.lead_token, tool, json!({})).await;
        assert!(error, "{answer}");
        assert_eq!(answer["error"]["message"], format!("Unknown tool {tool}."));
    }
}

/// On, both tools are listed with their arguments; the core hides every Crew tool behind the flag.
#[tokio::test]
async fn the_lease_tools_are_listed_behind_the_preview_flag() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let specs = listed(&w).await;
    for tool in crate::crew::LEASE_TOOLS {
        assert!(
            specs.iter().any(|spec| spec["name"] == tool),
            "{tool} is not listed"
        );
    }
    let request = specs
        .iter()
        .find(|spec| spec["name"].as_str() == Some(crate::crew::LEASE_REQUEST_TOOL))
        .expect("crew_lease_request is listed");
    assert_eq!(
        request["inputSchema"]["required"],
        json!(["paths", "mode", "expires_at"])
    );
    let release = specs
        .iter()
        .find(|spec| spec["name"].as_str() == Some(crate::crew::LEASE_RELEASE_TOOL))
        .expect("crew_lease_release is listed");
    assert_eq!(release["inputSchema"]["required"], json!(["paths"]));
}

/// The request's refusals: a lapsed time, a forged field, a write outside the brief, a protected
/// path, and releasing a lease that is not held.
#[tokio::test]
async fn a_request_refuses_what_the_core_may_not_grant() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    let token = worker_token(&w, &id);

    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": ["src/**"], "mode": "write", "expires_at": "2026-11-12T08:00:00Z" }),
        )
        .await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "lease_expired");

    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": ["src/**"], "mode": "write", "expires_at": FUTURE, "holder": "worker-02" }),
        )
        .await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "invalid_object");

    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": ["docs/**"], "mode": "write", "expires_at": FUTURE }),
        )
        .await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "not_authorized");

    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": ["tests/check.rs"], "mode": "write", "expires_at": FUTURE }),
        )
        .await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "protected_path_conflict");

    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_RELEASE_TOOL,
            json!({ "paths": [DOCS] }),
        )
        .await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "lease_not_held");
}
