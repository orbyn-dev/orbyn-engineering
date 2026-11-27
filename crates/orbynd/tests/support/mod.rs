//! Runs the real `orbynd` binary against the fake engine over real processes.
//!
//! Browser tests drive the browser a run gives them: `ORBYN_BROWSER`, or the Playwright
//! headless shell that `npm run test:e2e:install` puts under `web/node_modules`. Without
//! one they print why and pass, so the default checks run on a machine with no browser;
//! `ORBYN_REQUIRE_BROWSER=1` turns that into a failure instead, so a job that should have
//! one cannot hide behind the skip. Giving the CI jobs a browser is issue #1575.

#![allow(dead_code)]

use std::collections::BTreeMap;
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

/// The newest schema this build knows: what a start upgrades an old database to.
pub fn latest_schema() -> i64 {
    orbyn_core::store::MIGRATIONS
        .last()
        .expect("a migration list")
        .0
}

/// Every schema version this build's migration list holds, in order: the versions a start records
/// once each. A count against `latest_schema` says the same thing only while the versions stay
/// contiguous, so the assertions compare against the list itself.
pub fn migration_versions() -> Vec<i64> {
    orbyn_core::store::MIGRATIONS
        .iter()
        .map(|(version, _)| *version)
        .collect()
}

/// Builds the database the service opens in `data` at exactly schema version `through`, the way an
/// install of that release left it, by applying the real migration list and recording each version.
/// The returned connection writes the fixture rows a case needs; the migration list stays the source
/// of truth, so the fixture cannot drift from the schema a later release upgrades.
pub fn old_database(data: &Path, through: i64) -> rusqlite::Connection {
    std::fs::create_dir_all(data).unwrap();
    let conn = rusqlite::Connection::open(data.join("orbyn.sqlite3")).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in orbyn_core::store::MIGRATIONS
        .iter()
        .filter(|(version, _)| *version <= through)
    {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'fixture')",
            [version],
        )
        .unwrap();
    }
    conn
}

#[derive(Clone, Default)]
pub struct Options {
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
}

/// A child a case spawned itself, with the environment it was given, so `Service::adopt` can check
/// that the options a restart would pass on still describe that environment.
pub struct Spawned {
    child: Option<Child>,
    stdout: Option<BufReader<std::process::ChildStdout>>,
    env: Vec<(String, String)>,
}

impl Spawned {
    /// Starts `serve --data-dir` the way the harness starts one, with `args` and `env` on top of its
    /// own variables and `defaults` in between. The environment stays with the child: a case that
    /// adopts the process must hand `Service` the options a restart re-applies, and `adopt` refuses
    /// options that would drop it.
    pub fn start(
        binary: impl AsRef<std::ffi::OsStr>,
        data: &Path,
        args: &[String],
        env: Vec<(String, String)>,
        defaults: &[(&str, &str)],
    ) -> Self {
        let mut command = Command::new(binary.as_ref());
        command
            .args(["serve", "--data-dir"])
            .arg(data)
            .args(args)
            .env("ORBYN_CLAUDE_BIN", FAKE)
            .env("ORBYN_CODEX_BIN", FAKE)
            .env("ORBYN_OFFLINE", "1")
            .current_dir(data.parent().unwrap_or(data))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // The runner's own Claude session or task-tool switches must not decide a test: a test that
        // needs one asks for it through the environment it hands over here.
        for name in [
            "CLAUDECODE",
            "ORBYN_CLAUDE_TASK_TOOLS",
            "CLAUDE_CODE_ENABLE_TODO_TOOLS",
        ] {
            command.env_remove(name);
        }
        for (key, value) in defaults {
            command.env(key, value);
        }
        for (key, value) in &env {
            command.env(key, value);
        }
        let mut child = command.spawn().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child: Some(child),
            stdout: Some(stdout),
            env,
        }
    }

    /// The environment this spawn was given, which a restart's options have to keep.
    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    /// The ready line `serve` prints once it listens, or the child's stderr when it exited first.
    pub fn ready(&mut self) -> Result<Value, String> {
        let mut line = String::new();
        self.stdout
            .as_mut()
            .expect("an unadopted spawn")
            .read_line(&mut line)
            .unwrap();
        if line.is_empty() {
            let mut text = String::new();
            let child = self.child.as_mut().expect("an unadopted spawn");
            std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut text).unwrap();
            return Err(text);
        }
        Ok(serde_json::from_str(&line).unwrap())
    }

    fn take_parts(&mut self) -> (Child, BufReader<std::process::ChildStdout>) {
        (
            self.child.take().expect("an unadopted spawn"),
            self.stdout.take().expect("an unadopted spawn"),
        )
    }
}

impl Drop for Spawned {
    /// A case that stops before adopting would otherwise leave its process running.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The value each variable ends up with: `Command::env` lets a later entry for a key replace an
/// earlier one, so the last entry is the one a spawn or a restart actually gets.
fn effective_env(env: &[(String, String)]) -> BTreeMap<&str, &str> {
    env.iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
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
    /// The folder handed to `serve --data-dir`, which a restart opens again.
    data: PathBuf,
    /// The options this service started with, which a restart passes on.
    options: Options,
}

impl Service {
    pub fn start(data: &Path, options: Options) -> Self {
        Self::try_start(data, options).expect("service should start")
    }

    pub fn try_start(data: &Path, options: Options) -> Result<Self, String> {
        // Person routes need the desktop shell's key. Most tests have no shell, so the development
        // override (debug builds only) is on; a test of the key itself sets it to "0".
        let mut spawned = Spawned::start(
            ORBYND,
            data,
            &options.args,
            options.env.clone(),
            &[("ORBYN_DEV_DISABLE_PERSON_KEY", "1")],
        );
        let start = spawned.ready()?;
        Ok(Self::adopt(data, options, &start, &mut spawned))
    }

    /// Adopts a service a test started itself with `serve --data-dir`, from its ready line and its
    /// process, so it still owns the child and can restart it like one started here. A restart
    /// re-applies exactly `options`, so they must keep every variable the spawn was given: options
    /// that describe another environment would come back with another home on the restart.
    pub fn adopt(data: &Path, options: Options, ready: &Value, spawned: &mut Spawned) -> Self {
        // A restart re-applies `options` through `Command::env`, which keeps the last entry for a
        // key, so compare the value each variable ends up with on both sides: a duplicate key in
        // `options` that comes after the spawn's entry would otherwise pass this check while the
        // restart gets another value.
        let spawned_env = effective_env(spawned.env());
        let options_env = effective_env(&options.env);
        let mut differing: Vec<(&str, &str, Option<&str>)> = Vec::new();
        for (key, value) in spawned_env {
            let restarted = options_env.get(key).copied();
            if restarted != Some(value) {
                differing.push((key, value, restarted));
            }
        }
        assert!(
            differing.is_empty(),
            "restarting with these options would give the spawn a different environment \
             (key, spawn, restart): {differing:?}"
        );
        let (child, stdout) = spawned.take_parts();
        Self {
            url: ready["url"].as_str().unwrap().to_owned(),
            token: ready["token"].as_str().unwrap().to_owned(),
            access_key: ready["access_key"].as_str().map(str::to_owned),
            person_key: ready["person_key"].as_str().map(str::to_owned),
            client: reqwest::Client::new(),
            child,
            stdout,
            data: data.to_owned(),
            options,
        }
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

    /// Kills this process and starts a fresh service on the same data folder with the same
    /// options, so a crash case reads the recovered state through the routes.
    pub fn restart(mut self) -> Service {
        self.kill();
        Service::start(&self.data, self.options.clone())
    }

    /// The fake engine's launch records so far, one per line of `ORBYN_FAKE_LAUNCH_FILE`.
    pub fn launches(&self) -> Vec<Value> {
        let file = self.signal_file("ORBYN_FAKE_LAUNCH_FILE");
        std::fs::read_to_string(file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Waits until the fake engine has recorded a launch with `mode`, and returns that record.
    /// This is the kill point a crash case waits for instead of sleeping: the engine itself
    /// says it reached the step, so the test kills at a chosen point, not a guessed one.
    pub async fn wait_for_launch(&self, mode: &str) -> Value {
        for _ in 0..1000 {
            if let Some(record) = self
                .launches()
                .into_iter()
                .find(|record| record["mode"] == mode)
            {
                return record;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the fake engine recorded no {mode} launch");
    }

    /// The path of a record file this service's options give the fake engine.
    fn signal_file(&self, name: &str) -> PathBuf {
        let value = self
            .options
            .env
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value))
            .unwrap_or_else(|| panic!("the test's options set no {name}"));
        PathBuf::from(value)
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
            for frame in self
                .parser
                .push(&chunk)
                .expect("test SSE frames fit the cap")
            {
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
    project_and_thread(dir, service, engine).await
}

/// The same setup, but the case spawns the process itself and adopts it: the options it hands
/// `adopt` are the environment a restart re-applies, so a case that restarts proves the adopted
/// service keeps it.
pub async fn setup_adopted(dir: &TempDir, options: Options, engine: &str) -> (Service, String) {
    let data = dir.path().join("data");
    let mut spawned = Spawned::start(
        ORBYND,
        &data,
        &options.args,
        options.env.clone(),
        &[("ORBYN_DEV_DISABLE_PERSON_KEY", "1")],
    );
    let ready = spawned.ready().expect("the adopted service listens");
    let service = Service::adopt(&data, options, &ready, &mut spawned);
    project_and_thread(dir, service, engine).await
}

async fn project_and_thread(dir: &TempDir, service: Service, engine: &str) -> (Service, String) {
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project = service.project(&folder, true).await;
    let thread = service.thread(&project, engine).await;
    (service, thread)
}
