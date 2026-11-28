//! A multi-repository thread resolves navigation coordinates in the repository a `repo_id` names, or the
//! repository a task-folder reference names by its first segment, never in the task folder its engine runs in
//! (docs/multi-repo.md).

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git};

fn options() -> Options {
    Options {
        env: vec![("ORBYN_MULTI_REPO".to_owned(), "1".to_owned())],
        ..Options::default()
    }
}

/// A repository with one commit on `main`, so HEAD exists and a worktree can be cut from it.
fn seed_repo(path: &Path, files: &[(&str, &str)]) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    run_git(path, &["config", "core.autocrlf", "false"]);
    for (name, contents) in files {
        std::fs::write(path.join(name), contents).unwrap();
    }
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "first"]);
    support::canonical(path)
}

/// Adds a folder of `project` as a repository and trusts it, as the person's two calls do.
async fn add_trusted(service: &Service, project: &str, repo: &Path) -> String {
    let (status, added) = service
        .send(
            &format!("/api/projects/{project}/repos/add-folder"),
            json!({ "path": repo.to_string_lossy() }),
        )
        .await;
    assert_eq!(status, 200, "{added}");
    let repo_id = added["id"].as_str().unwrap().to_owned();
    let (status, trusted) = service
        .send(
            &format!("/api/projects/{project}/repos/{repo_id}/trust"),
            json!({}),
        )
        .await;
    assert_eq!(status, 200, "{trusted}");
    repo_id
}

/// The worktree of one repository in a task folder, at `root/name`.
fn worktree_of(task: &Value, repo_id: &str) -> PathBuf {
    let name = task["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|repo| repo["repo_id"] == repo_id)
        .unwrap()["name"]
        .as_str()
        .unwrap();
    PathBuf::from(task["root"].as_str().unwrap()).join(name)
}

async fn resolve(service: &Service, thread: &str, body: Value) -> (u16, Value) {
    service
        .send(&format!("/api/threads/{thread}/paths/resolve"), body)
        .await
}

#[tokio::test]
async fn navigation_coordinates_are_resolved_in_the_repository_its_repo_id_names() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options());
    let project_path = seed_repo(&dir.path().join("project"), &[("a.txt", "one\n")]);
    let project = service.project(&project_path, true).await;
    let other = seed_repo(&dir.path().join("other"), &[("a.txt", "other\n")]);
    let other_id = add_trusted(&service, &project, &other).await;
    // A thread over both, each on a worktree of one shared branch.
    let (status, thread) = service
        .send(
            &format!("/api/projects/{project}/repo-threads"),
            json!({
                "repos": [
                    { "repo_id": project, "base_branch": "main" },
                    { "repo_id": other_id, "base_branch": "main" },
                ],
                "engine": "codex",
                "model": null,
                "permission_mode": "supervised",
                "message": null,
                "branch_name": null,
            }),
        )
        .await;
    assert_eq!(status, 200, "{thread}");
    let thread_id = thread["id"].as_str().unwrap().to_owned();
    let cwd = thread["cwd"].as_str().unwrap().to_owned();
    let task = service
        .json(&format!("/api/threads/{thread_id}/task-folder"))
        .await;
    assert_eq!(
        cwd,
        task["root"].as_str().unwrap(),
        "the thread runs in the task folder"
    );
    assert!(
        !Path::new(&cwd).join(".git").exists(),
        "the task folder is not a repository"
    );
    let primary = worktree_of(&task, &project);
    let second = worktree_of(&task, &other_id);
    // A file of each repository, so the answer says which worktree the path was resolved in.
    std::fs::write(primary.join("primary.txt"), "primary\n").unwrap();
    std::fs::write(second.join("second.txt"), "second\n").unwrap();

    // No repo_id is the primary repository, and the answer's cwd stays the thread's stored folder.
    let (code, resolved) = resolve(
        &service,
        &thread_id,
        json!({ "cwd": cwd, "path": "primary.txt", "basis": "repository" }),
    )
    .await;
    assert_eq!(code, 200, "{resolved}");
    assert_eq!(resolved["cwd"], cwd);
    assert_eq!(resolved["checkout_path"], "primary.txt");
    assert_eq!(resolved["repository_path"], "primary.txt");

    // The second repository's file is not in the primary worktree; its repo_id resolves it there.
    let (code, unnamed) = resolve(
        &service,
        &thread_id,
        json!({ "cwd": cwd, "path": "second.txt", "basis": "repository" }),
    )
    .await;
    assert_eq!(code, 200, "{unnamed}");
    assert!(unnamed["checkout_path"].is_null());
    let (code, named) = resolve(
        &service,
        &thread_id,
        json!({
            "cwd": cwd,
            "path": "second.txt",
            "basis": "repository",
            "repo_id": other_id,
        }),
    )
    .await;
    assert_eq!(code, 200, "{named}");
    assert_eq!(named["cwd"], cwd);
    assert_eq!(named["checkout_path"], "second.txt");
    assert_eq!(named["repository_path"], "second.txt");

    // The explorer's primary-repository file tab resolves a thread-relative path in the primary worktree.
    let (code, tabbed) = resolve(
        &service,
        &thread_id,
        json!({ "cwd": cwd, "path": "primary.txt", "basis": "thread" }),
    )
    .await;
    assert_eq!(code, 200, "{tabbed}");
    assert_eq!(tabbed["checkout_path"], "primary.txt");

    // A repository that is not the thread's is not found.
    let (code, missing) = resolve(
        &service,
        &thread_id,
        json!({
            "cwd": cwd,
            "path": "primary.txt",
            "basis": "repository",
            "repo_id": "no-such-repo",
        }),
    )
    .await;
    assert_eq!(code, 404, "{missing}");
    assert_eq!(missing["code"], "not_found");

    // The fence is the thread's stored folder: a worktree of it is not the thread's cwd.
    let (code, stale) = resolve(
        &service,
        &thread_id,
        json!({
            "cwd": second.to_string_lossy(),
            "path": "second.txt",
            "basis": "repository",
            "repo_id": other_id,
        }),
    )
    .await;
    assert_eq!(code, 409, "{stale}");
    assert_eq!(stale["code"], "stale_owner");
    service.stop().await;
}

/// A task-folder reference (a message link or a tool result) names a repository by the folder that repository
/// sits in, so `planner/note.txt` is the `planner` repository's `note.txt`, and the answer says which repository
/// that is (issue #1581).
#[tokio::test]
async fn a_task_folder_reference_resolves_in_the_repository_its_first_segment_names() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options());
    let project_path = seed_repo(&dir.path().join("project"), &[("a.txt", "one\n")]);
    let project = service.project(&project_path, true).await;
    let other = seed_repo(&dir.path().join("other"), &[("a.txt", "other\n")]);
    let other_id = add_trusted(&service, &project, &other).await;
    let (status, thread) = service
        .send(
            &format!("/api/projects/{project}/repo-threads"),
            json!({
                "repos": [
                    { "repo_id": project, "base_branch": "main" },
                    { "repo_id": other_id, "base_branch": "main" },
                ],
                "engine": "codex",
                "model": null,
                "permission_mode": "supervised",
                "message": null,
                "branch_name": null,
            }),
        )
        .await;
    assert_eq!(status, 200, "{thread}");
    let thread_id = thread["id"].as_str().unwrap().to_owned();
    let cwd = thread["cwd"].as_str().unwrap().to_owned();
    let task = service
        .json(&format!("/api/threads/{thread_id}/task-folder"))
        .await;
    let primary = worktree_of(&task, &project);
    let second = worktree_of(&task, &other_id);
    // One name with different bytes in each repository, so the answer says whose file it resolved.
    std::fs::write(primary.join("note.txt"), "primary\n").unwrap();
    std::fs::write(second.join("note.txt"), "other\n").unwrap();
    let folder = |worktree: &Path| worktree.file_name().unwrap().to_string_lossy().into_owned();
    let (second_folder, primary_folder) = (folder(&second), folder(&primary));
    assert_ne!(
        second_folder, primary_folder,
        "each repository has its own folder"
    );

    // The reference the thread's own file tab makes, relative to the task folder, names the second repository.
    let (code, named) = resolve(
        &service,
        &thread_id,
        json!({ "cwd": cwd, "path": format!("{second_folder}/note.txt"), "basis": "thread" }),
    )
    .await;
    assert_eq!(code, 200, "{named}");
    assert_eq!(
        named["checkout_path"], "note.txt",
        "resolved in the repository the first segment names, not the primary worktree"
    );
    assert_eq!(named["repository_path"], "note.txt");
    assert_eq!(
        named["repo_id"], other_id,
        "the answer names the repository the coordinates belong to"
    );

    // The primary's own folder name is its file, and the answer names the primary repository.
    let (code, own) = resolve(
        &service,
        &thread_id,
        json!({ "cwd": cwd, "path": format!("{primary_folder}/note.txt"), "basis": "thread" }),
    )
    .await;
    assert_eq!(code, 200, "{own}");
    assert_eq!(own["checkout_path"], "note.txt");
    assert_eq!(own["repo_id"], project);

    // A path under no repository's folder keeps its older meaning: the primary repository's worktree.
    let (code, primary_file) = resolve(
        &service,
        &thread_id,
        json!({ "cwd": cwd, "path": "a.txt", "basis": "thread" }),
    )
    .await;
    assert_eq!(code, 200, "{primary_file}");
    assert_eq!(primary_file["checkout_path"], "a.txt");
    assert_eq!(primary_file["repo_id"], project);
    service.stop().await;
}
