//! Applying an execution profile over HTTP against the real service and the fake engine, so success does not depend on which
//! engines are installed here: a built-in applies with no acknowledgement, a grant-raising profile asks first, and an engine
//! switch is refused while a run is live.

mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};

async fn put(service: &Service, path: &str, body: Value) -> (u16, Value) {
    let mut request = service
        .client
        .put(format!("{}{path}", service.url))
        .bearer_auth(&service.token)
        .json(&body);
    if let Some(key) = &service.person_key {
        request = request.header("x-orbyn-person", key);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// The fake Claude lists no models, so the tier has nothing to resolve to. A custom model named for the builder tier gives it one.
async fn give_claude_a_builder_model(service: &Service) {
    let (status, body) = put(
        service,
        "/api/preferences",
        json!({ "values": { "custom_models": [{ "engine": "claude", "id": "sonnet", "label": "Sonnet" }] } }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
}

fn draft(name: &str, engine: Value, mode: &str, autopilot: &str) -> Value {
    json!({ "name": name, "description": "", "engine": engine, "model_tier": "builder", "permission_mode": mode, "autopilot_mode": autopilot })
}

async fn applied_rows(service: &Service, thread: &str) -> usize {
    service
        .audit(thread)
        .await
        .iter()
        .filter(|row| row["action"] == "profile.applied")
        .count()
}

#[tokio::test]
async fn a_built_in_applies_to_a_thread_with_no_acknowledgement() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    give_claude_a_builder_model(&service).await;
    let path = format!("/api/threads/{thread}/profile");
    let (status, body) = put(&service, &path, json!({ "profile_id": "builtin:careful" })).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["applied"]["engine"], "claude");
    assert_eq!(body["applied"]["permission_mode"], "supervised");
    assert_eq!(body["ignored"], json!(["autopilot_mode"]));
    assert_eq!(body["application"]["profile_id"], "builtin:careful");
    assert_eq!(body["application"]["name"], "Careful");
    assert_eq!(body["application"]["customized"], false);
    assert_eq!(applied_rows(&service, &thread).await, 1);
    // Clearing forgets the chip: the answer carries a null application.
    let (status, body) = put(&service, &path, json!({ "profile_id": null })).await;
    assert_eq!(status, 200, "{body}");
    assert!(body["application"].is_null(), "{body}");
    service.stop().await;
}

#[tokio::test]
async fn a_grant_raising_profile_is_refused_until_its_grants_are_acknowledged() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    give_claude_a_builder_model(&service).await;
    let (status, made) = service
        .send(
            "/api/profiles",
            draft("Everything", Value::Null, "full_access", "full"),
        )
        .await;
    assert_eq!(status, 201, "{made}");
    let id = made["id"].as_str().unwrap();
    let path = format!("/api/threads/{thread}/profile");
    let (status, body) = put(&service, &path, json!({ "profile_id": id })).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("confirm_required")),
        "{body}"
    );
    let grants = body["detail"]["grants"].as_array().unwrap();
    assert!(grants.contains(&json!("full_access")) && grants.contains(&json!("autopilot_full")));
    assert_eq!(applied_rows(&service, &thread).await, 0);
    let (status, body) = put(
        &service,
        &path,
        json!({ "profile_id": id, "ack_grants": ["full_access", "autopilot_full"] }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["applied"]["permission_mode"], "full_access");
    assert_eq!(body["application"]["name"], "Everything");
    assert_eq!(applied_rows(&service, &thread).await, 1);
    service.stop().await;
}

#[tokio::test]
async fn an_engine_switch_is_refused_while_a_run_is_live_and_changes_nothing() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    give_claude_a_builder_model(&service).await;
    let (status, made) = service
        .send(
            "/api/profiles",
            draft("On Claude", json!("claude"), "supervised", "off"),
        )
        .await;
    assert_eq!(status, 201, "{made}");
    let id = made["id"].as_str().unwrap();
    service.admit(&thread, "[hang]").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (status, body) = put(
        &service,
        &format!("/api/threads/{thread}/profile"),
        json!({ "profile_id": id }),
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("run_active")),
        "{body}"
    );
    let after = service.json(&format!("/api/threads/{thread}")).await;
    assert_eq!(after["engine"], "codex");
    assert_eq!(applied_rows(&service, &thread).await, 0);
    service.stop().await;
}
