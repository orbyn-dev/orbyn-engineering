//! Command-block reads through the real service: the local list and output routes,
//! their empty answer for a shell with no marks, and the `404`s for anything gone.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};

async fn get(service: &Service, path: &str) -> (u16, Value) {
    let response = service.get(path).send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn blocks_answer_not_found_or_an_empty_list() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;

    let (code, body) = get(
        &service,
        "/api/threads/no-such-thread/terminals/main/blocks",
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    let (code, body) = get(
        &service,
        &format!("/api/threads/{thread}/terminals/main/blocks"),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    let (code, terminal) = service
        .send(&format!("/api/threads/{thread}/terminals"), json!({}))
        .await;
    assert_eq!(code, 201, "{terminal}");
    let terminal = terminal["id"].as_str().unwrap();
    let base = format!("/api/threads/{thread}/terminals/{terminal}");

    // A shell that emits no OSC 133 marks yields no blocks, not an error.
    let (code, list) = get(&service, &format!("{base}/blocks")).await;
    assert_eq!(code, 200, "{list}");
    assert_eq!(list["blocks"], json!([]));
    assert!(list["next_before"].is_null());
    assert_eq!(list["limits"]["max_blocks"], json!(500));
    assert_eq!(list["limits"]["max_block_bytes"], json!(1_048_576));
    assert_eq!(list["limits"]["max_total_bytes"], json!(8_388_608));

    let (code, body) = get(&service, &format!("{base}/blocks/no-such-block/output")).await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    let (code, body) = get(&service, &format!("{base}/blocks?before=nope")).await;
    assert_eq!((code, body["code"].as_str()), (422, Some("bad_cursor")));

    let closed = service
        .client
        .delete(format!(
            "{}/api/threads/{thread}/terminals/{terminal}",
            service.url
        ))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    assert_eq!(closed.status(), 204);
    let (code, body) = get(&service, &format!("{base}/blocks")).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (404, Some("not_found")),
        "closing the terminal drops its blocks"
    );
    service.stop().await;
}
