//! The escape log over HTTP: the Preview gate, bad input, an empty log, one possible escape with its trail, the stored thresholds,
//! a damaged log answering `500 unreadable` and never a partial body, and a remote viewer reading. The rules and the revert source
//! are tested in `orbyn-core` (`store/crew_escapes_tests.rs`, `crew/tests/escapes.rs`). The table and flag-off walks of
//! `crew_tests.rs` cover these routes too, since they sit under `/api/crew`.

use orbyn_core::crew::changes::{STATE_CHANGING, changes_crew_state};
use orbyn_core::crew::escapes::THRESHOLDS_KEY;
use orbyn_core::crew::fixtures;
use serde_json::{Value, json};

use super::crew_tasks_tests::{Served, seed_task, serve, serve_with};
use crate::http::access::Access;

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

/// One POST: the service token when `token`, and the person key when `key` is given.
async fn post(
    app: &Served,
    path: &str,
    body: Value,
    token: bool,
    key: Option<&str>,
) -> (u16, Value) {
    let mut request = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}{path}", app.port))
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

fn decide(project: &str, state: &str) -> Value {
    json!({ "project": project, "seen": { "state": state } })
}

const WRITES: [&str; 4] = [
    "/api/crew/escapes/mark",
    "/api/crew/escapes/esc_one/confirm",
    "/api/crew/escapes/esc_one/dismiss",
    "/api/crew/escapes/esc_one/gap",
];

#[tokio::test]
async fn the_writes_are_person_routes_an_agent_cannot_reach_and_need_the_key() {
    // An agent's request is refused by path, as any state change under /api/crew is; the reads are not.
    for path in WRITES {
        assert!(changes_crew_state(path), "{path}");
    }
    for route in [
        "/api/crew/escapes/mark",
        "/api/crew/escapes/{id}/confirm",
        "/api/crew/escapes/{id}/dismiss",
        "/api/crew/escapes/{id}/gap",
    ] {
        assert!(STATE_CHANGING.contains(&("POST", route)), "{route}");
    }
    assert!(!changes_crew_state("/api/crew/escapes/esc_one"));

    // No development override: the key of this launch is the only way in, as in a build the person uses.
    let app = serve_with(
        "escapes-key",
        true,
        false,
        Access::new("token", vec![], None).with_person_key(Some("the-key")),
    )
    .await;
    let (project, id) = with_escape(&app, "The merge was reverted.");
    let body = |path: &str| match path {
        p if p.ends_with("/mark") => {
            let task = app
                .core
                .store
                .crew_escape(&id)
                .unwrap()
                .unwrap()
                .merge_task_id;
            json!({ "project": project, "merge_task_id": task })
        }
        p if p.ends_with("/gap") => {
            json!({ "project": project, "seen": { "state": "possible" }, "gap": "test_gap" })
        }
        _ => decide(&project, "possible"),
    };
    for path in WRITES {
        let (status, _) = post(&app, path, body(path), false, Some("the-key")).await;
        assert_eq!(status, 401, "{path}: the service token comes first");
        for key in [None, Some("wrong")] {
            let (status, reply) = post(&app, path, body(path), true, key).await;
            assert_eq!(
                (status, &reply["code"]),
                (403, &json!("person_required")),
                "{path}"
            );
        }
    }
    let state = || app.core.store.crew_escape(&id).unwrap().unwrap().state;
    assert_eq!(
        state(),
        orbyn_contracts::EscapeState::Possible,
        "every refused call left it possible"
    );

    // With the key, the same calls work: a confirm answers with the escape as it now stands, and nothing else confirmed it.
    let confirm = "/api/crew/escapes/esc_one/confirm";
    let (status, row) = post(&app, confirm, body(confirm), true, Some("the-key")).await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(
        (row["state"].clone(), row["decided_by"].clone()),
        (json!("confirmed"), json!("person"))
    );
    let (_, log) = app
        .get(&format!("/api/crew/escapes?project={project}"))
        .await;
    assert_eq!(
        (log["possible"].clone(), log["confirmed"].clone()),
        (json!(0), json!(1))
    );
}

#[tokio::test]
async fn a_person_marks_confirms_dismisses_and_sets_a_gap_and_the_refusals_are_plain() {
    let app = serve("escapes-writes", true, false).await;
    let (project, id) = with_escape(&app, "The merge was reverted.");
    let confirm = format!("/api/crew/escapes/{id}/confirm");
    let dismiss = format!("/api/crew/escapes/{id}/dismiss");
    let gap = format!("/api/crew/escapes/{id}/gap");
    let go = |path: String, body: Value| {
        let app = &app;
        async move { post(app, &path, body, true, None).await }
    };

    // A view that is not the escape's is 409 stale and writes nothing; so is a body with no `seen`.
    let (status, reply) = go(confirm.clone(), decide(&project, "confirmed")).await;
    assert_eq!((status, &reply["code"]), (409, &json!("stale")), "{reply}");
    let (status, _) = go(confirm.clone(), json!({ "project": project })).await;
    assert_eq!(status, 400);
    // Another project's escape is not found.
    let (status, _) = go(confirm.clone(), decide("nobody", "possible")).await;
    assert_eq!(status, 404);
    let (_, detail) = app.get(&format!("/api/crew/escapes/{id}")).await;
    assert_eq!(detail["escape"]["state"], "possible");
    assert_eq!(detail["events"].as_array().unwrap().len(), 1);

    let (status, row) = go(
        gap.clone(),
        json!({ "project": project, "seen": { "state": "possible" }, "gap": "tooling_gap" }),
    )
    .await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(
        (row["gap"].clone(), row["gap_by"].clone()),
        (json!("tooling_gap"), json!("person"))
    );
    let seen = json!({ "project": project, "seen": { "state": "possible", "gap": "tooling_gap" }, "note": "caught in review" });
    let (status, row) = go(dismiss.clone(), seen.clone()).await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["state"], "dismissed");
    // The same act again is the same answer; the opposite act is refused in plain words.
    let (status, again) = go(dismiss, seen.clone()).await;
    assert_eq!((status, &again), (200, &row));
    let decided =
        json!({ "project": project, "seen": { "state": "dismissed", "gap": "tooling_gap" } });
    let (status, refused) = go(confirm, decided).await;
    assert_eq!((status, &refused["code"]), (400, &json!("invalid_input")));
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("already dismissed"),
        "{refused}"
    );
    let (_, detail) = app.get(&format!("/api/crew/escapes/{id}")).await;
    let kinds: Vec<_> = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].clone())
        .collect();
    assert_eq!(
        kinds,
        [json!("detected"), json!("reclassified"), json!("dismissed")]
    );

    // A mark on a merge with no escape makes one confirmed escape; the log counts it apart from the dismissed one.
    let (task, _) = seed_task(&app.core, "marked");
    app.core
        .store
        .add_crew_merge(&fixtures::merge_record(&task, "/repo", "main", "y"))
        .unwrap();
    app.core
        .store
        .link_merge_to_project_for_test(&project, &task)
        .unwrap();
    let (status, marked) = go(
        "/api/crew/escapes/mark".into(),
        json!({ "project": project, "merge_task_id": task, "note": "broke the build" }),
    )
    .await;
    assert_eq!(status, 200, "{marked}");
    assert_eq!(
        (
            marked["source"].clone(),
            marked["state"].clone(),
            marked["reason"].clone()
        ),
        (
            json!("person_marked"),
            json!("confirmed"),
            json!("broke the build")
        )
    );
    let (_, log) = app
        .get(&format!("/api/crew/escapes?project={project}"))
        .await;
    assert_eq!(
        (
            log["possible"].clone(),
            log["confirmed"].clone(),
            log["dismissed"].clone()
        ),
        (json!(0), json!(1), json!(1))
    );
}

#[tokio::test]
async fn with_the_flag_off_the_writes_are_not_there_and_a_remote_viewer_cannot_use_them() {
    let off = serve("escapes-writes-off", false, false).await;
    for path in WRITES {
        let (status, reply) = post(&off, path, decide("p", "possible"), true, None).await;
        assert_eq!(
            (status, &reply["code"]),
            (404, &json!("not_found")),
            "{path}"
        );
    }
    let remote = serve("escapes-writes-remote", true, true).await;
    let (project, id) = with_escape(&remote, "The merge was reverted.");
    for path in WRITES {
        let (status, reply) = post(&remote, path, decide(&project, "possible"), true, None).await;
        assert_eq!(
            (status, &reply["code"]),
            (403, &json!("local_only")),
            "{path}"
        );
    }
    assert_eq!(
        remote.core.store.crew_escape(&id).unwrap().unwrap().state,
        orbyn_contracts::EscapeState::Possible
    );
}
