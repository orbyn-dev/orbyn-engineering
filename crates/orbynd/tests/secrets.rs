//! Settings › Secrets through the real service: the view, each change, the pattern checks and
//! their error bodies, and that the settings survive a restart. Secret-looking values are built
//! at run time and named by label.

mod support;

use reqwest::Method;
use serde_json::{Value, json};
use support::{Options, Service, TempDir};

async fn call(service: &Service, method: Method, path: &str, body: Value) -> (u16, Value) {
    let response = service
        .client
        .request(method, format!("{}{path}", service.url))
        .bearer_auth(&service.token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn start(dir: &TempDir) -> Service {
    Service::start(&dir.path().join("data"), Options::default())
}

fn pattern(name: &str, text: &str, kind: &str) -> Value {
    json!({ "name": name, "pattern": text, "kind": kind, "applies_to": "anywhere" })
}

#[tokio::test]
async fn the_view_starts_safe_and_each_change_comes_back_whole() {
    let dir = TempDir::new();
    let service = start(&dir);
    let view = service.json("/api/settings/secrets").await;
    assert_eq!(view["terminal"], true);
    assert_eq!(view["agents"], true);
    assert_eq!(view["crew_locked"], true);
    assert!(
        view["formats"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["enabled"] == true)
    );
    assert!(view["limits"]["max_patterns"].as_u64().unwrap() > 0);

    let (code, view) = call(
        &service,
        Method::PUT,
        "/api/settings/secrets/surfaces",
        json!({ "agents": false }),
    )
    .await;
    assert_eq!(code, 200, "{view}");
    assert_eq!(
        (view["terminal"].clone(), view["agents"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(view["crew_locked"], true, "no request moves it");

    let (code, view) = call(
        &service,
        Method::PUT,
        "/api/settings/secrets/formats/slack_token",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(code, 200, "{view}");
    let slack = view["formats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "slack_token")
        .unwrap()
        .clone();
    assert_eq!(slack["enabled"], false);
    let (code, body) = call(
        &service,
        Method::PUT,
        "/api/settings/secrets/formats/no_such_format",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!((code, body["code"].clone()), (404, json!("not_found")));
    service.stop().await;
}

#[tokio::test]
async fn an_invalid_pattern_is_refused_with_words_and_a_position_and_nothing_is_saved() {
    let dir = TempDir::new();
    let service = start(&dir);
    let bad = format!("{}_([A-Za-z0-9]{{24}}", "ab");
    let (code, body) = call(
        &service,
        Method::POST,
        "/api/settings/secrets/patterns",
        pattern("Broken", &bad, "regex"),
    )
    .await;
    assert_eq!(code, 422, "{body}");
    assert_eq!(body["code"], "invalid_pattern");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("group opened at position 3 is not closed")
    );
    assert_eq!(body["detail"]["position"], 3);
    let view = service.json("/api/settings/secrets").await;
    assert!(view["patterns"].as_array().unwrap().is_empty());

    let (code, body) = call(
        &service,
        Method::POST,
        "/api/settings/secrets/patterns/test",
        json!({ "pattern": bad, "kind": "regex", "applies_to": "anywhere", "sample": "x" }),
    )
    .await;
    assert_eq!(code, 200, "a test reports, it does not fail");
    assert_eq!(body["problem"]["position"], 3);
    service.stop().await;
}

#[tokio::test]
async fn a_broad_pattern_is_refused_until_the_person_accepts_it() {
    let dir = TempDir::new();
    let service = start(&dir);
    let (code, body) = call(
        &service,
        Method::POST,
        "/api/settings/secrets/patterns",
        pattern("Everything", ".*", "regex"),
    )
    .await;
    assert_eq!(
        (code, body["code"].clone()),
        (422, json!("pattern_too_broad"))
    );
    assert!(body["detail"]["lines_total"].as_u64().unwrap() > 0);
    let mut accepted = pattern("Everything", ".*", "regex");
    accepted["accept_broad"] = json!(true);
    let (code, view) = call(
        &service,
        Method::POST,
        "/api/settings/secrets/patterns",
        accepted,
    )
    .await;
    assert_eq!(code, 200, "{view}");
    assert_eq!(view["patterns"].as_array().unwrap().len(), 1);
    service.stop().await;
}

#[tokio::test]
async fn patterns_are_added_tried_edited_removed_and_kept_across_a_restart() {
    let dir = TempDir::new();
    let service = start(&dir);
    let text = format!("{}_[A-Za-z0-9]{{10}}", "live");
    let (code, view) = call(
        &service,
        Method::POST,
        "/api/settings/secrets/patterns",
        pattern("Stripe", &text, "regex"),
    )
    .await;
    assert_eq!(code, 200, "{view}");
    let id = view["patterns"][0]["id"].as_str().unwrap().to_owned();

    let sample = format!("KEY={}_{}", "live", "Abcdefgh12");
    let (_, tried) = call(
        &service,
        Method::POST,
        "/api/settings/secrets/patterns/test",
        json!({ "pattern": text, "kind": "regex", "applies_to": "anywhere", "sample": sample }),
    )
    .await;
    assert_eq!(tried["matches"].as_array().unwrap().len(), 1);
    assert_eq!(tried["masked"], format!("KEY={}", "•".repeat(15)));

    let (code, view) = call(
        &service,
        Method::PUT,
        &format!("/api/settings/secrets/patterns/{id}"),
        pattern("Stripe live", &text, "regex"),
    )
    .await;
    assert_eq!(code, 200, "{view}");
    service.stop().await;

    let service = start(&dir);
    let view = service.json("/api/settings/secrets").await;
    assert_eq!(view["patterns"][0]["name"], "Stripe live");
    let (code, view) = call(
        &service,
        Method::DELETE,
        &format!("/api/settings/secrets/patterns/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(code, 200, "{view}");
    assert!(view["patterns"].as_array().unwrap().is_empty());
    let (code, _) = call(
        &service,
        Method::DELETE,
        &format!("/api/settings/secrets/patterns/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(code, 404);
    service.stop().await;
}

#[tokio::test]
async fn a_project_is_switched_off_by_id_and_listed_by_name() {
    let dir = TempDir::new();
    let service = start(&dir);
    service.ensure_workspace(dir.path()).await;
    let (status, project) = service
        .send("/api/projects/create", json!({ "name": "solo" }))
        .await;
    assert_eq!(status, 201, "{project}");
    let id = project["id"].as_str().unwrap();
    let path = format!("/api/settings/secrets/projects/{id}");
    let (code, view) = call(&service, Method::PUT, &path, json!({ "redaction": false })).await;
    assert_eq!(code, 200, "{view}");
    assert_eq!(view["projects_off"][0]["name"], "solo");
    let (_, view) = call(&service, Method::PUT, &path, json!({ "redaction": true })).await;
    assert!(view["projects_off"].as_array().unwrap().is_empty());
    let (code, _) = call(
        &service,
        Method::PUT,
        "/api/settings/secrets/projects/missing",
        json!({ "redaction": false }),
    )
    .await;
    assert_eq!(code, 404);
    service.stop().await;
}

#[tokio::test]
async fn the_routes_need_the_credential() {
    let dir = TempDir::new();
    let service = start(&dir);
    for path in ["/api/settings/secrets"] {
        let response = service
            .client
            .get(format!("{}{path}", service.url))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 401);
    }
    service.stop().await;
}
