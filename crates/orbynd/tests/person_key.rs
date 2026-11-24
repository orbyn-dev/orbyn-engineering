//! The person key (#441, `docs/worker-token-isolation.md`) through the real service and a fake engine spawned the way
//! a run spawns one. The engine reads `<data>/service.json` and calls the two enforced `Person` routes with the service
//! token alone, as any process of the same user can. Those answer 403; the same calls with the key the shell got on the
//! ready line pass. Nothing a file, env or argv reader can see holds the key.
//!
//! The development override (`ORBYN_DEV_DISABLE_PERSON_KEY`) is on for every other test (`support`), and these tests set it
//! to `0`. Debug builds only: a release build has no override, which is the point.

mod support;

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git};

const KEY_HEADER: &str = "x-orbyn-person";

fn repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    std::fs::write(path.join("a.txt"), "one\n").unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "first"]);
}

/// The `[method, path, body]` of every `Person` route in `tests/routes.json`, path parameters filled with `x`. Step 3
/// enforces every one of them (`routes/tests.rs` fails for a `Person` route that is not enforced), so this is the enforced set.
fn person_routes() -> Vec<(String, String, String)> {
    let table: Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/routes.json"))
            .unwrap(),
    )
    .unwrap();
    let mut routes = Vec::new();
    for row in table["protected"].as_array().unwrap() {
        if row[2] != "person" {
            continue;
        }
        let mut path = String::new();
        let mut inside = false;
        for c in row[1].as_str().unwrap().chars() {
            match c {
                '{' => {
                    inside = true;
                    path.push('x');
                }
                '}' => inside = false,
                _ if !inside => path.push(c),
                _ => {}
            }
        }
        routes.push((row[0].as_str().unwrap().to_owned(), path, "{}".to_owned()));
    }
    routes
}

fn options_probing(
    dir: &TempDir,
    desktop: bool,
    routes: Option<&[(String, String, String)]>,
) -> Options {
    let probe_routes = routes
        .map(|routes| {
            vec![(
                "ORBYN_FAKE_PROBE_ROUTES".to_owned(),
                json!(routes).to_string(),
            )]
        })
        .unwrap_or_default();
    Options {
        args: if desktop {
            vec!["--parent-stdin".into()]
        } else {
            vec![]
        },
        env: vec![
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            // The most the service says, so the log check below reads a real log.
            ("ORBYN_LOG".into(), "debug".into()),
            // A test may put a stand-in `gh` in `<dir>/bin` after the service has started.
            (
                "PATH".into(),
                format!(
                    "{}:{}",
                    dir.path().join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
            (
                "ORBYN_FAKE_PROBE_FILE".into(),
                dir.path().join("probe.json").to_string_lossy().into_owned(),
            ),
            (
                "ORBYN_FAKE_PROBE_DATA".into(),
                dir.path().join("data").to_string_lossy().into_owned(),
            ),
        ]
        .into_iter()
        .chain(probe_routes)
        .collect(),
    }
}

struct Fixture {
    dir: TempDir,
    service: Service,
    project: String,
    worktree: String,
    thread: String,
}

async fn fixture(desktop: bool) -> Fixture {
    fixture_probing(desktop, None).await
}

async fn fixture_probing(desktop: bool, routes: Option<&[(String, String, String)]>) -> Fixture {
    let dir = TempDir::new();
    let folder = dir.path().join("app");
    repo(&folder);
    // Setting up a workspace is a person action. A service with no shell has no key, so a first service with the
    // development override does the setup on the same data dir and stops before the real one starts.
    let setup = if desktop {
        options_probing(&dir, desktop, routes)
    } else {
        Options::default()
    };
    let service = Service::start(&dir.path().join("data"), setup);
    let project = service.project(&folder, true).await;
    let listed = service
        .json(&format!("/api/projects/{project}/worktrees"))
        .await;
    let worktree = listed["worktrees"][0]["id"].as_str().unwrap().to_owned();
    let thread = service.thread(&project, "claude").await;
    let service = if desktop {
        service
    } else {
        drop(service);
        Service::start(
            &dir.path().join("data"),
            options_probing(&dir, desktop, routes),
        )
    };
    Fixture {
        dir,
        service,
        project,
        worktree,
        thread,
    }
}

impl Fixture {
    fn file_actions(&self) -> String {
        format!(
            "/api/projects/{}/worktrees/{}/file-actions",
            self.project, self.worktree
        )
    }

    /// One POST with the service token and, when given, the person key header.
    async fn call(&self, path: &str, body: Value, key: Option<&str>) -> (u16, Value) {
        let mut request = self.service.post(path).json(&body);
        if let Some(key) = key {
            request = request.header(KEY_HEADER, key);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// Waits for the fake engine's report of what it did with the token alone. The engine writes it whole after
    /// every call (`complete: false`, naming the route it is on) and once more when the sweep is done, so a probe a
    /// slow service is still holding says how far it got instead of leaving nothing. Gives up after a minute and
    /// says why: whether the file never appeared or could not be read as JSON, how far the sweep got and the route it
    /// was on, the run's own state, and what the test dir holds.
    async fn probe(&self) -> Value {
        let path = self.dir.path().join("probe.json");
        let mut last = "the file never appeared".to_owned();
        for _ in 0..6000 {
            match std::fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(report) => {
                        if let Some(fatal) = report["fatal"].as_str() {
                            panic!(
                                "the fake engine's probe failed before any call finished: {fatal}"
                            );
                        }
                        if report["complete"] == json!(true) {
                            return report;
                        }
                        last = progress(&report);
                    }
                    Err(error) => last = format!("the file is not JSON yet: {error}"),
                },
                Err(error) => last = format!("read {}: {error}", path.display()),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let state = self.run_state_naming_errors().await;
        let listing: Vec<String> = std::fs::read_dir(self.dir.path())
            .map(|dir| {
                dir.flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        panic!(
            "the fake engine wrote no complete probe report in 60s ({last}); the run is {state}; the test dir holds \
             {listing:?}. The engine was admitted with a thread, so it started or it did not: a service that is down, a \
             run that never spawned the engine, and a request the service never answered look the same from here"
        );
    }

    /// The thread's last run state for a failure message, or why it could not be read. Never panics: the service may
    /// be the thing that is wrong.
    async fn run_state_naming_errors(&self) -> String {
        let path = format!("/api/threads/{}", self.thread);
        match tokio::time::timeout(Duration::from_secs(2), self.service.get(&path).send()).await {
            Ok(Ok(response)) => match response.json::<Value>().await {
                Ok(thread) => thread["last_run_state"]
                    .as_str()
                    .unwrap_or("unreadable")
                    .to_owned(),
                Err(error) => format!("unreadable: {error}"),
            },
            Ok(Err(error)) => format!("unknown, the service did not answer: {error}"),
            Err(_) => "unknown, the service did not answer in 2s".to_owned(),
        }
    }
}

/// How far a sweep that has not finished got, and the route it was on when the report was last written.
fn progress(report: &Value) -> String {
    let calls = report["calls"].as_array();
    let finished = calls.map_or(0, Vec::len);
    let failed = calls.map_or(0, |calls| {
        calls
            .iter()
            .filter(|call| call["status"] != 403 || call["code"] != "person_required")
            .count()
    });
    format!(
        "the engine had finished {finished} calls, {failed} of them not a refusal, and was on {}",
        report["in_flight"].as_str().unwrap_or("an unknown route"),
    )
}

/// Replaces the secrets and any long token-looking run in text a failure message shows, and cuts it to 500 chars.
fn redacted(text: &str, secrets: &[&str]) -> String {
    let mut text = text.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        text = text.replace(secret, "[redacted]");
    }
    let mut out = String::new();
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            run.push(c);
            continue;
        }
        out.push_str(if run.len() >= 32 { "[redacted]" } else { &run });
        run.clear();
        out.push(c);
    }
    out.pop();
    out.chars().take(500).collect()
}

/// Every call of a probe report that is not a refusal with `person_required`, described in full, or the report's own
/// count problem. Empty when the report is as expected.
fn probe_failures(report: &Value, expected: usize) -> Vec<String> {
    let calls = report["calls"].as_array().cloned().unwrap_or_default();
    let mut failures = Vec::new();
    if calls.len() != expected {
        failures.push(format!(
            "the engine reported {} calls, not {expected}{}; the report holds fatal={}, error calls={}",
            calls.len(),
            report["aborted"]
                .as_str()
                .map(|why| format!(" (it stopped after {why})"))
                .unwrap_or_default(),
            report["fatal"],
            calls.iter().filter(|c| c.get("error").is_some()).count()
        ));
    }
    for call in &calls {
        if call["status"] == 403 && call["code"] == "person_required" {
            continue;
        }
        failures.push(format!(
            "token-only probe: route {} {}; sent {}; attempt {}; status {}; code {}; transport error {}; body {}",
            call["method"],
            call["path"],
            call["request"],
            call["attempt"],
            call["status"],
            call["code"],
            call["error"],
            call["body"],
        ));
    }
    failures
}

#[tokio::test]
async fn an_engine_with_only_the_service_token_is_refused_and_the_shells_key_passes() {
    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    assert_eq!(key.len(), 64, "a 32-byte hex key");
    assert_ne!(key, f.service.token, "the key is not the token");

    // The explorer refuses a change while a session is active in the worktree, so its calls come first.
    let create = json!({ "action": "create_file", "path": "made.txt" });
    let (status, body) = f.call(&f.file_actions(), create.clone(), None).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    assert!(!f.dir.path().join("app/made.txt").exists());
    let (status, body) = f.call(&f.file_actions(), create, Some(&key)).await;
    assert_eq!(status, 204, "{body}");
    assert!(f.dir.path().join("app/made.txt").exists());

    // The engine runs as the person: it reads service.json and has the token, and nothing else.
    f.service.admit(&f.thread, "[approve]").await;
    let report = f.probe().await;
    let calls = report["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 3, "{report}");
    for call in calls {
        assert_eq!(call["status"], 403, "{call}");
        assert_eq!(call["code"], "person_required", "{call}");
    }

    // The same calls from here, token only: still refused, and nothing happened.
    let approval = f.service.pending().await;
    let id = approval["id"].as_str().unwrap().to_owned();
    let decide = format!("/api/approvals/{id}/decide");
    let allow = json!({ "allow": true, "remember": false });
    let (status, body) = f.call(&decide, allow.clone(), None).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    let (status, body) = f.call(&decide, allow.clone(), Some(&"0".repeat(64))).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    let (status, body) = f.call(&decide, allow, Some(&key[..63])).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    assert_eq!(f.service.run_state(&f.thread).await, "awaiting_approval");
    // `decide` with the key is covered by the key-nowhere test below; here the approval is answered through the AG-UI resume.

    // Local routes still answer to the token alone.
    let local = f.service.get("/api/approvals?state=pending").send().await;
    assert_eq!(local.unwrap().status(), 200);

    // The AG-UI resume decides the same approval, so it needs the key too. A plain run on that route does not.
    let resume = json!({
        "threadId": f.thread, "runId": "r1",
        "resume": [{ "interruptId": id, "status": "resolved", "payload": { "approved": true } }],
    });
    let agui = format!("/agui/threads/{}", f.thread);
    for sent in [None, Some("0".repeat(64))] {
        let (status, body) = f.call(&agui, resume.clone(), sent.as_deref()).await;
        assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    }
    assert_eq!(f.service.run_state(&f.thread).await, "awaiting_approval");
    assert_eq!(
        f.service.pending().await["id"],
        id.as_str(),
        "still pending"
    );
    let plain = json!({ "threadId": f.thread, "runId": "r2", "resume": [] });
    let (status, _) = f.call(&agui, plain, None).await;
    assert_ne!(status, 403, "a plain run on the AG-UI route stays Local");

    // With the key, the AG-UI resume answers the interrupt (a stream, so read the answer from the thread).
    let response = f
        .service
        .post(&agui)
        .header(KEY_HEADER, &key)
        .json(&resume)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    drop(response);
    assert_eq!(f.service.finished(&f.thread).await, "completed");
}

#[tokio::test]
async fn a_service_without_a_desktop_shell_has_no_key_and_refuses_person_routes() {
    let f = fixture(false).await;
    assert!(f.service.person_key.is_none(), "no shell, no key");
    for (path, body) in [
        (
            f.file_actions(),
            json!({ "action": "create_file", "path": "made.txt" }),
        ),
        (
            "/api/approvals/a/decide".to_owned(),
            json!({ "allow": true, "remember": false }),
        ),
    ] {
        // Whatever header is sent, there is nothing to match.
        for key in [None, Some("0".repeat(64))] {
            let (status, reply) = f.call(&path, body.clone(), key.as_deref()).await;
            assert_eq!(
                (status, &reply["code"]),
                (403, &json!("desktop_required")),
                "{path}"
            );
        }
    }
    assert!(!f.dir.path().join("app/made.txt").exists());
}

/// A `Person` route that is a `GET` is reachable with `HEAD` too, because the router answers `HEAD` from the `GET` handler. The
/// key is still needed: a `HEAD` with the token alone must not learn whether the secret pattern exists.
#[tokio::test]
async fn a_head_on_a_person_get_route_is_refused_without_the_key() {
    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell has a key");
    let path = "/api/settings/secrets/patterns/x";
    let url = format!("{}{path}", f.service.url);
    let client = reqwest::Client::new();
    for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
        let refused = client
            .request(method.clone(), &url)
            .bearer_auth(&f.service.token)
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status().as_u16(), 403, "{method} without the key");
        let kept = client
            .request(method.clone(), &url)
            .bearer_auth(&f.service.token)
            .header(KEY_HEADER, &key)
            .send()
            .await
            .unwrap();
        assert_ne!(kept.status().as_u16(), 403, "{method} with the key");
    }
}

/// A refused `Person` call writes one audit row through the real service: the route pattern and the caller class, with no
/// body, header or token, and a second refusal of the same route inside the minute adds none. The refusal itself is unchanged.
#[tokio::test]
async fn a_refused_person_call_leaves_one_audit_row_without_the_request() {
    let f = fixture(true).await;
    let path = f.file_actions();
    let body = json!({ "action": "create_file", "path": "made-secret-name.txt" });
    for _ in 0..3 {
        let (status, reply) = f.call(&path, body.clone(), Some("not-the-key")).await;
        assert_eq!((status, &reply["code"]), (403, &json!("person_required")));
    }
    let rows: Vec<Value> = f
        .service
        .json("/api/audit?limit=200")
        .await
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["action"] == "person_route.refused")
        .cloned()
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(
        row["target"],
        "POST /api/projects/{id}/worktrees/{worktree}/file-actions"
    );
    assert_eq!(
        (row["actor"].as_str(), row["decision"].as_str()),
        (Some("service_token"), Some("deny"))
    );
    assert_eq!(row["detail"]["code"], "person_required");
    assert!(row["ts"].as_str().is_some_and(|ts| !ts.is_empty()));
    let text = row.to_string();
    for secret in [
        f.service.token.as_str(),
        "not-the-key",
        "made-secret-name",
        f.project.as_str(),
        f.worktree.as_str(),
    ] {
        assert!(!text.contains(secret), "{secret} is in the row: {text}");
    }
}

impl Fixture {
    /// The `person_route.refused` audit rows so far.
    async fn refusal_rows(&self) -> Vec<Value> {
        self.service
            .json("/api/audit?limit=200")
            .await
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["action"] == "person_route.refused")
            .cloned()
            .collect()
    }

    /// The one row for `target`, after checking that nothing sent is in it.
    fn only_row<'a>(&self, rows: &'a [Value], target: &str, absent: &[&str]) -> &'a Value {
        let matching: Vec<&Value> = rows.iter().filter(|row| row["target"] == target).collect();
        assert_eq!(matching.len(), 1, "{target}: {rows:?}");
        let row = matching[0];
        assert_eq!(
            (row["actor"].as_str(), row["decision"].as_str()),
            (Some("service_token"), Some("deny"))
        );
        assert_eq!(row["detail"]["code"], "person_required");
        let text = row.to_string();
        for secret in [
            self.service.token.as_str(),
            "not-the-key",
            self.project.as_str(),
            self.thread.as_str(),
        ]
        .into_iter()
        .chain(absent.iter().copied())
        {
            assert!(!text.contains(secret), "{secret} is in the row: {text}");
        }
        row
    }
}

/// A refusal made inside a handler for part of the request (`FIELD_LEVEL`) leaves the same audit row as a route-level one,
/// with the field's name in the target and no value, once per route, field and caller a minute. The three call sites are
/// the run's permission mode, the AG-UI resume and the pull checkout.
#[tokio::test]
async fn a_refused_handler_level_check_leaves_one_audit_row_with_the_field_name_only() {
    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");

    // Runs: raising the permission mode above the thread's own.
    let runs = format!("/api/threads/{}/runs", f.thread);
    let raise = json!({ "prompt": "raise-secret-prompt", "permission_mode": "full_access" });
    // The AG-UI resume.
    let agui = format!("/agui/threads/{}", f.thread);
    let resume = json!({
        "threadId": f.thread, "runId": "r1",
        "resume": [{ "interruptId": "resume-secret-id", "status": "resolved", "payload": { "approved": true } }],
    });
    // The pull checkout that moves the project's own checkout.
    let checkout = format!("/api/projects/{}/pulls/12/checkout", f.project);
    for _ in 0..3 {
        for (path, body) in [
            (&runs, raise.clone()),
            (&agui, resume.clone()),
            (&checkout, json!({ "worktree": false })),
        ] {
            let (status, reply) = f.call(path, body, Some("not-the-key")).await;
            assert_eq!(
                (status, &reply["code"]),
                (403, &json!("person_required")),
                "{path}"
            );
        }
    }
    let rows = f.refusal_rows().await;
    assert_eq!(rows.len(), 3, "{rows:?}");
    let sent = ["raise-secret-prompt", "resume-secret-id", "full_access"];
    let run_row = f.only_row(
        &rows,
        "POST /api/threads/{id}/runs [permission_mode]",
        &sent,
    );
    f.only_row(&rows, "POST /agui/threads/{id} [resume]", &sent);
    f.only_row(
        &rows,
        "POST /api/projects/{id}/pulls/{number}/checkout [worktree]",
        &sent,
    );
    assert_eq!(run_row["detail"]["field"], "permission_mode");
    assert!(run_row["ts"].as_str().is_some_and(|ts| !ts.is_empty()));

    // The same calls that stay Local write nothing, and the key still passes.
    let (status, reply) = f.call(&runs, json!({ "prompt": "[approve]" }), None).await;
    assert_ne!(status, 403, "{reply}");
    let (status, _) = f
        .call(
            &agui,
            json!({ "threadId": f.thread, "runId": "r2", "resume": [] }),
            None,
        )
        .await;
    assert_ne!(status, 403);
    assert_eq!(f.refusal_rows().await.len(), 3);
    let (status, reply) = f
        .call(&checkout, json!({ "worktree": false }), Some(&key))
        .await;
    assert_ne!(status, 403, "{reply}");
}

/// The window is shared: a route-level and a handler-level refusal each write one row, and repeating either, in any order,
/// adds none.
#[tokio::test]
async fn route_level_and_handler_level_refusals_share_the_minute() {
    let f = fixture(true).await;
    let checkout = format!("/api/projects/{}/pulls/12/checkout", f.project);
    let create = json!({ "action": "create_file", "path": "made.txt" });
    for _ in 0..2 {
        let (status, _) = f.call(&f.file_actions(), create.clone(), None).await;
        assert_eq!(status, 403);
        let (status, _) = f.call(&checkout, json!({ "worktree": false }), None).await;
        assert_eq!(status, 403);
    }
    let rows = f.refusal_rows().await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let mut targets: Vec<&str> = rows
        .iter()
        .map(|row| row["target"].as_str().unwrap())
        .collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        [
            "POST /api/projects/{id}/pulls/{number}/checkout [worktree]",
            "POST /api/projects/{id}/worktrees/{worktree}/file-actions",
        ]
    );
}

/// Every file under `root`, read whole, for the bytes of `needle`.
fn files_holding(root: &Path, needle: &[u8], found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_holding(&path, needle, found);
        } else if let Ok(bytes) = std::fs::read(&path)
            && bytes.windows(needle.len()).any(|window| window == needle)
        {
            found.push(path.display().to_string());
        }
    }
}

#[tokio::test]
async fn the_key_is_in_no_file_env_argv_or_log() {
    let mut f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    f.service.admit(&f.thread, "[approve]").await;
    let report = f.probe().await;
    let approval = f.service.pending().await;
    // Use the key once, so a leak that happens only when it is used would show.
    let id = approval["id"].as_str().unwrap().to_owned();
    let (status, body) = f
        .call(
            &format!("/api/approvals/{id}/decide"),
            json!({ "allow": false, "remember": false }),
            Some(&key),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    f.service.finished(&f.thread).await;

    // The data folder, which a confined worker cannot read and an unconfined one can.
    let mut found = Vec::new();
    files_holding(&f.dir.path().join("data"), key.as_bytes(), &mut found);
    assert!(found.is_empty(), "the key is on disk: {found:?}");

    // The engine's own argv and env, as it saw them.
    let argv = report["argv"].to_string();
    assert!(!argv.contains(&key), "engine argv holds the key");
    let env = report["env"].as_array().unwrap();
    assert!(!env.is_empty());
    assert!(
        env.iter()
            .all(|line| !line.as_str().unwrap().contains(&key)),
        "engine env holds the key"
    );

    // The service's own argv and env, from the process table.
    #[cfg(unix)]
    {
        let shown = std::process::Command::new("ps")
            .args([
                "eww",
                "-p",
                &f.service.child.id().to_string(),
                "-o",
                "command=",
            ])
            .output()
            .unwrap();
        let shown = String::from_utf8_lossy(&shown.stdout);
        assert!(shown.contains("orbynd"), "ps showed the service: {shown}");
        assert!(
            !shown.contains(&key),
            "the service's argv or env holds the key"
        );
    }

    // The service's log (stderr), after it has stopped and written everything.
    f.service.kill();
    let mut log = String::new();
    f.service
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();
    assert!(!log.is_empty(), "the service logged something");
    assert!(!log.contains(&key), "the service log holds the key");
    // The token is on disk for the CLI; the key must not travel with it.
    let handle =
        std::fs::read_to_string(f.dir.path().join("data/service.json")).unwrap_or_default();
    assert!(handle.is_empty() || !handle.contains("person"));
}

/// Step 3: every `Person` route, called by an engine with the service token alone, is refused; and the same calls with
/// the shell's key are not refused for want of the key. A request with the key reaches the handler, which answers for the
/// fake ids and empty body with whatever it does (a 4xx), so only the person codes are ruled out; the other answers are the
/// routes' own tests. The ids are not real, so no handler can act.
#[tokio::test]
async fn every_person_route_refuses_a_token_only_engine_and_takes_the_key() {
    let routes = person_routes();
    assert!(routes.len() > 90, "the table has the person rows");
    let f = fixture_probing(true, Some(&routes)).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    f.service.admit(&f.thread, "hello").await;
    let report = f.probe().await;
    let failures = probe_failures(&report, routes.len());
    assert!(
        failures.is_empty(),
        "{} of {} token-only probes failed:\n{}",
        failures.len(),
        routes.len(),
        failures.join("\n")
    );

    // The sockets are GET routes: without an upgrade they are refused the same way, and the key as a subprotocol opens one.
    let sockets: Vec<&(String, String, String)> = routes
        .iter()
        .filter(|(m, p, _)| m == "GET" && p.contains("/terminal"))
        .collect();
    assert_eq!(sockets.len(), 2, "the two terminal sockets");
    let mut failures = Vec::new();
    for (_, path, _) in &sockets {
        match f.service.get(path).send().await {
            Ok(refused) if refused.status() == 403 => {}
            Ok(refused) => failures.push(format!(
                "socket GET {path} without an upgrade: status {}, not 403",
                refused.status()
            )),
            Err(error) => failures.push(format!("socket GET {path}: transport error {error}")),
        }
    }

    // With the key, the person layer lets each one through. The route's own answer follows (not found, invalid and so on).
    // Routes whose handler takes no body and acts on the machine or the network (updates, the skills workspace, every
    // engine's update) are not called with the key here: an empty request could do real work. The layer's behavior for them is
    // covered with a stub handler in `routes/classes.rs` (`the_layer_guards_every_enforced_route`).
    const SKIPPED: [&str; 7] = [
        "/api/updates/app/apply",
        "/api/updates/app/download",
        "/api/updates/apply-all",
        "/api/engines/update-all",
        "/api/workspace/update",
        "/api/workspace/setup",
        "/api/workspace/rollback",
    ];
    for (method, path, body) in routes
        .iter()
        .filter(|(m, p, _)| m != "GET" && !SKIPPED.contains(&p.as_str()))
    {
        let verb = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let url = format!("{}{path}", f.service.url);
        let sent = format!(
            "{method} {url} (headers: authorization, {KEY_HEADER}, content-type; body {} bytes)",
            body.len()
        );
        let secrets = [f.service.token.as_str(), key.as_str()];
        let response = match f
            .service
            .client
            .request(verb, &url)
            .bearer_auth(&f.service.token)
            .header(KEY_HEADER, &key)
            .header("content-type", "application/json")
            .body(body.clone())
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                failures.push(format!(
                    "keyed call: route {method} {path}; sent {sent}; attempt 1; transport error {}",
                    redacted(&error.to_string(), &secrets)
                ));
                continue;
            }
        };
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let reply: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if matches!(
            reply["code"].as_str(),
            Some("person_required" | "desktop_required")
        ) {
            failures.push(format!(
                "keyed call refused the right key: route {method} {path}; sent {sent}; attempt 1; status {status}; body {}",
                redacted(&text, &secrets)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failed probes:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn current_branch(folder: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(folder)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn has_branch(folder: &Path, name: &str) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")])
        .current_dir(folder)
        .output()
        .unwrap()
        .status
        .success()
}

/// Ruling F3: the routes that move a checkout need the person key. A call with the service token alone is refused with
/// `person_required` and changes nothing; the same call with the shell's key works.
#[tokio::test]
async fn the_routes_that_move_a_checkout_refuse_a_token_only_caller_and_take_the_key() {
    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    let folder = f.dir.path().join("app");
    let refused = |status: u16, body: &Value| {
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("person_required")),
            "{body}"
        );
    };

    // POST /api/threads/{id}/branches
    let branches = format!("/api/threads/{}/branches", f.thread);
    let (status, body) = f.call(&branches, json!({ "name": "feature" }), None).await;
    refused(status, &body);
    assert!(!has_branch(&folder, "feature"));
    assert_eq!(current_branch(&folder), "main");
    let (status, body) = f
        .call(&branches, json!({ "name": "feature" }), Some(&key))
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(current_branch(&folder), "feature");

    // POST /api/threads/{id}/branches/switch
    let switch = format!("{branches}/switch");
    let (status, body) = f.call(&switch, json!({ "name": "main" }), None).await;
    refused(status, &body);
    assert_eq!(current_branch(&folder), "feature");
    let (status, body) = f.call(&switch, json!({ "name": "main" }), Some(&key)).await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(current_branch(&folder), "main");

    // POST /api/threads/{id}/vcs/refs
    let refs = format!("/api/threads/{}/vcs/refs", f.thread);
    let (status, body) = f.call(&refs, json!({ "name": "topic" }), None).await;
    refused(status, &body);
    assert!(!has_branch(&folder, "topic"));
    let (status, body) = f.call(&refs, json!({ "name": "topic" }), Some(&key)).await;
    assert_eq!(status, 201, "{body}");
    assert!(has_branch(&folder, "topic"));
    assert_eq!(
        current_branch(&folder),
        "main",
        "creating a ref does not switch"
    );

    // POST /api/threads/{id}/vcs/refs/switch
    let switch = format!("{refs}/switch");
    let (status, body) = f.call(&switch, json!({ "name": "topic" }), None).await;
    refused(status, &body);
    assert_eq!(current_branch(&folder), "main");
    let (status, body) = f
        .call(&switch, json!({ "name": "topic" }), Some(&key))
        .await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(current_branch(&folder), "topic");

    // GET /api/threads/{id}/branches stays Local.
    let listed = f.service.get(&branches).send().await.unwrap();
    assert_eq!(listed.status(), 200);

    // POST /api/projects/{id}/git/init, on a trusted folder that is not a repository.
    let plain = f.dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::write(plain.join("a.txt"), "one\n").unwrap();
    let (status, project) = f
        .service
        .send("/api/projects/add", json!({ "path": plain }))
        .await;
    assert_eq!(status, 201, "{project}");
    let id = project["id"].as_str().unwrap().to_owned();
    let (status, body) = f
        .service
        .send(&format!("/api/projects/{id}/trust"), json!({}))
        .await;
    assert_eq!(status, 200, "{body}");
    let init = format!("/api/projects/{id}/git/init");
    let (status, body) = f.call(&init, json!({}), None).await;
    refused(status, &body);
    assert!(!plain.join(".git").exists());
    let (status, body) = f.call(&init, json!({}), Some(&key)).await;
    assert_eq!(status, 200, "{body}");
    assert!(plain.join(".git").exists());
}

/// Ruling F3: a pull checkout with `worktree: false` runs `git switch` in the project's own checkout, so it needs the
/// person key and is refused before the fetch; `worktree: true` leaves that checkout alone and stays `Local`.
#[cfg(unix)]
#[tokio::test]
async fn a_pull_checkout_that_moves_the_main_checkout_needs_the_key() {
    use std::os::unix::fs::PermissionsExt;

    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    let folder = f.dir.path().join("app");

    // A stand-in `gh` that answers `pr view` with a pull whose branch is `feature`, and a bare remote that has it.
    let bin = f.dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = f.dir.path().join("gh-log");
    let gh = format!(
        r#"#!/bin/sh
echo "$@" >> "{}"
case "$1 $2" in
  "pr view") echo '{{"number":12,"title":"T","url":"https://github.com/o/r/pull/12","state":"OPEN","headRefName":"feature","baseRefName":"main","author":{{"login":"me"}},"isDraft":false,"labels":[],"statusCheckRollup":[]}}' ;;
esac
"#,
        log.display()
    );
    std::fs::write(bin.join("gh"), gh).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let remote = f.dir.path().join("remote.git");
    run_git(
        f.dir.path(),
        &["init", "-q", "--bare", remote.to_str().unwrap()],
    );
    run_git(&folder, &["branch", "feature"]);
    run_git(
        &folder,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    run_git(&folder, &["push", "-q", "origin", "feature", "main"]);
    run_git(&folder, &["branch", "-D", "feature"]);
    // A GitHub-shaped origin, so the forge is `gh`; git still fetches from the bare remote.
    run_git(
        &folder,
        &[
            "config",
            &format!("url.{}.insteadOf", remote.display()),
            "https://github.com/o/r.git",
        ],
    );
    run_git(
        &folder,
        &["remote", "set-url", "origin", "https://github.com/o/r.git"],
    );
    let checkout = format!("/api/projects/{}/pulls/12/checkout", f.project);

    // worktree false, token only: refused with nothing changed (no fetch, so no local branch, and the forge is not asked).
    let (status, body) = f.call(&checkout, json!({ "worktree": false }), None).await;
    assert_eq!(
        (status, &body["code"]),
        (403, &json!("person_required")),
        "{body}"
    );
    assert!(!has_branch(&folder, "feature"), "a refused call fetched");
    assert_eq!(current_branch(&folder), "main");
    assert!(std::fs::read_to_string(&log).unwrap_or_default().is_empty());

    // worktree false, with the key: switches the project's checkout.
    let (status, body) = f
        .call(&checkout, json!({ "worktree": false }), Some(&key))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(current_branch(&folder), "feature");
    run_git(&folder, &["switch", "-q", "main"]);

    // worktree true, token only: allowed, and the project's checkout stays where it was.
    let (status, body) = f.call(&checkout, json!({ "worktree": true }), None).await;
    assert_eq!(status, 200, "{body}");
    assert!(
        Path::new(body["path"].as_str().unwrap())
            .join("a.txt")
            .exists()
    );
    assert_eq!(current_branch(&folder), "main");
}

/// A run may keep or lower its thread's permission mode with the service token alone, but raising it is the person's.
/// The middle modes are the control: an equal mode (`auto`) and both lower modes (`auto_edits`, `supervised`) reach the
/// handler, while the one-step-looser mode and an empty or garbage key are refused with `person_required`.
#[tokio::test]
async fn a_run_can_lower_but_not_raise_its_thread_s_permission_mode() {
    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    let runs = format!("/api/threads/{}/runs", f.thread);

    // The thread starts Supervised. With the key, raise it to Auto so the checks below have an equal mode and two
    // lower ones. The PATCH route is Person as a whole, and the shell's key is the person.
    let response = f
        .service
        .client
        .patch(format!("{}/api/threads/{}", f.service.url, f.thread))
        .bearer_auth(&f.service.token)
        .header(KEY_HEADER, &key)
        .json(&json!({ "permission_mode": "auto" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let thread = f.service.json(&format!("/api/threads/{}", f.thread)).await;
    assert_eq!(thread["lifecycle"]["permission_mode"], json!("auto"));

    // FullAccess is one step looser than Auto.
    let raise = json!({ "prompt": "raise", "permission_mode": "full_access" });
    let (status, body) = f.call(&runs, raise.clone(), None).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    // An empty or garbage key is a wrong key, not "no key": both still answer the same refusal.
    for wrong in ["", "garbage"] {
        let (status, body) = f.call(&runs, raise.clone(), Some(wrong)).await;
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("person_required")),
            "{wrong:?}"
        );
    }
    let thread = f.service.json(&format!("/api/threads/{}", f.thread)).await;
    assert_eq!(
        thread["lifecycle"]["permission_mode"],
        json!("auto"),
        "a refused run moved the mode"
    );

    // Equal and lower modes pass the person layer. A handler may refuse for another reason, but not with 403.
    for mode in ["auto", "auto_edits", "supervised"] {
        let (status, body) = f
            .call(
                &runs,
                json!({ "prompt": "stay", "permission_mode": mode }),
                None,
            )
            .await;
        assert_ne!(status, 403, "{mode}: {body}");
        assert_ne!(body["code"], json!("person_required"), "{mode}: {body}");
        f.service.finished(&f.thread).await;
    }
}

/// `CheckoutPull.worktree` has no serde default. A body that omits it is refused by the JSON extractor as
/// `invalid_input` (400) before the handler runs, so no checkout can move. The explicit `worktree: false` is the
/// body that would move the project's own checkout, and that reaches the handler and is refused by the person layer.
#[tokio::test]
async fn an_omitted_checkout_worktree_is_invalid_input_and_never_moves_the_checkout() {
    let f = fixture(true).await;
    let checkout = format!("/api/projects/{}/pulls/12/checkout", f.project);

    let (status, body) = f.call(&checkout, json!({}), None).await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid_input")));

    let (status, body) = f.call(&checkout, json!({ "worktree": false }), None).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
}

/// The person layer reads a non-empty `resume[]` before the handler parses it. A malformed entry is therefore a
/// person-action refusal, not invalid input: a token-only caller cannot answer an interrupt whatever the body says.
#[tokio::test]
async fn a_malformed_agui_resume_is_refused_before_the_body_is_parsed() {
    let f = fixture(true).await;
    let agui = format!("/agui/threads/{}", f.thread);
    let (status, body) = f
        .call(
            &agui,
            json!({ "threadId": &f.thread, "runId": "run-1", "resume": [{}] }),
            None,
        )
        .await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
}

/// A browser cannot set a header on a socket and the key must not be in the URL, so the page offers it as the subprotocol
/// `orbyn.person.<key>` beside `orbyn.v1`. The upgrade answers `orbyn.v1` and never echoes the key.
#[tokio::test]
async fn a_terminal_socket_takes_the_key_as_a_subprotocol_and_never_echoes_it() {
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let f = fixture(true).await;
    let key = f.service.person_key.clone().expect("a shell gets a key");
    let url = format!(
        "{}/ws/terminal/{}?token={}",
        f.service.url.replacen("http", "ws", 1),
        f.thread,
        f.service.token
    );
    let open = |protocols: Option<String>| {
        let mut request = url.clone().into_client_request().unwrap();
        if let Some(protocols) = protocols {
            request.headers_mut().insert(
                "sec-websocket-protocol",
                HeaderValue::from_str(&protocols).unwrap(),
            );
        }
        async move { connect_async(request).await }
    };
    // No key, a wrong key and the token's own value are refused before the upgrade.
    for offered in [
        None,
        Some("orbyn.v1".to_owned()),
        Some(format!("orbyn.v1, orbyn.person.{}", "0".repeat(64))),
        Some(format!("orbyn.v1, orbyn.person.{}", f.service.token)),
    ] {
        let refused = open(offered.clone()).await;
        let Err(tokio_tungstenite::tungstenite::Error::Http(response)) = refused else {
            panic!("{offered:?} was not refused");
        };
        assert_eq!(response.status(), 403, "{offered:?}");
    }
    let (_socket, response) = open(Some(format!("orbyn.v1, orbyn.person.{key}")))
        .await
        .expect("the key opens the socket");
    assert_eq!(response.status(), 101);
    assert_eq!(
        response.headers().get("sec-websocket-protocol").unwrap(),
        "orbyn.v1",
        "the answer selects orbyn.v1 and never the key"
    );
}
