//! A thread over two repositories through the real service: the Changes and Git reads name one repository, so a
//! task folder is never handed to Git.

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

/// A repository with one commit on `main`, so HEAD exists.
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

fn paths(diff: &Value) -> Vec<String> {
    diff.as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_owned())
        .collect()
}

fn branch_names(branches: &Value) -> Vec<String> {
    branches
        .as_array()
        .unwrap()
        .iter()
        .map(|branch| branch["name"].as_str().unwrap().to_owned())
        .collect()
}

fn subjects(commits: &Value) -> Vec<String> {
    commits
        .as_array()
        .unwrap()
        .iter()
        .map(|commit| commit["subject"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn repository_reads_name_one_repository_and_never_the_task_folder() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options());
    let project = service.project(&dir.path().join("project"), true).await;
    // The project's own checkout, and a second repository with its latest commit changing a.txt.
    seed_repo(
        &dir.path().join("library/repo/project"),
        &[("base.txt", "base\n")],
    );
    let other = seed_repo(&dir.path().join("other"), &[("a.txt", "one\n")]);
    std::fs::write(other.join("a.txt"), "other\n").unwrap();
    run_git(&other, &["commit", "-qam", "other"]);
    // A second branch to cut this repository's worktree from, so the two choices differ.
    run_git(&other, &["branch", "release"]);
    // Add it to the project and trust it.
    let (status, added) = service
        .send(
            &format!("/api/projects/{project}/repos/add-folder"),
            json!({ "path": other.to_string_lossy() }),
        )
        .await;
    assert_eq!(status, 200, "{added}");
    let other_id = added["id"].as_str().unwrap().to_owned();
    let (status, trusted) = service
        .send(
            &format!("/api/projects/{project}/repos/{other_id}/trust"),
            json!({}),
        )
        .await;
    assert_eq!(status, 200, "{trusted}");
    assert_eq!(trusted["trusted"], true);
    // A thread over both, each on its own worktree of the one branch.
    let (status, thread) = service
        .send(
            &format!("/api/projects/{project}/repo-threads"),
            json!({
                "repos": [
                    { "repo_id": project, "base_branch": "main" },
                    { "repo_id": other_id, "base_branch": "release" },
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
    let task = service
        .json(&format!("/api/threads/{thread_id}/task-folder"))
        .await;
    let root = PathBuf::from(task["root"].as_str().unwrap());
    assert!(
        !root.join(".git").exists(),
        "the task folder is not a repository"
    );
    // The repositories read serves each repository's own base: the branch its worktree was cut from.
    let repos = service
        .json(&format!("/api/threads/{thread_id}/repos"))
        .await;
    let base = |repo_id: &str| -> Option<String> {
        repos
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["repo_id"] == repo_id)
            .and_then(|repo| repo["base_branch"].as_str().map(str::to_owned))
    };
    assert_eq!(base(&project).as_deref(), Some("main"));
    assert_eq!(base(&other_id).as_deref(), Some("release"));
    let (primary_dir, other_dir) = (worktree_of(&task, &project), worktree_of(&task, &other_id));
    // The same path changed in both: the reads must not confuse the two files.
    std::fs::write(primary_dir.join("a.txt"), "from the primary\n").unwrap();
    std::fs::write(other_dir.join("a.txt"), "from the second\n").unwrap();

    // No repo_id is the primary repository.
    let primary = service
        .json(&format!("/api/threads/{thread_id}/diff"))
        .await;
    assert_eq!(paths(&primary), ["a.txt"]);
    assert_eq!(primary[0]["repo_id"], project);
    assert!(
        primary[0]["patch"]
            .as_str()
            .unwrap()
            .contains("from the primary"),
        "{primary}"
    );
    // The repo_id is the second repository, which is another repository's file.
    let second = service
        .json(&format!("/api/threads/{thread_id}/diff?repo_id={other_id}"))
        .await;
    assert_eq!(paths(&second), ["a.txt"]);
    assert_eq!(second[0]["repo_id"], other_id);
    assert!(
        second[0]["patch"]
            .as_str()
            .unwrap()
            .contains("from the second"),
        "{second}"
    );
    assert!(
        !second[0]["patch"]
            .as_str()
            .unwrap()
            .contains("from the primary"),
        "the primary's bytes are not the second repository's file: {second}"
    );
    // A repository that is not the thread's is not found.
    let missing = service
        .get(&format!(
            "/api/threads/{thread_id}/diff?repo_id=no-such-repo"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status().as_u16(), 404);

    // Commits are the named repository's.
    let commits = service
        .json(&format!("/api/threads/{thread_id}/commits"))
        .await;
    assert_eq!(subjects(&commits)[0], "first");
    let other_commits = service
        .json(&format!(
            "/api/threads/{thread_id}/commits?repo_id={other_id}"
        ))
        .await;
    assert_eq!(subjects(&other_commits)[0], "other");
    // A commit diff runs in the repository that has the commit.
    let sha = other_commits[0]["sha"].as_str().unwrap();
    let commit = service
        .json(&format!(
            "/api/threads/{thread_id}/diff?scope=commit&sha={sha}&repo_id={other_id}"
        ))
        .await;
    assert_eq!(paths(&commit), ["a.txt"]);

    // Branches are the named repository's.
    run_git(&other, &["branch", "second-only"]);
    let branches = service
        .json(&format!("/api/threads/{thread_id}/branches"))
        .await;
    assert!(!branch_names(&branches).contains(&"second-only".to_owned()));
    let other_branches = service
        .json(&format!(
            "/api/threads/{thread_id}/branches?repo_id={other_id}"
        ))
        .await;
    assert!(branch_names(&other_branches).contains(&"second-only".to_owned()));

    // A branch comparison reads the named repository, committed and uncommitted work included.
    std::fs::write(other_dir.join("committed.txt"), "x\n").unwrap();
    run_git(&other_dir, &["add", "."]);
    run_git(
        &other_dir,
        &["commit", "-q", "-m", "work in the second repository"],
    );
    let branch = service
        .json(&format!(
            "/api/threads/{thread_id}/diff?scope=branch&base=main&repo_id={other_id}"
        ))
        .await;
    assert!(
        paths(&branch).contains(&"committed.txt".to_owned()),
        "{branch}"
    );
    let primary_branch = service
        .json(&format!(
            "/api/threads/{thread_id}/diff?scope=branch&base=main"
        ))
        .await;
    assert!(
        !paths(&primary_branch).contains(&"committed.txt".to_owned()),
        "{primary_branch}"
    );
}

#[tokio::test]
async fn a_thread_without_the_projects_folder_names_its_first_repository() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options());
    let project = service.project(&dir.path().join("project"), true).await;
    let first = seed_repo(&dir.path().join("first"), &[("a.txt", "one\n")]);
    let second = seed_repo(&dir.path().join("second"), &[("a.txt", "two\n")]);
    let first_id = add_trusted(&service, &project, &first).await;
    let second_id = add_trusted(&service, &project, &second).await;
    // The project's own folder is not chosen, so the first stored repository is the primary one.
    let (status, thread) = service
        .send(
            &format!("/api/projects/{project}/repo-threads"),
            json!({
                "repos": [
                    { "repo_id": first_id, "base_branch": "main" },
                    { "repo_id": second_id, "base_branch": "main" },
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
    let task = service
        .json(&format!("/api/threads/{thread_id}/task-folder"))
        .await;
    let (first_dir, second_dir) = (
        worktree_of(&task, &first_id),
        worktree_of(&task, &second_id),
    );
    std::fs::write(first_dir.join("a.txt"), "from the first\n").unwrap();
    std::fs::write(second_dir.join("a.txt"), "from the second\n").unwrap();
    // With no repo_id the read is the first repository, and the files carry the same repository's id.
    let primary = service
        .json(&format!("/api/threads/{thread_id}/diff"))
        .await;
    assert_eq!(paths(&primary), ["a.txt"]);
    assert_eq!(
        primary[0]["repo_id"], first_id,
        "the label names the repository the read used: {primary}"
    );
    assert!(
        primary[0]["patch"]
            .as_str()
            .unwrap()
            .contains("from the first"),
        "{primary}"
    );
    // The second repository is only read when it is named.
    let second_diff = service
        .json(&format!(
            "/api/threads/{thread_id}/diff?repo_id={second_id}"
        ))
        .await;
    assert_eq!(second_diff[0]["repo_id"], second_id);
    assert!(
        second_diff[0]["patch"]
            .as_str()
            .unwrap()
            .contains("from the second"),
        "{second_diff}"
    );
}

/// A run in a multi-repository thread is snapshotted in each repository's worktree, so its turn and thread
/// diffs read in the repository one names instead of answering `404 not_found` because the task folder is
/// not a checkout. The engine names the file it writes by its absolute path, as a tool call may.
#[tokio::test]
async fn a_multi_repository_run_keeps_a_checkpoint_in_each_repository() {
    let dir = TempDir::new();
    let data = dir.path().join("data");
    let service = Service::start(
        &data,
        Options {
            env: vec![
                ("ORBYN_MULTI_REPO".to_owned(), "1".to_owned()),
                (
                    "ORBYN_FAKE_CHECKOUT".to_owned(),
                    data.to_string_lossy().into_owned(),
                ),
            ],
            ..Options::default()
        },
    );
    let project = service.project(&dir.path().join("project"), true).await;
    // The project's own repository and a second one of the thread, each with one commit so HEAD exists.
    seed_repo(
        &dir.path().join("library/repo/project"),
        &[("base.txt", "base\n")],
    );
    let other = seed_repo(&dir.path().join("other"), &[("base.txt", "base\n")]);
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
    let task = service
        .json(&format!("/api/threads/{thread_id}/task-folder"))
        .await;
    let primary_dir = worktree_of(&task, &project);
    let other_dir = worktree_of(&task, &other_id);
    let written = primary_dir.join("written.txt");
    let run = service
        .admit(&thread_id, &format!("[write:{}]", written.display()))
        .await;
    assert_eq!(service.finished(&thread_id).await, "completed");

    // The run's turn diff is the primary repository's, read in its worktree rather than the task folder.
    let turn = service
        .json(&format!(
            "/api/threads/{thread_id}/diff?scope=turn&run={run}"
        ))
        .await;
    assert_eq!(paths(&turn), ["written.txt"]);
    // The second repository has the run's checkpoint too, and its tree did not change.
    let second = service
        .json(&format!(
            "/api/threads/{thread_id}/diff?scope=turn&run={run}&repo_id={other_id}"
        ))
        .await;
    assert!(
        second.as_array().unwrap().is_empty(),
        "the run changed nothing in the second repository: {second}"
    );
    // The whole-thread diff starts at the first checkpoint of the repository it names.
    let whole = service
        .json(&format!("/api/threads/{thread_id}/diff?scope=thread"))
        .await;
    assert_eq!(paths(&whole), ["written.txt"]);
    // The turn picker lists the run, so a person can ask for its diff.
    let checkpoints = service
        .json(&format!("/api/threads/{thread_id}/checkpoints"))
        .await;
    assert!(
        checkpoints
            .as_array()
            .unwrap()
            .iter()
            .any(|checkpoint| checkpoint["run_id"] == run),
        "{checkpoints}"
    );
    assert!(
        !other_dir.join("written.txt").exists(),
        "the write was in the primary repository's worktree only"
    );
    // A restore moves each worktree to the run's start, in that repository's own Git objects.
    // A file only the second worktree has makes its own restore observable: skipping that worktree would leave it.
    std::fs::write(other_dir.join("later.txt"), "later\n").unwrap();
    let (status, body) = service
        .send(
            &format!("/api/threads/{thread_id}/restore"),
            json!({ "run_id": run, "point": "start" }),
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert!(
        !written.exists(),
        "the restore removed the file the run added"
    );
    assert!(
        !other_dir.join("later.txt").exists(),
        "the restore moved the second repository's worktree to the run's start too"
    );
    assert!(
        primary_dir.join("base.txt").exists() && other_dir.join("base.txt").exists(),
        "a restore leaves the tracked files it did not change"
    );
}
