//! `POST /api/crew/needs-you/notify` over HTTP (KC10): the first window to ask for an item is given it, a later one is not, and a
//! body that names too many items or a kind that does not exist is refused.

use serde_json::json;

use super::crew_lead_tests::serve;

#[tokio::test]
async fn the_first_ask_is_given_the_item_and_the_second_is_not() {
    let app = serve("notify", true, false).await;
    let thread = app.thread("Lead");
    let task = app.task("waits", &thread, Some("needs_person"));
    let body = json!({ "items": [
        { "kind": "could_not_start", "id": task },
        { "kind": "could_not_start", "id": "sha256:unknown" },
    ] });

    let (status, first) = app
        .call("POST", "/api/crew/needs-you/notify", Some(body.clone()))
        .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(
        first,
        json!({ "claimed": [{ "kind": "could_not_start", "id": task }] })
    );
    let (status, second) = app
        .call("POST", "/api/crew/needs-you/notify", Some(body))
        .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second, json!({ "claimed": [] }));
}

#[tokio::test]
async fn a_bad_body_is_refused_and_the_flag_off_hides_the_route() {
    let app = serve("notify-bad", true, false).await;
    let many: Vec<_> = (0..51)
        .map(|n| json!({ "kind": "task_needs_person", "id": format!("sha256:{n}") }))
        .collect();
    let (status, body) = app
        .call(
            "POST",
            "/api/crew/needs-you/notify",
            Some(json!({ "items": many })),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input");
    let (status, _) = app
        .call(
            "POST",
            "/api/crew/needs-you/notify",
            Some(json!({ "items": [{ "kind": "not_a_kind", "id": "sha256:a" }] })),
        )
        .await;
    assert_eq!(status, 400);

    let off = serve("notify-off", false, false).await;
    let (status, _) = off
        .call(
            "POST",
            "/api/crew/needs-you/notify",
            Some(json!({ "items": [] })),
        )
        .await;
    assert_eq!(status, 404);
}
