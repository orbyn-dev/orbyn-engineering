//! The Taskboard route over HTTP: an empty board has seven quiet columns, bad input is refused, a stored task is a card in
//! Queued with no key text, a remote viewer may read it, and with the flag off it is not there. The table, credential and
//! flag-off walks of `crew_tests.rs` cover the route too, since it sits under `/api/crew`.

use serde_json::json;

use super::crew_tasks_tests::{seed_task, serve};

#[tokio::test]
async fn an_empty_board_has_seven_columns_and_bad_input_is_refused() {
    let app = serve("board-empty", true, false).await;
    let (status, board) = app.get("/api/crew/flow/board?crew=nobody").await;
    assert_eq!(status, 200);
    let columns = board["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 7);
    assert_eq!(columns[0]["column"], "queued");
    assert_eq!(columns[6]["column"], "stopped_cancelled");
    assert!(columns.iter().all(|c| c["count"] == 0));
    for path in [
        "/api/crew/flow/board",
        "/api/crew/flow/board?crew=..%2Fetc",
        "/api/crew/flow/board?crew=crew-1&column=nowhere",
        "/api/crew/flow/board?crew=crew-1&per_column=0",
        "/api/crew/flow/board?crew=crew-1&per_column=51",
        "/api/crew/flow/board?crew=crew-1&per_column=abc",
        "/api/crew/flow/board?crew=crew-1&limit=1000",
        "/api/crew/flow/board?crew=crew-1&engine=..%2Fx",
    ] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
}

#[tokio::test]
async fn a_stored_task_is_a_queued_card_without_its_key() {
    let app = serve("board-task", true, false).await;
    let (task, key_text) = seed_task(&app.core, "board");
    let key_id = app.core.store.crew_answer_key(&task).unwrap().unwrap().id;
    let (status, board) = app.get("/api/crew/flow/board?crew=crew-1").await;
    assert_eq!(status, 200);
    let queued = &board["columns"][0];
    assert_eq!(queued["count"], 1);
    let card = &queued["cards"][0];
    assert_eq!(card["id"], task.as_str());
    assert_eq!(card["name"], "Task board.");
    assert_eq!(card["engine"], json!(null), "not routed yet");
    assert_eq!(card["can_cancel"], true);
    assert_eq!(card["can_rerun"], false);
    let text = board.to_string();
    assert!(!text.contains(&key_text), "the board holds the key's check");
    assert!(
        !text.contains(key_id.as_str()),
        "the board holds the key's id"
    );
    let (_, one) = app
        .get("/api/crew/flow/board?crew=crew-1&column=queued&limit=1&offset=1")
        .await;
    assert_eq!(one["columns"][0]["cards"], json!([]));
    assert_eq!(one["columns"][0]["count"], 1);
}

#[tokio::test]
async fn a_remote_viewer_may_read_it_and_the_flag_off_hides_it() {
    let remote = serve("board-remote", true, true).await;
    let (status, _) = remote.get("/api/crew/flow/board?crew=crew-1").await;
    assert_eq!(status, 200);
    let off = serve("board-off", false, false).await;
    let (_, unknown) = off.get("/api/crew-nothing").await;
    let (status, body) = off.get("/api/crew/flow/board?crew=crew-1").await;
    assert_eq!(status, 404);
    assert_eq!(body, unknown, "it answers as an absent route does");
}
