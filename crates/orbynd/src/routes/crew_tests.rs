//! The Crew routes: the Preview gate, the route table, the credential, the remote refusal, every guard refusal over HTTP, and
//! what a response never holds. The table test walks every route the router registers, so a `/api/crew` route that is in
//! neither list, or that answers with the flag off, fails here. Values that stand for a secret or a key are named by label and
//! built at run time.

use std::collections::BTreeSet;
use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_core::crew::Stamper;
use orbyn_core::crew::fixtures::{self, merge_record};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::crew::GUARDS;
use super::router;
use super::tests::registered;
use crate::http::AppState;
use crate::http::access::Access;
use orbyn_core::crew::changes::{READS, STATE_CHANGING, changes_crew_state};

struct Served {
    port: u16,
    core: Arc<Core>,
    routes: BTreeSet<(String, String)>,
    dir: std::path::PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-crew-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let routes = registered(&app);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        routes,
        dir,
    }
}

impl Served {
    async fn call_as(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new().request(method.parse().unwrap(), url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        self.call_as(method, path, body, Some("token")).await
    }

    fn crew_routes(&self) -> Vec<(String, String)> {
        self.routes
            .iter()
            .filter(|(_, path)| path.contains("crew"))
            .cloned()
            .collect()
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn concrete(path: &str) -> String {
    path.replace("{task_id}", "task-1")
        .replace("{id}", "task-1")
}

/// A task with a stored brief and key, and the text of one of the key's checks.
fn seed_task(core: &Core, label: &str) -> (String, String) {
    let stamper = Stamper {
        crew_id: wire::CrewId::new("crew-1").unwrap(),
        engine: wire::CrewToken::new("claude").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&json!({
            "kind": "implement",
            "outcome": format!("Task {label}."),
            "inputs": [],
            "files": [{ "path": "src/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key_command = format!("echo key-check-{label}-text");
    let key = stamper
        .answer_key(
            &json!({
                "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": key_command },
                    "proves": "It runs." }],
                "protected_paths": ["tests/**"]
            }),
            &brief.id,
        )
        .unwrap();
    let task = core
        .store
        .create_crew_task(&brief, &key, &format!("run-{label}"))
        .unwrap();
    (task.id, key_command)
}

#[tokio::test]
async fn with_the_flag_off_every_crew_route_is_not_found() {
    let app = serve("off", false, false).await;
    let routes = app.crew_routes();
    assert!(!routes.is_empty(), "the router registers Crew routes");
    let (_, unknown) = app.call("GET", "/api/crew-nothing", None).await;
    for (method, path) in &routes {
        assert!(
            path.starts_with("/api/crew"),
            "a Crew route sits under /api/crew"
        );
        // The route's own method and every other: a wrong method must not tell a viewer the route exists.
        for tried in [
            "GET",
            "POST",
            "PUT",
            "PATCH",
            "DELETE",
            "HEAD",
            method.as_str(),
        ] {
            let (status, body) = app.call(tried, &concrete(path), Some(json!({}))).await;
            assert_eq!(status, 404, "{tried} on the {method} route is hidden");
            if tried != "HEAD" {
                assert_eq!(body, unknown, "it answers as an absent route does");
            }
        }
    }
    let (status, info) = app.call("GET", "/api/info", None).await;
    assert_eq!(status, 200);
    assert!(info.get("crew_preview").is_none());
}

#[tokio::test]
async fn the_route_table_agrees_with_the_core_marker_and_every_change_names_its_guard() {
    let app = serve("table", true, false).await;
    let found: BTreeSet<(String, String)> = app
        .crew_routes()
        .into_iter()
        .map(|(method, path)| (method, path.replace("{task_id}", "{id}")))
        .collect();
    let owned = |list: &[(&str, &str)]| -> BTreeSet<(String, String)> {
        list.iter()
            .map(|(m, p)| ((*m).to_owned(), (*p).to_owned()))
            .collect()
    };
    let (reads, changes) = (owned(READS), owned(STATE_CHANGING));
    assert!(reads.is_disjoint(&changes), "a route is a read or a change");
    let listed: BTreeSet<_> = reads.union(&changes).cloned().collect();
    assert_eq!(
        found, listed,
        "every Crew route is a read or a change the core marks, and every marked route exists"
    );
    // The marker the core refuses by path agrees with the methods: nothing but a GET is a read.
    for (method, path) in &found {
        let marked = changes_crew_state(&path.replace("{id}", "task-1"));
        assert_eq!(
            marked,
            method != "GET",
            "{method} is marked as the core marks it"
        );
    }
    // Every change names a guard, and every guard is for a marked change.
    let guarded: BTreeSet<(String, String)> = GUARDS
        .iter()
        .map(|(m, p, _)| ((*m).to_owned(), (*p).to_owned()))
        .collect();
    assert_eq!(guarded, changes, "each change has exactly one guard");
    for (_, _, guard) in GUARDS {
        assert!(guard.len() > 20, "a change route names its guard");
        assert!(guard.contains("local only"), "a change route is local only");
    }
}

#[tokio::test]
async fn every_crew_route_needs_the_credential() {
    for preview in [true, false] {
        let app = serve(if preview { "auth-on" } else { "auth-off" }, preview, false).await;
        for (method, path) in app.crew_routes() {
            for token in [None, Some("not-the-token")] {
                let (status, _) = app
                    .call_as(&method, &concrete(&path), Some(json!({})), token)
                    .await;
                assert_eq!(status, 401, "route {method} needs the token");
            }
        }
    }
}

#[tokio::test]
async fn a_remote_viewer_reads_and_cannot_change_anything() {
    let app = serve("remote", true, true).await;
    for (method, path) in STATE_CHANGING {
        let (status, body) = app.call(method, &concrete(path), Some(json!({}))).await;
        assert_eq!(status, 403, "a remote viewer cannot change state");
        // Approving once needs the desktop app, so a remote viewer is told that; everything else is local only.
        let code = if path.ends_with("/approve-once") {
            "desktop_required"
        } else {
            "local_only"
        };
        assert_eq!(body["code"], code, "{path}");
    }
    let (status, _) = app.call("GET", "/api/crew/autopilot", None).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_preview_flag_is_readable_and_never_settable() {
    let app = serve("flag", true, false).await;
    let (status, body) = app.call("GET", "/api/crew/preview", None).await;
    assert_eq!((status, &body), (200, &json!({ "enabled": true })));
    let (_, info) = app.call("GET", "/api/info", None).await;
    assert_eq!(info["crew_preview"], true);
    for method in ["PUT", "POST", "PATCH", "DELETE"] {
        let (status, _) = app
            .call(
                method,
                "/api/crew/preview",
                Some(json!({ "enabled": false })),
            )
            .await;
        assert_eq!(status, 405, "{method} cannot set the flag");
    }
    assert!(app.core.config.crew_preview, "the flag did not move");
}

#[tokio::test]
async fn reads_answer_empty_views_and_every_list_is_bounded() {
    let app = serve("reads", true, false).await;
    for path in [
        "/api/crew/autopilot",
        "/api/crew/autopilot/guards",
        "/api/crew/autopilot/review",
        "/api/crew/autopilot/merge-queue",
        "/api/crew/autopilot/watch",
        "/api/crew/autopilot/digest",
        "/api/crew/approvals",
        "/api/crew/stops",
        "/api/crew/trust",
    ] {
        let (status, _) = app.call("GET", path, None).await;
        assert_eq!(status, 200, "{path}");
    }
    let (_, modes) = app.call("GET", "/api/crew/autopilot", None).await;
    assert_eq!(modes["effective_default"], "full");
    assert_eq!(modes["confirmed"], false);
    let (_, guards) = app.call("GET", "/api/crew/autopilot/guards", None).await;
    assert_eq!(guards["always_denied"].as_array().unwrap().len(), 10);
    assert_eq!(guards["in_a_row_limit"], 3);
    assert_eq!(guards["in_a_run_limit"], 20);
    let (_, digest) = app.call("GET", "/api/crew/autopilot/digest", None).await;
    assert_eq!(digest["quiet"], true);
    let (_, queue) = app
        .call("GET", "/api/crew/autopilot/merge-queue", None)
        .await;
    assert_eq!(queue["forecast"]["conflict_forecast"], false);
    // Bounds and shapes.
    for path in [
        "/api/crew/approvals?limit=0",
        "/api/crew/approvals?limit=201",
        "/api/crew/approvals?limit=-1",
        "/api/crew/approvals?offset=abc",
        "/api/crew/approvals?offset=100001",
        "/api/crew/approvals?kind=nothing",
        "/api/crew/approvals?task=..%2Fetc",
        "/api/crew/autopilot/review?limit=1000",
        "/api/crew/autopilot/watch?limit=0",
        "/api/crew/autopilot/merge-queue?offset=x",
        "/api/crew/autopilot/digest?hours=0",
        "/api/crew/autopilot/digest?hours=721",
        "/api/crew/autopilot/risk/..%2F..%2Fetc",
        "/api/crew/autopilot/review/a%20b/revert-check",
    ] {
        let (status, body) = app.call("GET", path, None).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
    let (status, _) = app
        .call("GET", "/api/crew/autopilot/risk/task-1", None)
        .await;
    assert_eq!(status, 404);
    let (status, _) = app
        .call("GET", "/api/crew/approvals?limit=200&offset=100000", None)
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_area_reads_are_bounded_read_only_and_never_write() {
    let app = serve("areas", true, false).await;
    let (status, thresholds) = app
        .call("GET", "/api/crew/autopilot/thresholds", None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(thresholds["proposed"], true);
    assert_eq!(thresholds["window_days"], 14);
    assert_eq!(thresholds["min_merges"], 5);
    for path in [
        "/api/crew/autopilot/areas",
        "/api/crew/autopilot/areas?project=..%2Fx",
        "/api/crew/autopilot/areas?project=a%20b",
        "/api/crew/autopilot/areas/history",
        "/api/crew/autopilot/areas/history?project=p",
        "/api/crew/autopilot/areas/history?project=p&area=",
        "/api/crew/trust?project=..%2Fx",
    ] {
        let (status, body) = app.call("GET", path, None).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
    for path in [
        "/api/crew/autopilot/areas?project=no-such-project",
        "/api/crew/autopilot/areas/history?project=no-such-project&area=src%2Fa",
        "/api/crew/trust?project=no-such-project",
    ] {
        let (status, _) = app.call("GET", path, None).await;
        assert_eq!(status, 404, "{path}");
    }
    let project = fixtures::add_project(&app.core, app.dir.join("areas-project").to_str().unwrap());
    let (status, areas) = app
        .call(
            "GET",
            &format!("/api/crew/autopilot/areas?project={}", project),
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(areas["ceiling"], "full");
    assert_eq!(areas["enforced"], true);
    assert_eq!(areas["total"], 0);
    let (status, _) = app
        .call(
            "GET",
            &format!("/api/crew/autopilot/areas?project={}&limit=201", project),
            None,
        )
        .await;
    assert_eq!(status, 400);
    let (status, history) = app
        .call(
            "GET",
            &format!(
                "/api/crew/autopilot/areas/history?project={}&area=src%2Fa",
                project
            ),
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(history["area"], "src/a");
    assert_eq!(history["events"], json!([]));
    let (status, trust) = app
        .call("GET", &format!("/api/crew/trust?project={}", project), None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(trust["autonomy_by_area"], true);
    // All three are reads: the core's marker does not refuse them, and none of them is listed as a change.
    for path in [
        "/api/crew/autopilot/areas",
        "/api/crew/autopilot/areas/history",
        "/api/crew/autopilot/thresholds",
    ] {
        assert!(!changes_crew_state(path), "{path}");
        assert!(STATE_CHANGING.iter().all(|(_, p)| *p != path));
        assert!(READS.contains(&("GET", path)));
    }
}

#[tokio::test]
async fn a_mode_is_saved_only_through_the_core_and_only_for_something_real() {
    let app = serve("mode", true, false).await;
    let put = |body: Value| {
        let app = &app;
        async move {
            app.call("PUT", "/api/crew/autopilot/mode", Some(body))
                .await
        }
    };
    for (body, want) in [
        (json!({ "scope": "default", "id": "x", "mode": "on" }), 400),
        (json!({ "scope": "repo", "mode": "on" }), 400),
        (json!({ "scope": "repo", "id": "../x", "mode": "on" }), 400),
        (
            json!({ "scope": "repo", "id": "no-such-project", "mode": "on" }),
            404,
        ),
        (
            json!({ "scope": "crew", "id": "no-such-crew", "mode": "on" }),
            404,
        ),
        (json!({ "scope": "default", "mode": "yolo" }), 400),
        (json!({ "scope": "global", "mode": "on" }), 400),
        (json!({ "mode": "on" }), 400),
        (
            json!({ "scope": "default", "mode": "on", "extra": true }),
            200,
        ),
    ] {
        let (status, _) = put(body.clone()).await;
        assert_eq!(status, want, "{body}");
    }
    let (_, modes) = app.call("GET", "/api/crew/autopilot", None).await;
    assert_eq!(modes["effective_default"], "on");
    assert_eq!(modes["confirmed"], true);
    assert_eq!(modes["entries"].as_array().unwrap().len(), 1);
    // A crew that has a task may have its own mode.
    let (task, _) = seed_task(&app.core, "mode");
    app.core
        .store
        .add_crew_run(&task, "lead-thread", "worker-1")
        .unwrap();
    let (status, _) = put(json!({ "scope": "crew", "id": "crew-1", "mode": "off" })).await;
    assert_eq!(status, 200);
    let (_, modes) = app.call("GET", "/api/crew/autopilot", None).await;
    assert_eq!(modes["entries"].as_array().unwrap().len(), 2);
    assert_eq!(modes["effective_default"], "on");
}

#[tokio::test]
async fn confirming_keeps_the_default_and_changes_no_mode() {
    let app = serve("confirm", true, false).await;
    let (status, modes) = app
        .call("POST", "/api/crew/autopilot/confirm", Some(json!({})))
        .await;
    assert_eq!(status, 200);
    assert_eq!(modes["confirmed"], true);
    assert_eq!(modes["effective_default"], "full");
}

#[tokio::test]
async fn an_answer_to_a_merge_refuses_what_it_should_before_the_core_acts() {
    let app = serve("answers", true, false).await;
    let (task, _) = seed_task(&app.core, "answers");
    let repo = app.dir.join("not-a-repo");
    app.core
        .store
        .add_crew_merge(&merge_record(
            &task,
            &repo.to_string_lossy(),
            "main",
            "plain",
        ))
        .unwrap();
    let base = format!("/api/crew/autopilot/review/{task}");
    let seen = "b".repeat(40);
    // Unknown task, bad ids and commits, stale commit.
    let (status, _) = app
        .call(
            "POST",
            "/api/crew/autopilot/review/other-task/looks-good",
            Some(json!({ "merge_commit": seen })),
        )
        .await;
    assert_eq!(status, 404);
    let (status, _) = app
        .call(
            "POST",
            &format!("{base}/looks-good"),
            Some(json!({ "merge_commit": "zz" })),
        )
        .await;
    assert_eq!(status, 400);
    let (status, body) = app
        .call(
            "POST",
            &format!("{base}/looks-good"),
            Some(json!({ "merge_commit": "d".repeat(40) })),
        )
        .await;
    assert_eq!(status, 400);
    assert!(body["message"].as_str().unwrap().contains("changed"));
    let (status, _) = app
        .call("POST", &format!("{base}/looks-good"), Some(json!({})))
        .await;
    assert_eq!(status, 400, "the commit is required");
    // Comment bounds.
    for text in [
        json!(null),
        json!(""),
        json!("   "),
        json!("x".repeat(2001)),
    ] {
        let (status, _) = app
            .call(
                "POST",
                &format!("{base}/comment"),
                Some(json!({ "merge_commit": seen, "text": text })),
            )
            .await;
        assert_eq!(status, 400);
    }
    // A secret in a comment is redacted before it is kept.
    let secret_label = format!("ghp_{}", "a1".repeat(19));
    let (status, body) = app
        .call(
            "POST",
            &format!("{base}/comment"),
            Some(json!({ "merge_commit": seen, "text": format!("rotate {secret_label} now") })),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["review_state"], "commented");
    let stored = app.core.store.crew_merge(&task).unwrap().unwrap();
    assert!(!stored.comment.unwrap().contains(&secret_label));
    // A short prefix of the commit the person saw is enough; the answer is recorded.
    let (status, _) = app
        .call(
            "POST",
            &format!("{base}/looks-good"),
            Some(json!({ "merge_commit": &seen[..12] })),
        )
        .await;
    assert_eq!(status, 400, "a prefix is not the commit the person saw");
    let (status, body) = app
        .call(
            "POST",
            &format!("{base}/looks-good"),
            Some(json!({ "merge_commit": seen })),
        )
        .await;
    assert_eq!(
        (status, body["review_state"].as_str()),
        (200, Some("looks_good"))
    );
    // Wrong method.
    let (status, _) = app.call("GET", &format!("{base}/revert"), None).await;
    assert_eq!(status, 405);
}

#[cfg(unix)]
#[tokio::test]
async fn a_revert_is_refused_by_the_core_when_the_branch_is_checked_out_and_when_it_is_done() {
    let app = serve("revert", true, false).await;
    let (task, _) = seed_task(&app.core, "revert");
    let repo = app.dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| fixtures::git(&repo, args);
    git(&["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "first"]);
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let tip = git(&["rev-parse", "HEAD"]);
    let mut record = merge_record(&task, &repo.to_string_lossy(), &branch, "plain");
    record.merge_commit = tip.clone();
    app.core.store.add_crew_merge(&record).unwrap();
    let path = format!("/api/crew/autopilot/review/{task}/revert");
    let body = json!({ "merge_commit": tip });
    let (status, answer) = app.call("POST", &path, Some(body.clone())).await;
    assert_eq!(status, 400, "the branch is checked out");
    assert!(answer["message"].as_str().unwrap().contains("checked out"));
    assert_eq!(git(&["rev-parse", "HEAD"]), tip, "nothing moved");
    let stored = app.core.store.crew_merge(&task).unwrap().unwrap();
    assert_eq!(stored.review_state, "waiting");
    // The check a person reads first lists the later merges.
    let (status, check) = app
        .call(
            "GET",
            &format!("/api/crew/autopilot/review/{task}/revert-check"),
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(check["already_reverted"], false);
    assert!(check["later_merges"].as_array().unwrap().is_empty());
    // A merge that was reverted cannot be reverted again.
    app.core
        .store
        .set_merge_reverted(&task, &"e".repeat(40))
        .unwrap();
    let (status, answer) = app.call("POST", &path, Some(body)).await;
    assert_eq!(status, 400);
    assert!(answer["message"].as_str().unwrap().contains("already"));
}

#[tokio::test]
async fn no_response_holds_a_secret_or_the_answer_key() {
    let app = serve("leaks", true, false).await;
    let (task, key_text) = seed_task(&app.core, "leaks");
    let secret_label = format!("ghp_{}", "b2".repeat(19));
    let text = format!("ran {key_text} with {secret_label}");
    let repo = app.dir.join("repo-leaks");
    app.core
        .store
        .add_crew_merge(&merge_record(&task, &repo.to_string_lossy(), "main", &text))
        .unwrap();
    app.core
        .store
        .add_merge_entry(&fixtures::merge_entry(
            &task,
            &repo.to_string_lossy(),
            &text,
        ))
        .unwrap();
    // Even a ledger row stored before the scrub existed is cleaned on the way out.
    app.core
        .store
        .add_crew_autopilot_event(
            &task,
            "crew-1",
            "denied",
            &json!({ "decided_by": "hard_rule", "rule": "N7", "action": { "kind": "command", "target": text, "paths": [text] } }),
        )
        .unwrap();
    let key_id = app.core.store.crew_answer_key(&task).unwrap().unwrap().id;
    for path in [
        format!("/api/crew/autopilot/risk/{task}"),
        "/api/crew/autopilot/review".to_owned(),
        "/api/crew/autopilot/merge-queue".to_owned(),
        "/api/crew/autopilot/watch".to_owned(),
        "/api/crew/autopilot/digest".to_owned(),
        "/api/crew/approvals".to_owned(),
        "/api/crew/stops".to_owned(),
        "/api/crew/trust".to_owned(),
    ] {
        let (status, body) = app.call("GET", &path, None).await;
        assert_eq!(status, 200, "{path}");
        let text = body.to_string();
        assert!(!text.contains(&key_text), "{path} holds the key's check");
        assert!(!text.contains(key_id.as_str()), "{path} holds the key's id");
        assert!(
            !text.contains(&secret_label),
            "{path} holds the secret label"
        );
    }
    let (_, log) = app.call("GET", "/api/crew/approvals", None).await;
    assert_eq!(log["total"], 1);
    assert_eq!(log["rows"][0]["rule"], "N7");
}

#[tokio::test]
async fn the_review_queue_and_the_log_page() {
    let app = serve("pages", true, false).await;
    let (task, _) = seed_task(&app.core, "pages");
    app.core
        .store
        .add_crew_merge(&merge_record(&task, "/nowhere", "main", "plain"))
        .unwrap();
    for n in 0..5 {
        app.core
            .store
            .add_crew_autopilot_event(
                &task,
                "crew-1",
                "denied",
                &json!({ "decided_by": "hard_rule", "n": n }),
            )
            .unwrap();
    }
    let (_, queue) = app
        .call("GET", "/api/crew/autopilot/review?limit=1", None)
        .await;
    assert_eq!(queue["items"].as_array().unwrap().len(), 1);
    assert_eq!(queue["total"], 1);
    assert!(queue["next_offset"].is_null());
    let (_, first) = app.call("GET", "/api/crew/approvals?limit=2", None).await;
    assert_eq!(first["rows"].as_array().unwrap().len(), 2);
    assert_eq!(first["total"], 5);
    assert_eq!(first["next_offset"], 2);
    let (_, last) = app
        .call("GET", "/api/crew/approvals?limit=2&offset=4", None)
        .await;
    assert_eq!(last["rows"].as_array().unwrap().len(), 1);
    assert!(last["next_offset"].is_null());
    let (_, only) = app
        .call("GET", "/api/crew/approvals?kind=allowed", None)
        .await;
    assert_eq!(only["total"], 0);
    // The trip wire view counts what the ledger holds.
    let (_, wires) = app.call("GET", "/api/crew/stops", None).await;
    assert_eq!(wires["in_a_row_limit"], 3);
    let (_, digest) = app
        .call("GET", "/api/crew/autopilot/digest?hours=720", None)
        .await;
    assert_eq!(digest["merged_count"], 1);
    assert_eq!(digest["waiting_review"], 1);
    assert_eq!(digest["denied"], 5);
    assert_eq!(digest["quiet"], false);
}

#[test]
fn the_wire_types_round_trip_and_carry_no_key_or_secret_field() {
    let modes = wire::AutopilotModes {
        confirmed: true,
        effective_default: wire::AutopilotMode::Full,
        entries: vec![wire::AutopilotModeEntry {
            scope: wire::AutopilotScopeKind::Crew,
            id: Some("crew-1".into()),
            mode: wire::AutopilotMode::Off,
            updated_at: "2026-11-12T10:00:00Z".into(),
        }],
        truncated: false,
    };
    let text = serde_json::to_string(&modes).unwrap();
    assert!(text.contains("\"effective_default\":\"full\""));
    assert!(text.contains("\"scope\":\"crew\""));
    assert_eq!(
        serde_json::from_str::<wire::AutopilotModes>(&text).unwrap(),
        modes
    );
    let change: wire::AutopilotModeChange =
        serde_json::from_value(json!({ "scope": "default", "mode": "on" })).unwrap();
    assert_eq!(change.id, None);
    assert!(
        serde_json::from_value::<wire::AutopilotModeChange>(json!({ "scope": "x", "mode": "on" }))
            .is_err()
    );
    let body: wire::AutopilotAnswerBody =
        serde_json::from_value(json!({ "merge_commit": "abc1234" })).unwrap();
    assert_eq!(body.text, None);
    let row = wire::AutopilotRate {
        merges: 12,
        reverts: 3,
        revert_permille: 250,
        counts: true,
    };
    assert_eq!(
        serde_json::from_value::<wire::AutopilotRate>(serde_json::to_value(&row).unwrap()).unwrap(),
        row
    );
}
