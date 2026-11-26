//! F25: a new worktree's branch name follows the naming mode, through the real service
//! over HTTP and real Git. The engine is the protocol double, so its one-shot call is
//! recorded at the executable boundary: the prompt on stdin is the wire evidence.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git};

const ENGINE: &str = "codex";

/// Records every one-shot prompt and makes the engine answer `reply` to a branch-name
/// request. Any other one-shot still answers the title double's reply.
fn one_shot_options(dir: &TempDir, reply: &str) -> (PathBuf, Options) {
    let record = dir.path().join("one-shot");
    let options = Options {
        env: vec![
            (
                "ORBYN_FAKE_ONE_SHOT_FILE".into(),
                record.to_string_lossy().into_owned(),
            ),
            ("ORBYN_FAKE_BRANCH_NAME".into(), reply.to_owned()),
        ],
        ..Options::default()
    };
    (record, options)
}

/// A trusted real Git project, so the service makes real worktrees beside it.
async fn project(dir: &TempDir, options: Options) -> (Service, String, PathBuf) {
    let service = Service::start(&dir.path().join("data"), options);
    let repo = dir.path().join("project");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("README.md"), "fixture\n").unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "t@example.com"]);
    run_git(&repo, &["config", "user.name", "T"]);
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-qm", "Initial"]);
    let project = service.project(&repo, true).await;
    (service, project, repo)
}

async fn put(service: &Service, path: &str, body: Value) -> (u16, Value) {
    let response = service
        .client
        .put(format!("{}{path}", service.url))
        .bearer_auth(&service.token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn set_preferences(service: &Service, values: Value) {
    let (status, body) = put(service, "/api/preferences", json!({ "values": values })).await;
    assert_eq!(status, 200, "{body}");
}

async fn new_worktree_thread(
    service: &Service,
    project: &str,
    purpose: &str,
    title: &str,
) -> (u16, Value) {
    service
        .send(
            "/api/threads",
            json!({
                "project_id": project,
                "engine": ENGINE,
                "model": null,
                "title": title,
                "worktree": purpose,
                "skill_profile": null,
            }),
        )
        .await
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "-C",
            repo.to_str().unwrap(),
            "rev-parse",
            "--verify",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .unwrap()
        .status
        .success()
}

#[tokio::test]
async fn create_thread_custom_naming_asks_the_engine_with_the_instructions() {
    let dir = TempDir::new();
    let (record, options) = one_shot_options(&dir, "release/rewrite-login");
    let (service, project, repo) = project(&dir, options).await;
    set_preferences(
        &service,
        json!({
            "branch_naming_mode": "custom",
            "branch_name_instructions": "Use release/ and the issue number 42.",
        }),
    )
    .await;

    let (status, thread) =
        new_worktree_thread(&service, &project, "login-timeout", "Add the login timeout").await;
    assert_eq!(status, 201, "{thread}");
    assert_eq!(thread["worktree"]["branch"], "release/rewrite-login");
    assert!(
        branch_exists(&repo, "release/rewrite-login"),
        "the model-written name is a real local ref"
    );

    let prompt = std::fs::read_to_string(&record).expect("the engine answered one prompt");
    assert!(
        prompt.contains("Use release/ and the issue number 42."),
        "the instructions reach the prompt: {prompt}"
    );
    assert!(
        prompt.contains("Add the login timeout"),
        "the message reaches the prompt: {prompt}"
    );
    let args = std::fs::read_to_string(record.with_extension("args")).unwrap();
    assert!(
        args.contains("fake-small"),
        "the resolved text model: {args}"
    );

    let audit = service.json("/api/audit?limit=100").await;
    let row = audit
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["action"] == "engine.one_shot" && row["detail"]["purpose"] == "branch_name")
        .expect("the spending call is audited");
    assert_eq!(row["target"], ENGINE);
    assert_eq!(row["detail"]["model"], "fake-small");
}

#[tokio::test]
async fn create_thread_static_and_semantic_naming_never_ask_the_engine() {
    let dir = TempDir::new();
    let (record, options) = one_shot_options(&dir, "release/should-not-be-used");
    let (service, project, _repo) = project(&dir, options).await;

    set_preferences(
        &service,
        json!({
            "branch_naming_mode": "static",
            "branch_name_prefix": "team",
            "branch_name_instructions": "Use release/.",
        }),
    )
    .await;
    let (status, thread) =
        new_worktree_thread(&service, &project, "static-one", "Static proof").await;
    assert_eq!(status, 201, "{thread}");
    assert_eq!(thread["worktree"]["branch"], "team/static-one");
    assert!(!record.exists(), "static naming spends no engine call");

    set_preferences(
        &service,
        json!({ "branch_naming_mode": "semantic", "branch_name_prefix": "feat" }),
    )
    .await;
    let (status, thread) =
        new_worktree_thread(&service, &project, "semantic-one", "Add the login timeout").await;
    assert_eq!(status, 201, "{thread}");
    assert_eq!(thread["worktree"]["branch"], "feat/add-the-login-timeout");
    assert!(!record.exists(), "semantic naming spends no engine call");
}

#[tokio::test]
async fn create_thread_custom_naming_without_instructions_asks_no_engine() {
    let dir = TempDir::new();
    let (record, options) = one_shot_options(&dir, "release/should-not-be-used");
    let (service, project, _repo) = project(&dir, options).await;
    set_preferences(
        &service,
        json!({ "branch_naming_mode": "custom", "branch_name_instructions": "" }),
    )
    .await;

    let (status, thread) =
        new_worktree_thread(&service, &project, "plain-custom", "Add the login timeout").await;
    assert_eq!(status, 201, "{thread}");
    assert_eq!(thread["worktree"]["branch"], "plain-custom");
    assert!(!record.exists(), "nothing to follow, so nothing is asked");
}

#[tokio::test]
async fn create_thread_on_a_reused_worktree_asks_no_engine() {
    let dir = TempDir::new();
    let (record, options) = one_shot_options(&dir, "release/rewrite-login");
    let (service, project, _repo) = project(&dir, options).await;
    set_preferences(
        &service,
        json!({
            "branch_naming_mode": "custom",
            "branch_name_instructions": "Use release/.",
        }),
    )
    .await;

    let (status, first) =
        new_worktree_thread(&service, &project, "reused-one", "Rewrite the login").await;
    assert_eq!(status, 201, "{first}");
    assert_eq!(first["worktree"]["branch"], "release/rewrite-login");
    assert!(record.exists());
    std::fs::remove_file(&record).unwrap();

    // The same purpose names the worktree already there, whose branch is kept.
    let (status, second) =
        new_worktree_thread(&service, &project, "reused-one", "Rewrite the login again").await;
    assert_eq!(status, 201, "{second}");
    assert_eq!(second["worktree"]["branch"], "release/rewrite-login");
    assert!(
        !record.exists(),
        "a reused worktree keeps its name without a model call"
    );
}

#[tokio::test]
async fn create_thread_custom_naming_falls_back_when_the_reply_is_not_a_ref() {
    let dir = TempDir::new();
    let (record, options) = one_shot_options(&dir, "not a branch name");
    let (service, project, _repo) = project(&dir, options).await;
    set_preferences(
        &service,
        json!({
            "branch_naming_mode": "custom",
            "branch_name_instructions": "Use release/.",
        }),
    )
    .await;

    let (status, thread) =
        new_worktree_thread(&service, &project, "login-timeout", "Add the login timeout").await;
    assert_eq!(status, 201, "{thread}");
    assert_eq!(
        thread["worktree"]["branch"], "login-timeout",
        "the purpose stays the complete name"
    );
    assert!(record.exists(), "the engine was asked before the fallback");
}
