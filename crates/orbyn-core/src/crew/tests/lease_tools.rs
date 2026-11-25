//! The member lease tools (issue #439): a worker asks for a lease and releases it through MCP, and
//! with the Crew Preview flag off neither tool is listed or callable. The write refusal at the gate is
//! unchanged and tested in `tests/autopilot.rs`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{
    CrewPathPattern, CrewRole, ErrorCode, LeaseMode, LeaseState, ResultState, StopReason,
};
use serde_json::{Value, json};

use super::harness::{Turn, World, bind, brief, key, report, world, world_with};

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

/// H10: a request that reaches another member's lease is refused, and the lead is told. The duo's
/// two briefs write `src/**` and `docs/**`, so the first member's read of `docs/**` reaches the
/// second's write lease although the first has asked for nothing yet.
#[tokio::test]
async fn a_request_refuses_a_path_another_member_holds() {
    let d = super::bus_support::duo().await;
    let (answer, refused) = d
        .call(
            &d.ta,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": [DOCS], "mode": "read", "expires_at": FUTURE }),
        )
        .await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "lease_conflict");
    let notices = d.lead_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.contains("LeaseConflict")),
        "{notices:?}"
    );
}

/// H10: the live-lease check and the grant are one step. While one grant holds the lock, a
/// member's request and a start's grants both wait: each would pass its check against the store as
/// it stands, and both would then be stored.
#[tokio::test]
async fn a_grant_waits_for_the_one_in_flight() {
    fn engine(id: &str) -> Value {
        json!({ "id": id, "assigned_by": "person" })
    }
    let w = world_with(
        true,
        Arc::new(|_, _| Turn::Hang),
        &[("alpha", false), ("beta", false)],
    )
    .await;
    let (first, error) = w
        .start(brief(json!({ "engine": engine("alpha") })), key())
        .await;
    assert!(!error, "{first}");
    let first_id = first["task"].as_str().unwrap().to_owned();
    let token = worker_token(&w, &first_id);

    let held = w.core.crew_lease_grants.lock().await;
    // Neither reaches a live lease: the read names no write lease, and the start writes a path no
    // other brief holds.
    let mut request = Box::pin(w.call(
        &token,
        crate::crew::LEASE_REQUEST_TOOL,
        json!({ "paths": [DOCS], "mode": "read", "expires_at": FUTURE }),
    ));
    let mut start = Box::pin(w.start(
        brief(json!({
            "engine": engine("beta"),
            "outcome": "Document it.",
            "files": [{ "path": "web/**", "mode": "write" }],
        })),
        key(),
    ));
    tokio::select! {
        biased;
        answer = &mut request => panic!("the request ran while a grant held the lock: {answer:?}"),
        answer = &mut start => panic!("a start ran while a grant held the lock: {answer:?}"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }
    drop(held);
    let (answer, refused) = request.await;
    assert!(!refused, "{answer}");
    assert_eq!(answer["state"], "granted");
    let (second, error) = start.await;
    assert!(!error, "{second}");
    let second_id = second["task"].as_str().unwrap().to_owned();
    assert!(
        w.core
            .store
            .crew_leases(&second_id)
            .unwrap()
            .iter()
            .any(|lease| {
                lease.state == LeaseState::Granted
                    && lease.paths.iter().any(|path| path.as_str() == "web/**")
            }),
        "the start's grant was stored"
    );
}

/// A lease expiry for later, as the request tool takes it: whole seconds in UTC.
fn expiry_in(seconds: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(crate::util::unix_now() + seconds)
        .unwrap()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

/// H10: the request reads the clock again once it has the grant lock, so a request whose expiry
/// passed while it waited is refused, not stored already lapsed.
#[tokio::test]
async fn a_request_rechecks_its_expiry_after_the_wait() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    let token = worker_token(&w, &id);

    let held = w.core.crew_lease_grants.lock().await;
    let expires_at = expiry_in(2);
    let mut request = Box::pin(w.call(
        &token,
        crate::crew::LEASE_REQUEST_TOOL,
        json!({ "paths": [DOCS], "mode": "read", "expires_at": expires_at }),
    ));
    tokio::select! {
        biased;
        answer = &mut request => panic!("the request ran while a grant held the lock: {answer:?}"),
        // Past the expiry the request carried in: it must read the clock again before it stores.
        _ = tokio::time::sleep(Duration::from_millis(2_200)) => {}
    }
    drop(held);
    let (answer, refused) = request.await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "lease_expired");
    assert!(
        docs_leases(&w, &id, LeaseState::Granted).is_empty(),
        "a lapsed request stored a grant"
    );
}

/// H10: a release takes the same grant lock before it reads the live leases, so a grant that lands
/// between its read and the objects it writes cannot be hidden by them.
#[tokio::test]
async fn a_release_waits_for_the_grant_in_flight() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    let token = worker_token(&w, &id);
    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": [DOCS], "mode": "read", "expires_at": FUTURE }),
        )
        .await;
    assert!(!refused, "{answer}");

    let held = w.core.crew_lease_grants.lock().await;
    let mut release = Box::pin(w.call(
        &token,
        crate::crew::LEASE_RELEASE_TOOL,
        json!({ "paths": [DOCS] }),
    ));
    tokio::select! {
        biased;
        answer = &mut release => panic!("the release ran while a grant held the lock: {answer:?}"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }
    drop(held);
    let (answer, refused) = release.await;
    assert!(!refused, "{answer}");
    assert!(
        !docs_leases(&w, &id, LeaseState::Released).is_empty(),
        "the release did not end the lease"
    );
}

/// H10: a read request and a write start that name the same path race under one lock: exactly one
/// takes the path and the other is refused with `lease_conflict`. The test above shows both wait;
/// this one shows the guarantee they wait for.
#[tokio::test]
async fn a_request_and_a_start_race_for_the_same_path() {
    fn engine(id: &str) -> Value {
        json!({ "id": id, "assigned_by": "person" })
    }
    let w = world_with(
        true,
        Arc::new(|_, _| Turn::Hang),
        &[("alpha", false), ("beta", false)],
    )
    .await;
    let (first, error) = w
        .start(brief(json!({ "engine": engine("alpha") })), key())
        .await;
    assert!(!error, "{first}");
    let first_id = first["task"].as_str().unwrap().to_owned();
    let crew = w.task(&first_id).crew_id;
    let token = worker_token(&w, &first_id);

    let held = w.core.crew_lease_grants.lock().await;
    let mut request = Box::pin(w.call(
        &token,
        crate::crew::LEASE_REQUEST_TOOL,
        json!({ "paths": [DOCS], "mode": "read", "expires_at": FUTURE }),
    ));
    let mut start = Box::pin(w.start(
        brief(json!({
            "engine": engine("beta"),
            "outcome": "Document it.",
            "files": [{ "path": DOCS, "mode": "write" }],
        })),
        key(),
    ));
    tokio::select! {
        biased;
        answer = &mut request => panic!("the request ran while a grant held the lock: {answer:?}"),
        answer = &mut start => panic!("a start ran while a grant held the lock: {answer:?}"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }
    drop(held);
    let (answer, request_refused) = request.await;
    let (second, start_refused) = start.await;
    assert!(
        request_refused ^ start_refused,
        "exactly one takes the path: {answer} / {second}"
    );
    let refused = if request_refused { &answer } else { &second };
    assert_eq!(refused["code"], "lease_conflict", "{refused}");
    let live = w
        .core
        .live_crew_leases(&crew, crate::util::unix_now())
        .unwrap();
    assert_eq!(
        live.iter()
            .filter(|(_, lease)| lease.paths.iter().any(|path| path.as_str() == DOCS))
            .count(),
        1,
        "exactly one grant on {DOCS} is live: {live:?}"
    );
}

/// H10: a request that waited for a grant in flight stores nothing after the task ended. The task
/// state is read again in the grant's own transaction, so the run-end cleanup that reads the live
/// leases after the move to a final state cannot be overtaken by a waiting grant.
#[tokio::test]
async fn a_request_waiting_for_a_grant_stores_nothing_after_the_run_end() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    let crew = w.task(&id).crew_id;
    let token = worker_token(&w, &id);

    // The request passes the run's live check, then waits here for a grant in flight.
    let held = w.core.crew_lease_grants.lock().await;
    let mut request = Box::pin(w.call(
        &token,
        crate::crew::LEASE_REQUEST_TOOL,
        json!({ "paths": [DOCS], "mode": "read", "expires_at": FUTURE }),
    ));
    tokio::select! {
        biased;
        answer = &mut request => panic!("the request ran while a grant held the lock: {answer:?}"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }
    // The task ends while the request waits: it goes final, then its run-end cleanup reads the
    // live leases and writes the brief's own grant released.
    let stopped = w
        .core
        .store
        .move_crew_task(&id, "stopped", Some("budget"))
        .unwrap();
    w.core.end_crew_task(
        &stopped,
        ResultState::Stopped,
        Some(StopReason::Budget),
        "The task ran past its time budget.",
        None,
    );
    drop(held);
    let (answer, refused) = request.await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], "run_ended");
    assert!(
        docs_leases(&w, &id, LeaseState::Granted).is_empty(),
        "a waiting request stored a grant after the run-end cleanup"
    );
    assert!(
        w.core
            .live_crew_leases(&crew, crate::util::unix_now())
            .unwrap()
            .is_empty(),
        "a grant survived the run end"
    );
}

/// H10: a member's request for a narrower write lease, and its release, leave the brief's own
/// write grant live for the brief's other files: the two grants are separate lineages (one per
/// path set), so neither the request nor the release replaces the brief's grant.
#[tokio::test]
async fn a_path_scoped_lease_leaves_the_briefs_write_grant_live() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    let token = worker_token(&w, &id);
    let member = w.core.store.crew_run(&id).unwrap().unwrap().member;
    let pattern = |path: &str| CrewPathPattern::new(path).unwrap();

    // The brief grants src/**; the worker asks for one file inside it, then releases it.
    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_REQUEST_TOOL,
            json!({ "paths": ["src/one.rs"], "mode": "write", "expires_at": FUTURE }),
        )
        .await;
    assert!(!refused, "{answer}");
    assert_eq!(answer["state"], "granted");
    assert!(
        w.core
            .crew_check_lease(&id, &member, &pattern("src/two.rs"), LeaseMode::Write)
            .is_ok(),
        "the path-scoped request hid the brief's write grant"
    );

    let (answer, refused) = w
        .call(
            &token,
            crate::crew::LEASE_RELEASE_TOOL,
            json!({ "paths": ["src/one.rs"] }),
        )
        .await;
    assert!(!refused, "{answer}");
    assert!(
        w.core
            .crew_check_lease(&id, &member, &pattern("src/two.rs"), LeaseMode::Write)
            .is_ok(),
        "the release dropped the brief's write grant"
    );
    assert!(
        w.core
            .crew_check_lease(&id, &member, &pattern("src/one.rs"), LeaseMode::Write)
            .is_ok(),
        "the released path is still inside the brief's grant"
    );

    // The brief's grant stays granted for its files, and the scoped lease is the one released.
    let leases = w.core.store.crew_leases(&id).unwrap();
    assert!(
        leases.iter().any(|lease| lease.state == LeaseState::Granted
            && lease.paths.iter().any(|path| path.as_str() == "src/**")),
        "{leases:?}"
    );
    assert!(
        leases
            .iter()
            .any(|lease| lease.state == LeaseState::Released
                && lease.paths.iter().any(|path| path.as_str() == "src/one.rs")),
        "{leases:?}"
    );
}
