//! Issue 506, frame `pr-detail-none`: the read a pull request page makes before a pull
//! request exists — whether the branch's remote already has it, and the pull request that
//! already comes from it or none.
//!
//! The forge is asked only for a branch the remote has, so every case here runs on a
//! repository with no remote: the frame's own case. A repository that has no remote can
//! have no pull request, and the read says so without reaching for a forge.

mod support;

use std::path::PathBuf;

use serde_json::Value;
use support::{Options, Service, TempDir, run_git};

async fn start(dir: &TempDir) -> Service {
    let service = Service::start(&dir.path().join("data"), Options::default());
    service.ensure_workspace(dir.path()).await;
    service
}

/// A repository on `main`, one commit, and a local branch `feat/login` never pushed.
fn checkout(dir: &TempDir) -> PathBuf {
    let repo = dir.path().join("checkout");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "t@example.com"]);
    run_git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-q", "-m", "first"]);
    run_git(&repo, &["branch", "feat/login"]);
    repo
}

/// The answer and its status for one branch read.
async fn detail(service: &Service, project: &str, branch: &str) -> (u16, Value) {
    let response = service
        .get(&format!(
            "/api/projects/{project}/branches/detail?branch={branch}"
        ))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn folder(dir: &TempDir) -> PathBuf {
    let folder = dir.path().join("plain");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.txt"), "one\n").unwrap();
    folder
}

#[tokio::test]
async fn a_branch_no_remote_has_is_not_pushed_and_has_no_pull_request() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&checkout(&dir), true).await;

    let (status, body) = detail(&service, &project, "feat/login").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["repo_id"], project,
        "the primary repository is the project"
    );
    assert_eq!(body["branch"], "feat/login");
    assert_eq!(body["remote_ref"], Value::Null, "{body}");
    assert_eq!(body["pull_request"], Value::Null, "{body}");
}

#[tokio::test]
async fn the_branch_that_is_checked_out_answers_the_same_way() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&checkout(&dir), true).await;

    let (status, body) = detail(&service, &project, "main").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["branch"], "main");
    assert_eq!(body["remote_ref"], Value::Null, "{body}");
    assert_eq!(body["pull_request"], Value::Null, "{body}");
}

#[tokio::test]
async fn an_unknown_branch_is_not_found() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&checkout(&dir), true).await;

    let (status, body) = detail(&service, &project, "feat/nothing").await;

    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found", "{body}");
}

#[tokio::test]
async fn a_name_that_is_not_a_branch_is_refused() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&checkout(&dir), true).await;

    for name in ["-x", "bad..name", "trailing/"] {
        let (status, body) = detail(&service, &project, name).await;
        assert_eq!(status, 400, "{name}: {body}");
        assert_eq!(body["code"], "invalid_input", "{name}: {body}");
    }
}

#[tokio::test]
async fn a_folder_that_is_not_in_git_has_no_branch() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&folder(&dir), true).await;

    let (status, body) = detail(&service, &project, "main").await;

    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found", "{body}");
}

#[tokio::test]
async fn an_unknown_repository_is_not_found() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&checkout(&dir), true).await;

    let (status, body) = detail(&service, &project, "feat/login&repo_id=not-a-repository").await;

    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found", "{body}");
}

#[tokio::test]
async fn an_untrusted_project_is_refused() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let project = service.project(&checkout(&dir), false).await;

    let (status, body) = detail(&service, &project, "main").await;

    assert_eq!(status, 403, "{body}");
}

#[tokio::test]
async fn an_unknown_project_is_not_found() {
    let dir = TempDir::new();
    let service = start(&dir).await;

    let (status, body) = detail(&service, "no-such-project", "main").await;

    assert_eq!(status, 404, "{body}");
}

/// The repository asked for is the one read, so its own id comes back and the project's
/// own folder is not answered instead.
#[tokio::test]
async fn a_named_repository_is_read_rather_than_the_project_folder() {
    let dir = TempDir::new();
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            env: vec![("ORBYN_MULTI_REPO".to_owned(), "1".to_owned())],
            ..Options::default()
        },
    );
    service.ensure_workspace(dir.path()).await;
    let project = service.project(&checkout(&dir), true).await;

    let second = dir.path().join("second");
    std::fs::create_dir_all(&second).unwrap();
    run_git(&second, &["init", "-q", "-b", "main"]);
    run_git(&second, &["config", "user.email", "t@example.com"]);
    run_git(&second, &["config", "user.name", "T"]);
    std::fs::write(second.join("b.txt"), "two\n").unwrap();
    run_git(&second, &["add", "."]);
    run_git(&second, &["commit", "-q", "-m", "first"]);
    run_git(&second, &["branch", "feat/second"]);

    let (status, body) = service
        .send(
            &format!("/api/projects/{project}/repos/add-folder"),
            serde_json::json!({ "path": second }),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let repo_id = body["id"].as_str().unwrap().to_owned();
    let (status, _) = service
        .send(
            &format!("/api/projects/{project}/repos/{repo_id}/trust"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(status, 200, "the added repository is trusted");

    let (status, body) = detail(
        &service,
        &project,
        &format!("feat/second&repo_id={repo_id}"),
    )
    .await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["repo_id"], repo_id);
    assert_eq!(body["branch"], "feat/second");
    assert_eq!(body["remote_ref"], Value::Null, "{body}");
}
