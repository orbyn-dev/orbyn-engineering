//! Runs the real `orbynd` binary against the fake engine over real processes.
//!
//! Browser tests drive the browser a run gives them: `ORBYN_BROWSER`, or the Playwright
//! headless shell that `npm run test:e2e:install` puts under `web/node_modules`. Without
//! one they print why and pass, so the default checks run on a machine with no browser;
//! `ORBYN_REQUIRE_BROWSER=1` turns that into a failure instead, so a job that should have
//! one cannot hide behind the skip. Giving the CI jobs a browser is issue #1575.

#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Value, json};

/// The browser the browser tests drive, or `None` when this run has none.
///
/// `ORBYN_BROWSER` names one explicitly. Without it, the headless shell the E2E setup
/// downloads into `web/node_modules` is used, so a checkout set up by docs/setup.md runs
/// the browser cases in a normal test run. A missing browser skips silently, because
/// libtest hides a passing test's stderr; a CI job that should run these cases must set
/// `ORBYN_REQUIRE_BROWSER=1` so a missing browser fails instead. Issue #1575 gives the
/// CI jobs a browser.
pub fn browser() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ORBYN_BROWSER") {
        let path = PathBuf::from(path);
        assert!(
            path.is_file(),
            "ORBYN_BROWSER is not a file: {}",
            path.display()
        );
        // The service starts in another folder, so a relative override would name a different file there.
        return Some(std::path::absolute(&path).expect("ORBYN_BROWSER has an absolute form"));
    }
    if let Some(path) = e2e_headless_shell() {
        return Some(path);
    }
    let reason = "no browser: set ORBYN_BROWSER, or run `npm run test:e2e:install` in web (see docs/setup.md)";
    assert!(
        std::env::var("ORBYN_REQUIRE_BROWSER").as_deref() != Ok("1"),
        "ORBYN_REQUIRE_BROWSER=1 and {reason}"
    );
    eprintln!("browser test skipped: {reason}");
    None
}

/// The environment a service needs to use [`browser`], plus any `extra` pairs, or
/// `None` when this run has no browser.
pub fn browser_env(extra: &[(&str, &str)]) -> Option<Vec<(String, String)>> {
    let browser = browser()?;
    let mut env = vec![(
        "ORBYN_BROWSER".to_owned(),
        browser.to_string_lossy().into_owned(),
    )];
    env.extend(
        extra
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
    );
    Some(env)
}

/// The headless shell an E2E setup downloads, newest revision first: installs keep
/// older revisions beside the current one.
fn e2e_headless_shell() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/node_modules/playwright-core/.local-browsers");
    let revisions = folders_with_prefix(&root, "chromium_headless_shell-");
    for revision in revisions.iter().rev() {
        for build in folders_with_prefix(revision, "chrome-headless-shell-") {
            let binary = build.join(format!(
                "chrome-headless-shell{}",
                std::env::consts::EXE_SUFFIX
            ));
            if binary.is_file() {
                return Some(binary);
            }
        }
    }
    None
}

fn folders_with_prefix(root: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        })
        .collect();
    found.sort();
    found
}

/// The form the product stores: on Windows, without the verbatim prefix.
pub fn canonical(path: impl AsRef<Path>) -> PathBuf {
    orbyn_core::util::canonicalize(path.as_ref()).unwrap()
}

pub const FAKE: &str = env!("CARGO_BIN_EXE_orbyn-fake-engine");
const ORBYND: &str = env!("CARGO_BIN_EXE_orbynd");

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("orbynd-test-{}", orbyn_core::util::new_id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(canonical(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
pub struct Options {
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
}

pub struct Service {
    pub url: String,
    pub token: String,
    pub access_key: Option<String>,
    /// The person key from the ready line, which only a service started with `--parent-stdin` prints.
    pub person_key: Option<String>,
    pub client: reqwest::Client,
    pub child: Child,
    /// Everything after the ready line, for tests of the shell channel.
    pub stdout: BufReader<std::process::ChildStdout>,
}

impl Service {
    pub fn start(data: &Path, options: Options) -> Self {
        Self::try_start(data, options).expect("service should start")
    }

    pub fn try_start(data: &Path, options: Options) -> Result<Self, String> {
        let mut command = Command::new(ORBYND);
        command
            .args(["serve", "--data-dir"])
            .arg(data)
            .args(&options.args)
            .env("ORBYN_CLAUDE_BIN", FAKE)
            .env("ORBYN_CODEX_BIN", FAKE)
            .env("ORBYN_OFFLINE", "1")
            // Person routes need the desktop shell's key. Most tests have no shell, so the development
            // override (debug builds only) is on; a test of the key itself sets it to "0".
            .env("ORBYN_DEV_DISABLE_PERSON_KEY", "1")
            .current_dir(data.parent().unwrap_or(data))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // The runner's own Claude session or task-tool switches must not decide a test: a
        // test that needs one asks for it through `options.env`.
        for name in [
            "CLAUDECODE",
            "ORBYN_CLAUDE_TASK_TOOLS",
            "CLAUDE_CODE_ENABLE_TODO_TOOLS",
        ] {
            command.env_remove(name);
        }
        for (key, value) in &options.env {
            command.env(key, value);
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut line = String::new();
        let mut stdout = BufReader::new(stdout);
        stdout.read_line(&mut line).unwrap();
        if line.is_empty() {
            let mut text = String::new();
            std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut text).unwrap();
            let _ = child.wait();
            return Err(text);
        }
        let start: Value = serde_json::from_str(&line).unwrap();
        Ok(Self {
            url: start["url"].as_str().unwrap().to_owned(),
            token: start["token"].as_str().unwrap().to_owned(),
            access_key: start["access_key"].as_str().map(str::to_owned),
            person_key: start["person_key"].as_str().map(str::to_owned),
            client: reqwest::Client::new(),
            child,
            stdout,
        })
    }

    pub fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{}{path}", self.url))
            .bearer_auth(&self.token)
    }

    pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}{path}", self.url))
            .bearer_auth(&self.token)
    }

    pub async fn json(&self, path: &str) -> Value {
        let response = self.get(path).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "{path}: {}",
            response.status()
        );
        response.json().await.unwrap()
    }

    pub async fn patch(&self, thread: &str, body: Value) -> (u16, Value) {
        let response = self
            .client
            .patch(format!("{}/api/threads/{thread}", self.url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    pub async fn send(&self, path: &str, body: Value) -> (u16, Value) {
        // The test acts as the person: with a desktop shell it also presents the shell's key.
        let mut request = self.post(path).json(&body);
        if let Some(key) = &self.person_key {
            request = request.header("x-orbyn-person", key);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// Adopts a generated library in place of the empty one a bare service starts with.
    pub async fn ensure_workspace(&self, parent: &Path) {
        if self.json("/api/workspace").await["library"]["snapshot"] != true {
            return;
        }
        let library = library(&parent.join("library"));
        let (status, body) = self
            .send("/api/workspace/setup", json!({ "adopt_path": library }))
            .await;
        assert_eq!(status, 200, "{body}");
    }

    /// Imports `path` when it is a real repository, otherwise creates a project
    /// with its folder name.
    pub async fn project(&self, path: &Path, trusted: bool) -> String {
        self.ensure_workspace(path.parent().unwrap()).await;
        let name = path.file_name().unwrap().to_str().unwrap();
        let (status, body) = if path.join(".git/HEAD").exists() {
            self.send("/api/projects/add", json!({ "path": path }))
                .await
        } else {
            self.send("/api/projects/create", json!({ "name": name }))
                .await
        };
        assert_eq!(status, 201, "{body}");
        let project = if body.get("project").is_some() {
            body["project"].clone()
        } else {
            body
        };
        let id = project["id"].as_str().unwrap().to_owned();
        if trusted {
            let (status, _) = self
                .send(&format!("/api/projects/{id}/trust"), json!({}))
                .await;
            assert_eq!(status, 200);
        }
        id
    }

    pub async fn thread(&self, project: &str, engine: &str) -> String {
        let (status, thread) = self
            .send(
                "/api/threads",
                json!({
                    "project_id": project, "engine": engine, "model": null, "title": null,
                    "worktree": null, "skill_profile": null,
                }),
            )
            .await;
        assert_eq!(status, 201, "{thread}");
        thread["id"].as_str().unwrap().to_owned()
    }

    pub async fn admit(&self, thread: &str, prompt: &str) -> String {
        let (status, run) = self
            .send(
                &format!("/api/threads/{thread}/runs"),
                json!({ "prompt": prompt }),
            )
            .await;
        assert_eq!(status, 202, "{run}");
        run["id"].as_str().unwrap().to_owned()
    }

    pub async fn run_state(&self, thread: &str) -> Value {
        let thread = self.json(&format!("/api/threads/{thread}")).await;
        thread["last_run_state"].clone()
    }

    /// Waits for the thread's latest run to reach a terminal state.
    pub async fn finished(&self, thread: &str) -> String {
        for _ in 0..1500 {
            let thread_body = self.json(&format!("/api/threads/{thread}")).await;
            if thread_body["active_run"].is_null() {
                return thread_body["last_run_state"].as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("run did not finish");
    }

    pub async fn pending(&self) -> Value {
        for _ in 0..1000 {
            let approvals = self.json("/api/approvals?state=pending").await;
            if approvals.as_array().is_some_and(|list| !list.is_empty()) {
                return approvals[0].clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no approval appeared");
    }

    pub async fn decide(&self, approval: &str, allow: bool) -> (u16, Value) {
        self.send(
            &format!("/api/approvals/{approval}/decide"),
            json!({ "allow": allow, "remember": false }),
        )
        .await
    }

    /// Reads SSE frames until `stop` matches, up to a limit.
    pub async fn events_until(
        &self,
        thread: &str,
        after: Option<i64>,
        stop: impl Fn(&Value) -> bool,
    ) -> Vec<Value> {
        let mut request = self
            .get(&format!("/api/threads/{thread}/events"))
            .header("Accept", "text/event-stream");
        if let Some(after) = after {
            request = request.header("Last-Event-ID", after.to_string());
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 200);
        let mut body = response.bytes_stream();
        let mut parser = orbyn_core::agents::sse::SseParser::default();
        let mut events = Vec::new();
        loop {
            let chunk = tokio::time::timeout(Duration::from_secs(15), body.next())
                .await
                .expect("event stream stalled")
                .expect("event stream ended")
                .unwrap();
            for frame in parser.push(&chunk).expect("test SSE frames fit the cap") {
                let event: Value = serde_json::from_str(&frame).unwrap();
                let done = stop(&event);
                events.push(event);
                if done {
                    return events;
                }
            }
        }
    }

    /// Opens an SSE response, for a test that reads frames as they arrive.
    pub async fn sse(&self, path: &str) -> SseReader {
        let response = self
            .get(path)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{path}");
        SseReader {
            response,
            parser: orbyn_core::agents::sse::SseParser::default(),
            pending: std::collections::VecDeque::new(),
        }
    }

    /// Reads SSE frames from `path` until `stop` matches one, and returns them.
    pub async fn sse_until(&self, path: &str, stop: impl Fn(&Value) -> bool) -> Vec<Value> {
        let mut reader = self.sse(path).await;
        let mut frames = Vec::new();
        while let Some(value) = reader.next().await {
            let done = stop(&value);
            frames.push(value);
            if done {
                return frames;
            }
        }
        panic!("the stream ended before its last frame");
    }

    /// Reads SSE frames from `path` until the service closes the stream.
    pub async fn sse_to_end(&self, path: &str) -> Vec<Value> {
        let mut reader = self.sse(path).await;
        let mut frames = Vec::new();
        while let Some(value) = reader.next().await {
            frames.push(value);
        }
        frames
    }

    pub async fn all_events(&self, thread: &str) -> Vec<Value> {
        self.events_until(thread, None, |event| {
            matches!(
                event["event"]["type"].as_str(),
                Some("RUN_FINISHED" | "RUN_ERROR")
            )
        })
        .await
    }

    pub async fn audit(&self, thread: &str) -> Vec<Value> {
        self.json(&format!("/api/audit?thread_id={thread}&limit=200"))
            .await
            .as_array()
            .unwrap()
            .clone()
    }

    /// Asks for a graceful exit by closing the parent pipe or sending SIGTERM.
    pub async fn stop(mut self) {
        #[cfg(unix)]
        {
            let pid = self.child.id().to_string();
            let _ = Command::new("kill").args(["-TERM", &pid]).status();
        }
        for _ in 0..1000 {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An open SSE response, read frame by frame.
pub struct SseReader {
    response: reqwest::Response,
    parser: orbyn_core::agents::sse::SseParser,
    pending: std::collections::VecDeque<Value>,
}

impl SseReader {
    /// The next frame, or `None` when the service closed the stream.
    pub async fn next(&mut self) -> Option<Value> {
        loop {
            if let Some(value) = self.pending.pop_front() {
                return Some(value);
            }
            let chunk = tokio::time::timeout(Duration::from_secs(45), self.response.chunk())
                .await
                .expect("stream stalled")
                .unwrap_or(None)?;
            for frame in self.parser.push(&chunk).expect("test SSE frames fit the cap") {
                self.pending
                    .push_back(serde_json::from_str(&frame).unwrap());
            }
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn text_of(events: &[Value]) -> String {
    events
        .iter()
        .filter(|event| event["event"]["type"] == "TEXT_MESSAGE_CONTENT")
        .filter(|event| {
            !event["event"]["messageId"]
                .as_str()
                .unwrap_or_default()
                .ends_with(":prompt")
        })
        .filter_map(|event| event["event"]["delta"].as_str())
        .collect()
}

pub fn types_of(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| event["event"]["type"].as_str().map(str::to_owned))
        .collect()
}

pub fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A minimal valid skills library with its host links.
pub fn library(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root.join("skills/how")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "shared policy\n").unwrap();
    std::fs::write(root.join(".gitignore"), "repo/\n").unwrap();
    std::fs::write(
        root.join("skills/catalog.json"),
        r#"{"skills":[{"name":"how","category":"Engineering practice"}]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("skills/how/SKILL.md"),
        "---\nname: how\ndescription: \"Trace behavior\"\n---\n\n# how\n",
    )
    .unwrap();
    #[cfg(unix)]
    for host in [".claude", ".agents"] {
        let links = root.join(host).join("skills");
        std::fs::create_dir_all(&links).unwrap();
        std::os::unix::fs::symlink("../../skills/how", links.join("how")).unwrap();
    }
    run_git(root, &["init", "-q", "-b", "main"]);
    run_git(root, &["config", "user.email", "t@example.com"]);
    run_git(root, &["config", "user.name", "T"]);
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "library"]);
    canonical(root)
}

/// A trusted project with a thread on `engine`.
pub async fn setup(dir: &TempDir, options: Options, engine: &str) -> (Service, String) {
    let service = Service::start(&dir.path().join("data"), options);
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project = service.project(&folder, true).await;
    let thread = service.thread(&project, engine).await;
    (service, thread)
}
