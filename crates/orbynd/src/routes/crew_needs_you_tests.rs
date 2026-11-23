//! The Crew "needs you" inbox over HTTP: an empty inbox, a stored task as one item, a dismissal that hides only that item, bad
//! input refused, a remote viewer reading but not dismissing, and the flag off hiding both routes. The table, credential and
//! flag-off walks of `crew_tests.rs` cover the routes too, since they sit under `/api/crew`.

use serde_json::{Value, json};

use super::crew_tasks_tests::{seed_task, serve};

async fn post(app: &super::crew_tasks_tests::Served, path: &str, body: Value) -> (u16, Value) {
    let url = format!("http://127.0.0.1:{}{path}", app.port);
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth("token")
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

#[tokio::test]
async fn an_empty_inbox_answers_and_bad_input_is_refused() {
    let app = serve("needs-you-empty", true, false).await;
    let (status, list) = app.get("/api/crew/needs-you").await;
    assert_eq!(status, 200);
    assert_eq!(list, json!({ "items": [], "count": 0 }));

    for (body, want) in [
        (json!({ "kind": "nowhere", "id": "x" }), 400),
        (json!({ "kind": "manual_check", "id": "x" }), 400),
        (json!({ "kind": "task_needs_person", "id": "x" }), 400),
        (json!({ "id": "x" }), 400),
        (json!({ "kind": "could_not_start" }), 400),
        (json!({ "kind": "could_not_start", "id": "../x" }), 400),
        (
            json!({ "kind": "could_not_start", "id": "no-such-task" }),
            404,
        ),
    ] {
        let (status, answer) = post(&app, "/api/crew/needs-you/dismiss", body.clone()).await;
        assert_eq!(status, want, "{body}: {answer}");
        if want == 400 {
            assert_eq!(answer["code"], "invalid_input", "{body}");
        } else {
            assert_eq!(answer["code"], "not_found", "{body}");
        }
    }
}

#[tokio::test]
async fn a_stored_task_is_an_item_and_dismiss_hides_it() {
    let app = serve("needs-you-task", true, false).await;
    let (task, key_text) = seed_task(&app.core, "needs-you");
    app.core
        .store
        .add_crew_run(&task, "lead-1", "member-01")
        .unwrap();
    app.core
        .store
        .move_crew_task(&task, "needs_person", None)
        .unwrap();

    let (status, list) = app.get("/api/crew/needs-you").await;
    assert_eq!(status, 200);
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["id"], task.as_str());
    assert_eq!(item["kind"], "could_not_start");
    assert_eq!(item["dismissible"], true);
    assert!(item["reason"].as_str().is_some_and(|r| !r.is_empty()));
    assert!(!list.to_string().contains(&key_text), "the key's check");

    let (status, done) = post(
        &app,
        "/api/crew/needs-you/dismiss",
        json!({ "kind": "could_not_start", "id": task }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(done, json!({ "dismissed": true }));
    let (_, list) = app.get("/api/crew/needs-you").await;
    assert_eq!(list["items"], json!([]));
    // The same dismissal again is idempotent.
    let (status, done) = post(
        &app,
        "/api/crew/needs-you/dismiss",
        json!({ "kind": "could_not_start", "id": task }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(done, json!({ "dismissed": true }));
}

#[tokio::test]
async fn a_remote_viewer_may_read_and_the_flag_off_hides_both_routes() {
    let remote = serve("needs-you-remote", true, true).await;
    let (status, _) = remote.get("/api/crew/needs-you").await;
    assert_eq!(status, 200);
    let (status, body) = post(
        &remote,
        "/api/crew/needs-you/dismiss",
        json!({ "kind": "could_not_start", "id": "no-such-task" }),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["code"], "local_only");

    let off = serve("needs-you-off", false, false).await;
    let (_, unknown) = off.get("/api/crew-nothing").await;
    let (status, body) = off.get("/api/crew/needs-you").await;
    assert_eq!(status, 404);
    assert_eq!(body, unknown, "it answers as an absent route does");
    let (status, body) = post(
        &off,
        "/api/crew/needs-you/dismiss",
        json!({ "kind": "could_not_start", "id": "no-such-task" }),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body, unknown, "it answers as an absent route does");
}
