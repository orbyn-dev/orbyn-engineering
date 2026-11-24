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
    let root = PathBuf::from(task["root"].as_str().unwrap());
    assert!(
        !root.join(".git").exists(),
        "the task folder is not a repository"
    );
    let worktree_of = |repo_id: &str| {
        let name = task["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["repo_id"] == repo_id)
            .unwrap()["name"]
            .as_str()
            .unwrap()
            .to_owned();
        root.join(name)
    };
    let (primary_dir, other_dir) = (worktree_of(&project), worktree_of(&other_id));
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
