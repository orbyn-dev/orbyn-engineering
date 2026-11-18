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
    let service = Service::start(
        &dir.path().join("data"),
        options_probing(&dir, desktop, routes),
    );
    let folder = dir.path().join("app");
    repo(&folder);
    let project = service.project(&folder, true).await;
    let listed = service
        .json(&format!("/api/projects/{project}/worktrees"))
        .await;
    let worktree = listed["worktrees"][0]["id"].as_str().unwrap().to_owned();
    let thread = service.thread(&project, "claude").await;
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

    /// Waits for the fake engine's report of what it did with the token alone.
    async fn probe(&self) -> Value {
        let path = self.dir.path().join("probe.json");
        for _ in 0..1500 {
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(report) = serde_json::from_str(&text)
            {
                return report;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the fake engine wrote no probe report");
    }
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
    let calls = report["calls"].as_array().unwrap();
    assert_eq!(calls.len(), routes.len(), "the engine called every route");
    for call in calls {
        assert_eq!(call["status"], 403, "{call}");
        assert_eq!(call["code"], "person_required", "{call}");
    }

    // The sockets are GET routes: without an upgrade they are refused the same way, and the key as a subprotocol opens one.
    let sockets: Vec<&(String, String, String)> =
        routes.iter().filter(|(m, _, _)| m == "GET").collect();
    assert_eq!(sockets.len(), 2, "the two terminal sockets");
    for (_, path, _) in &sockets {
        let refused = f.service.get(path).send().await.unwrap();
        assert_eq!(refused.status(), 403, "{path}");
    }

    // With the key, the person layer lets each one through. The route's own answer follows (not found, invalid and so on).
    // Routes whose handler takes no body and acts on the machine or the network (updates, the skills workspace, every
    // engine's update) are not called with the key here: an empty request could do real work. The layer's behavior for them is
    // covered with a stub handler in `routes/classes.rs` (`the_layer_guards_every_enforced_route`).
    const SKIPPED: [&str; 6] = [
        "/api/updates/app/apply",
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
        let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let response = f
            .service
            .client
            .request(method, format!("{}{path}", f.service.url))
            .bearer_auth(&f.service.token)
            .header(KEY_HEADER, &key)
            .header("content-type", "application/json")
            .body(body.clone())
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let reply: Value = response.json().await.unwrap_or(Value::Null);
        assert!(
            !matches!(
                reply["code"].as_str(),
                Some("person_required" | "desktop_required")
            ),
            "{path} refused the right key: {status} {reply}"
        );
    }
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
