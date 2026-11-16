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

fn options(dir: &TempDir, desktop: bool) -> Options {
    Options {
        args: if desktop {
            vec!["--parent-stdin".into()]
        } else {
            vec![]
        },
        env: vec![
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            // File actions are Preview-only until step 3 of the design.
            ("ORBYN_CREW_PREVIEW".into(), "1".into()),
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
        ],
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
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(&dir, desktop));
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
    assert_eq!(calls.len(), 2, "{report}");
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
    let (status, body) = f.call(&decide, allow.clone(), Some(&key[..63])).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    assert_eq!(f.service.run_state(&f.thread).await, "awaiting_approval");

    // Local routes still answer to the token alone.
    let local = f.service.get("/api/approvals?state=pending").send().await;
    assert_eq!(local.unwrap().status(), 200);

    // With the key, both routes do their work.
    let (status, body) = f.call(&decide, allow, Some(&key)).await;
    assert_eq!(status, 200, "{body}");
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
