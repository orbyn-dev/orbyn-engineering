//! The escape log over HTTP: the Preview gate, bad input, an empty log, one possible escape with its trail, the stored thresholds,
//! a damaged log answering `500 unreadable` and never a partial body, and a remote viewer reading. The rules and the revert source
//! are tested in `orbyn-core` (`store/crew_escapes_tests.rs`, `crew/tests/escapes.rs`). The table and flag-off walks of
//! `crew_tests.rs` cover these routes too, since they sit under `/api/crew`.

use orbyn_core::crew::escapes::THRESHOLDS_KEY;
use orbyn_core::crew::fixtures;
use serde_json::json;

use super::crew_tasks_tests::{Served, seed_task, serve};

/// A project with one merge and one possible `revert` escape for it. Returns the project id and the escape id.
fn with_escape(app: &Served, reason: &str) -> (String, String) {
    let project = fixtures::add_project(
        &app.core,
        app.core.config.data_dir.join("p").to_str().unwrap(),
    );
    let (task, _) = seed_task(&app.core, "escape");
    app.core
        .store
        .add_crew_merge(&fixtures::merge_record(&task, "/repo", "main", "x"))
        .unwrap();
    app.core
        .store
        .insert_escape_for_test("esc_one", &project, &task, "revert", "possible", reason)
        .unwrap();
    (project, "esc_one".to_owned())
}

#[tokio::test]
async fn an_empty_log_counts_nothing_and_bad_input_is_refused() {
    let app = serve("escapes-empty", true, false).await;
    let project = fixtures::add_project(
        &app.core,
        app.core.config.data_dir.join("p").to_str().unwrap(),
    );
    let (status, log) = app
        .get(&format!("/api/crew/escapes?project={project}"))
        .await;
    assert_eq!(status, 200);
    assert_eq!(log["rows"], json!([]));
    assert_eq!(log["total"], 0);
    assert_eq!(
        (
            log["possible"].clone(),
            log["confirmed"].clone(),
            log["dismissed"].clone()
        ),
        (json!(0), json!(0), json!(0))
    );
    assert_eq!(
        (log["window_days"].clone(), log["min_merges"].clone()),
        (json!(60), json!(10))
    );
    assert_eq!(log["low_sample"], true);
    for path in [
        "/api/crew/escapes".to_owned(),
        "/api/crew/escapes?project=..%2Fetc".to_owned(),
        format!("/api/crew/escapes?project={project}&state=likely"),
        format!("/api/crew/escapes?project={project}&limit=0"),
        format!("/api/crew/escapes?project={project}&limit=201"),
        format!("/api/crew/escapes?project={project}&offset=abc"),
        "/api/crew/escapes/..%2Fx".to_owned(),
    ] {
        let (status, body) = app.get(&path).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
    assert_eq!(app.get("/api/crew/escapes?project=nobody").await.0, 404);
    assert_eq!(app.get("/api/crew/escapes/esc_missing").await.0, 404);
}

#[tokio::test]
async fn a_possible_escape_is_listed_and_counted_apart_with_its_trail() {
    let app = serve("escapes-one", true, false).await;
    let (project, id) = with_escape(&app, "The merge was reverted.");
    let (status, log) = app
        .get(&format!("/api/crew/escapes?project={project}"))
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        (
            log["possible"].clone(),
            log["confirmed"].clone(),
            log["dismissed"].clone()
        ),
        (json!(1), json!(0), json!(0))
    );
    assert_eq!(log["total"], 1);
    let row = &log["rows"][0];
    assert_eq!(
        (
            row["id"].as_str(),
            row["source"].as_str(),
            row["state"].as_str()
        ),
        (Some("esc_one"), Some("revert"), Some("possible"))
    );
    assert!(
        row["line"].as_str().unwrap().contains("not confirmed"),
        "{row}"
    );
    assert_eq!(row["decided_by"], json!(null));
    assert_eq!(row["areas"], json!(["src/app"]));
    let (_, none) = app
        .get(&format!(
            "/api/crew/escapes?project={project}&state=confirmed"
        ))
        .await;
    assert_eq!(
        (none["total"].clone(), none["rows"].clone()),
        (json!(0), json!([]))
    );

    let (status, detail) = app.get(&format!("/api/crew/escapes/{id}")).await;
    assert_eq!(status, 200);
    assert_eq!(detail["escape"]["id"], "esc_one");
    assert_eq!(detail["events"][0]["kind"], "detected");
    assert_eq!(detail["truncated"], false);
}

#[tokio::test]
async fn a_reason_goes_through_the_scrub_and_no_diff_text_is_in_the_body() {
    let app = serve("escapes-scrub", true, false).await;
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let (project, id) = with_escape(&app, &format!("token {secret}"));
    for path in [
        format!("/api/crew/escapes?project={project}"),
        format!("/api/crew/escapes/{id}"),
    ] {
        let (status, body) = app.get(&path).await;
        assert_eq!(status, 200);
        assert!(!body.to_string().contains(secret), "{path}");
    }
}

#[tokio::test]
async fn thresholds_default_then_stored_and_a_bad_document_fails_closed() {
    let app = serve("escapes-thresholds", true, false).await;
    let (status, first) = app.get("/api/crew/escapes/thresholds").await;
    assert_eq!(status, 200);
    assert_eq!(
        first,
        json!({ "report_days": 60, "follow_up_days": 14, "min_merges": 10, "rate_multiple": 2, "stored": false })
    );
    app.core
        .store
        .set_setting(
            THRESHOLDS_KEY,
            r#"{"report_days":30,"follow_up_days":7,"min_merges":5,"rate_multiple":3}"#,
        )
        .unwrap();
    let (_, stored) = app.get("/api/crew/escapes/thresholds").await;
    assert_eq!(
        stored,
        json!({ "report_days": 30, "follow_up_days": 7, "min_merges": 5, "rate_multiple": 3, "stored": true })
    );
    let project = fixtures::add_project(
        &app.core,
        app.core.config.data_dir.join("p").to_str().unwrap(),
    );
    let (_, log) = app
        .get(&format!("/api/crew/escapes?project={project}"))
        .await;
    assert_eq!(
        (log["window_days"].clone(), log["min_merges"].clone()),
        (json!(30), json!(5))
    );
    for bad in [
        "not json",
        r#"{"report_days":0,"follow_up_days":7,"min_merges":5,"rate_multiple":3}"#,
        r#"{"report_days":30,"follow_up_days":7,"min_merges":5}"#,
        r#"{"report_days":30,"follow_up_days":7,"min_merges":5,"rate_multiple":3,"extra":1}"#,
    ] {
        app.core.store.set_setting(THRESHOLDS_KEY, bad).unwrap();
        let (status, body) = app.get("/api/crew/escapes/thresholds").await;
        assert_eq!(
            (status, body["code"].as_str()),
            (500, Some("unreadable")),
            "{bad}"
        );
        let (status, body) = app
            .get(&format!("/api/crew/escapes?project={project}"))
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (500, Some("unreadable")),
            "{bad}"
        );
    }
}

#[tokio::test]
async fn a_damaged_log_is_unreadable_not_partial() {
    let app = serve("escapes-damaged", true, false).await;
    let (project, id) = with_escape(&app, "The merge was reverted.");
    app.core.store.damage_escapes_for_test().unwrap();
    for path in [
        format!("/api/crew/escapes?project={project}"),
        format!("/api/crew/escapes/{id}"),
    ] {
        let (status, body) = app.get(&path).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (500, Some("unreadable")),
            "{path}"
        );
        assert!(
            body.get("rows").is_none() && body.get("escape").is_none(),
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_remote_viewer_reads_and_the_flag_off_hides_every_route() {
    let remote = serve("escapes-remote", true, true).await;
    let (project, id) = with_escape(&remote, "The merge was reverted.");
    for path in [
        format!("/api/crew/escapes?project={project}"),
        format!("/api/crew/escapes/{id}"),
        "/api/crew/escapes/thresholds".to_owned(),
    ] {
        assert_eq!(remote.get(&path).await.0, 200, "{path}");
    }
    let off = serve("escapes-off", false, false).await;
    for path in [
        "/api/crew/escapes?project=p".to_owned(),
        "/api/crew/escapes/esc_one".to_owned(),
        "/api/crew/escapes/thresholds".to_owned(),
    ] {
        let (status, body) = off.get(&path).await;
        let (missing_status, missing) = off.get("/api/crew/not-a-route").await;
        assert_eq!((status, body), (missing_status, missing), "{path}");
    }
}
