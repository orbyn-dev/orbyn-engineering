//! `POST /api/crew/autopilot/merge-queue/{id}/resolve` over HTTP: a person route an agent cannot reach, the Preview gate, the key,
//! a stale view and a person's reject. The rules and the lane are tested in `orbyn-core` (`crew/tests/forge_merge.rs`).

use orbyn_core::crew::changes::{STATE_CHANGING, changes_crew_state};
use orbyn_core::crew::fixtures;
use serde_json::{Value, json};

use super::crew_tasks_tests::{Served, seed_task, serve_with};
use crate::http::access::Access;

const PATH: &str = "/api/crew/autopilot/merge-queue/{id}/resolve";

async fn post(app: &Served, id: &str, body: Value, token: bool, key: Option<&str>) -> (u16, Value) {
    let mut request = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}{}",
            app.port,
            PATH.replace("{id}", id)
        ))
        .json(&body);
    if token {
        request = request.bearer_auth("token");
    }
    if let Some(key) = key {
        request = request.header("x-orbyn-person", key);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn keyed(name: &str, preview: bool, remote: bool) -> impl Future<Output = Served> {
    serve_with(
        name,
        preview,
        remote,
        Access::new("token", vec![], None).with_person_key(Some("the-key")),
    )
}

#[tokio::test]
async fn the_resolve_route_is_a_person_route_an_agent_cannot_reach_and_needs_the_key() {
    assert!(changes_crew_state(&PATH.replace("{id}", "task-1")));
    assert!(STATE_CHANGING.contains(&("POST", PATH)));
    assert!(!changes_crew_state("/api/crew/autopilot/merge-queue"));

    let app = keyed("forge-resolve-key", true, false).await;
    let (task, _) = seed_task(&app.core, "forge");
    let revision = fixtures::forge_needs_person(&app.core, &task, "/repo", "the target base moved");
    let body = json!({ "action": "reject", "seen_revision": revision });
    let (status, _) = post(&app, &task, body.clone(), false, Some("the-key")).await;
    assert_eq!(status, 401, "the service token comes first");
    for key in [None, Some("wrong")] {
        let (status, reply) = post(&app, &task, body.clone(), true, key).await;
        assert_eq!((status, &reply["code"]), (403, &json!("person_required")));
    }
    assert!(fixtures::audit_actions(&app.core, "crew.merge.forge").is_empty());
    let queue = app.get("/api/crew/autopilot/merge-queue").await.1;
    assert_eq!(
        queue["entries"][0]["state"], "blocked",
        "every refused call left it as it was"
    );
    assert_eq!(queue["entries"][0]["forge"]["phase"], "needs_person");
    assert_eq!(queue["entries"][0]["forge"]["revision"], revision);
}

#[tokio::test]
async fn a_stale_view_is_409_and_a_person_rejects_with_the_key() {
    let app = keyed("forge-resolve-act", true, false).await;
    let (task, _) = seed_task(&app.core, "forge");
    let revision = fixtures::forge_needs_person(&app.core, &task, "/repo", "the target base moved");
    let (status, reply) = post(
        &app,
        &task,
        json!({ "action": "reject", "seen_revision": revision + 1 }),
        true,
        Some("the-key"),
    )
    .await;
    assert_eq!((status, &reply["code"]), (409, &json!("stale")), "{reply}");
    for bad in [
        json!({ "action": "merge", "seen_revision": revision }),
        json!({ "action": "reject" }),
        json!({}),
    ] {
        let (status, _) = post(&app, &task, bad, true, Some("the-key")).await;
        assert_eq!(status, 400);
    }
    assert!(fixtures::audit_actions(&app.core, "crew.merge.forge").is_empty());
    let (status, reply) = post(
        &app,
        &task,
        json!({ "action": "reject", "seen_revision": revision }),
        true,
        Some("the-key"),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(
        (reply["outcome"].clone(), reply["phase"].clone()),
        (json!("rejected"), json!("rejected"))
    );
    assert_eq!(
        fixtures::audit_actions(&app.core, "crew.merge.forge"),
        ["crew.merge.forge.resolve"]
    );
    let queue = app.get("/api/crew/autopilot/merge-queue").await.1;
    assert_eq!(
        queue["entries"],
        json!([]),
        "a rejected entry is out of the queue"
    );
    let (status, reply) = post(
        &app,
        &task,
        json!({ "action": "reject", "seen_revision": revision + 1 }),
        true,
        Some("the-key"),
    )
    .await;
    assert_eq!(
        (status, &reply["code"]),
        (409, &json!("stale")),
        "no longer waiting"
    );
}

#[tokio::test]
async fn the_route_is_local_only_and_absent_with_the_flag_off() {
    let remote = keyed("forge-resolve-remote", true, true).await;
    let (status, reply) = post(
        &remote,
        "task-1",
        json!({ "action": "reject", "seen_revision": 1 }),
        true,
        Some("the-key"),
    )
    .await;
    assert_eq!((status, &reply["code"]), (403, &json!("local_only")));
    let off = keyed("forge-resolve-off", false, false).await;
    let (status, reply) = post(
        &off,
        "task-1",
        json!({ "action": "reject", "seen_revision": 1 }),
        true,
        Some("the-key"),
    )
    .await;
    assert_eq!((status, &reply["code"]), (404, &json!("not_found")));
}
