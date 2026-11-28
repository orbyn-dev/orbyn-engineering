use crate::app::Core;
use orbyn_contracts::{ForgeStatus, PullFile, PullRequest};
use std::time::{Duration, Instant};

use super::stack::stack_of;
use super::watch::{WatchState, watch_news};
use super::*;

fn pull(number: u64, head: &str, base: &str) -> PullRequest {
    PullRequest {
        number,
        title: String::new(),
        url: String::new(),
        state: "open".into(),
        head: head.into(),
        base: base.into(),
        author: None,
        draft: false,
        updated_at: None,
        labels: Vec::new(),
        review_state: None,
        checks_state: None,
        checks: None,
    }
}

#[test]
fn a_change_breakdown_counts_kinds_and_lines() {
    let file = |status: &str, additions: u64, deletions: u64| PullFile {
        path: "a.rs".into(),
        previous_path: None,
        status: status.into(),
        additions,
        deletions,
        patch: None,
        viewed: false,
        viewed_synced: false,
    };
    let breakdown = pulls::change_breakdown(&[
        file("added", 3, 0),
        file("modified", 2, 1),
        file("deleted", 0, 4),
        file("renamed", 1, 1),
        // A forge status none of the four names counts only in `files`.
        file("copied", 5, 0),
    ]);
    assert_eq!(breakdown.files, 5);
    assert_eq!(breakdown.added, 1);
    assert_eq!(breakdown.modified, 1);
    assert_eq!(breakdown.deleted, 1);
    assert_eq!(breakdown.renamed, 1);
    // Line totals cover every file, including the unnamed kind.
    assert_eq!(breakdown.additions, 11);
    assert_eq!(breakdown.deletions, 6);
    assert_eq!(pulls::change_breakdown(&[]).files, 0);
}

#[test]
fn a_stack_follows_bases_both_ways() {
    let pulls = [
        pull(1, "a", "main"),
        pull(2, "b", "a"),
        pull(3, "c", "b"),
        pull(9, "other", "main"),
    ];
    let stack = stack_of(&pulls, 2).unwrap();
    let numbers: Vec<u64> = stack.layers.iter().map(|layer| layer.number).collect();
    assert_eq!(numbers, [1, 2, 3]);
    assert_eq!(stack.position, 1);
    assert_eq!(stack_of(&pulls, 9).unwrap().layers.len(), 1);
    assert!(stack_of(&pulls, 42).is_none());
}

#[test]
fn a_stack_with_a_cycle_still_ends() {
    let pulls = [pull(1, "a", "b"), pull(2, "b", "a")];
    assert_eq!(stack_of(&pulls, 1).unwrap().layers.len(), 2);
}

#[test]
fn watch_news_skips_your_own_comments() {
    let last = WatchState {
        checks: "pending".into(),
        ..WatchState::default()
    };
    let mut now = last.clone();
    now.comments = vec![("1".into(), Some("me".into()))];
    assert!(watch_news(&last, &now, Some("me")).0.is_empty());
    now.comments.push(("2".into(), Some("them".into())));
    let (news, comments_only) = watch_news(&last, &now, Some("me"));
    assert_eq!(news, ["- 1 new comment(s) or review(s)."]);
    assert!(comments_only);
    now.checks = "fail".into();
    let (news, comments_only) = watch_news(&last, &now, Some("me"));
    assert_eq!(news.len(), 2);
    assert!(!comments_only);
}

#[test]
fn a_watch_wake_quotes_repository_context_as_data() {
    let repo = "repo\"\nnot-a-new-field";
    let cwd = "C:\\work tree\\\"quoted\"\nnot-a-command";
    let prompt = super::watch::wake_prompt(12, repo, cwd, &["- A check failed.".into()]);
    let context = prompt
        .lines()
        .find_map(|line| line.strip_prefix("Repository: "))
        .expect("the wake identifies its repository");
    let context: serde_json::Value = serde_json::from_str(context).unwrap();
    assert_eq!(context["id"], repo);
    assert_eq!(context["worktree"], cwd);
    assert_eq!(prompt.lines().last(), Some("- A check failed."));
    assert!(!prompt.lines().any(|line| line == "not-a-command"));
}

#[tokio::test]
async fn watch_wakes_for_the_same_number_identify_each_scoped_repository() {
    use crate::source_control::forge::{Forge, with_crew_fixture};
    use crate::store::thread_repos::ThreadRepoRecord;
    use crate::store::{Admission, NewProject, NewThread};
    use crate::testing::{TempDir, run_git};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    let dir = TempDir::new();
    let mut config =
        crate::app::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.multi_repo = true;
    let core = Core::open(config).unwrap();
    let folders: Vec<_> = [
        ("primary worktree", "primary"),
        ("secondary worktree", "secondary"),
    ]
    .into_iter()
    .map(|(name, remote)| {
        let path = dir.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        run_git(&path, &["init", "-q", "-b", "main"]);
        run_git(
            &path,
            &[
                "remote",
                "add",
                "origin",
                &format!("https://github.com/example/{remote}.git"),
            ],
        );
        crate::util::canonicalize(&path).unwrap()
    })
    .collect();
    let primary = folders[0].to_string_lossy().into_owned();
    let project = core
        .store
        .add_project(NewProject {
            path: &primary,
            name: "primary",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id;
    core.store.trust_project(&project, "test").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.clone(),
            title: "watch both".into(),
            engine: "codex".into(),
            model: None,
            cwd: primary,
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    // Keep wakes queued without launching any engine, even if a workspace becomes available.
    core.store
        .admit_run(Admission {
            thread_id: &thread.id,
            prompt: "busy",
            actor: "test",
        })
        .unwrap();
    let repos = [project, "secondary-repository".into()];
    for (position, (repo, folder)) in repos.iter().zip(&folders).enumerate() {
        core.store
            .thread_repo_insert(
                &ThreadRepoRecord {
                    thread_id: thread.id.clone(),
                    repo_id: repo.clone(),
                    position: u32::try_from(position).unwrap(),
                    name: format!("repository {position}"),
                    branch: "main".into(),
                    base_branch: Some("main".into()),
                    worktree_path: folder.to_string_lossy().into_owned(),
                },
                "2026-11-26T00:00:00Z",
            )
            .unwrap();
        core.store.watch_pull(&thread.id, repo, 12).unwrap();
    }
    let reads = Arc::new(Mutex::new(Vec::new()));
    let seen = reads.clone();
    let fixture = Arc::new(move |forge, cwd: &std::path::Path, args: &[&str]| {
        assert_eq!(forge, Forge::GitHub);
        seen.lock().unwrap().push((
            cwd.to_owned(),
            args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        ));
        let answer = match args {
            ["pr", "view", "12", ..] => json!({
                "number": 12, "state": "OPEN", "title": "same number", "url": "https://example.test/pull/12",
                "headRefName": "work", "baseRefName": "main", "comments": [], "reviews": [],
            }),
            ["pr", "checks", "12", ..] => json!([{"name":"check", "state":"FAILURE"}]),
            [
                "api",
                "--paginate",
                "repos/{owner}/{repo}/issues/12/comments",
            ] => json!([]),
            ["api", "graphql", ..] => {
                json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}})
            }
            ["api", "user"] => json!({"login":"me"}),
            _ => panic!("unexpected forge read: {args:?}"),
        };
        Ok(answer.to_string())
    });
    with_crew_fixture(fixture, async {
        for watch in core.store.pull_watches(Some(&thread.id)).unwrap() {
            core.check_watch(&watch).await.unwrap();
        }
    })
    .await;
    let queue = core.store.list_queue(&thread.id).unwrap();
    assert_eq!(queue.len(), 2, "both repositories reported the same news");
    let reads = reads.lock().unwrap();
    for folder in &folders {
        assert_eq!(
            reads.iter().filter(|(cwd, _)| cwd == folder).count(),
            6,
            "each watch keeps its own worktree and adds no identity-only forge read"
        );
    }
    let contexts: Vec<serde_json::Value> = queue
        .iter()
        .map(|message| {
            assert!(message.prompt.ends_with("- A check failed."));
            let context = message
                .prompt
                .lines()
                .find_map(|line| line.strip_prefix("Repository: "))
                .expect("queued wake must identify the repository actually read");
            serde_json::from_str(context).unwrap()
        })
        .collect();
    for (repo, folder) in repos.iter().zip(&folders) {
        assert!(contexts.iter().any(|context| context["id"] == *repo
            && context["worktree"] == folder.to_string_lossy().as_ref()));
    }
    assert_ne!(queue[0].prompt, queue[1].prompt);
}

#[test]
fn ids_for_api_paths_stay_plain() {
    assert!(check_opaque("PRRT_kw-1=").is_ok());
    assert!(check_opaque("1/../2").is_err());
    assert!(check_opaque("").is_err());
}

fn forge_stub(name: &str) -> ForgeStatus {
    ForgeStatus {
        id: name.into(),
        name: name.into(),
        program: None,
        installed: false,
        version: None,
        signed_in: false,
        accounts: Vec::new(),
        viewer: None,
        problem: None,
    }
}

#[tokio::test]
async fn forges_answer_from_memory_without_asking_the_clis() {
    let dir = crate::testing::TempDir::new();
    let mut config =
        crate::app::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    let key = crate::util::swr::key(&["forges"]);
    core.scm_reads.forges.put(&key, vec![forge_stub("kept")]);
    // Asking for a rescan runs the real CLIs, so it is left to the end-to-end tests;
    // here only what is kept.
    let started = Instant::now();
    for _ in 0..100 {
        assert_eq!(core.forges(false).await[0].id, "kept");
    }
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[tokio::test]
async fn an_untrusted_project_is_refused_even_when_its_pull_request_is_kept() {
    let dir = crate::testing::TempDir::new();
    let mut config =
        crate::app::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    let folder = dir.path().join("app");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.to_string_lossy().into_owned();
    let project = core
        .store
        .add_project(crate::store::NewProject {
            path: &path,
            name: "app",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    assert!(!project.trusted);
    let key = crate::util::swr::key(&[&project.id, "12", &path]);
    let kept: orbyn_contracts::PullRequestDetail = serde_json::from_value(serde_json::json!({
        "pull": serde_json::to_value(pull(12, "feature", "main")).unwrap(),
        "body": "", "checks": null, "comments": null, "mergeable": null,
        "reviewers": [], "head_sha": null, "base_sha": null,
        "additions": null, "deletions": null, "changed_files": null,
    }))
    .unwrap();
    core.scm_reads.detail.put(&key, kept);
    let error = core.project_pull(&project.id, 12, false).await.unwrap_err();
    assert_eq!(error.code(), "folder_untrusted");
    // Trusted, the same kept answer is served without asking a forge.
    core.store.trust_project(&project.id, "t").unwrap();
    assert!(core.project_pull(&project.id, 12, false).await.is_ok());
}

#[tokio::test]
async fn another_forges_kept_marks_are_this_services_own() {
    let dir = crate::testing::TempDir::new();
    let folder = dir.path().join("repo");
    std::fs::create_dir_all(&folder).unwrap();
    crate::testing::run_git(&folder, &["init", "-q", "-b", "main"]);
    crate::testing::run_git(
        &folder,
        &["remote", "add", "origin", "https://gitlab.com/o/r.git"],
    );
    let mut config =
        crate::app::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    let project = core
        .store
        .add_project(crate::store::NewProject {
            path: folder.to_str().unwrap(),
            name: "repo",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "t").unwrap();
    // A GitLab changed-files answer kept as the files read would keep it; no CLI is asked.
    let file = PullFile {
        path: "a.rs".into(),
        previous_path: None,
        status: "modified".into(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1 +1 @@\n-x\n+y".into()),
        viewed: false,
        viewed_synced: false,
    };
    let kept = super::read_cache::PullFilesRead {
        files: vec![file],
        viewed: super::read_cache::ForgeViewed::NotApplicable,
    };
    let key = crate::util::swr::key(&[&project.id, "12", &project.path]);
    core.scm_reads.files.put(&key, kept);
    // A mark on a forge with no viewed state of its own stays this service's and reaches no forge:
    // marking it asks for the folder's forge only, which is a git read, and writes the store.
    core.set_files_viewed(&project.id, 12, &["a.rs".to_owned()], true, "t")
        .await
        .unwrap();
    let marked = core.pull_files(&project.id, 12, false).await.unwrap();
    assert!(marked[0].viewed, "the mark the person made here is shown");
    assert!(
        !marked[0].viewed_synced,
        "another forge has no viewed state to sync"
    );
    // Unmarking removes the service's mark again.
    core.set_files_viewed(&project.id, 12, &["a.rs".to_owned()], false, "t")
        .await
        .unwrap();
    let unmarked = core.pull_files(&project.id, 12, false).await.unwrap();
    assert!(!unmarked[0].viewed);
}

/// The frame `pr-detail-none` asks two things of one ref listing: is the branch a local branch,
/// and does the `origin` remote already hold it (issue 506).
#[test]
fn a_branch_the_remote_holds_is_named_by_its_origin_ref() {
    let listing = "refs/heads/main\nrefs/heads/feat/login\nrefs/remotes/origin/feat/login\n";
    assert_eq!(
        super::pulls::branch_refs(listing, "feat/login"),
        (true, Some("origin/feat/login".to_owned()))
    );
}

#[test]
fn a_local_branch_the_remote_does_not_hold_is_not_pushed() {
    let listing = "refs/heads/main\nrefs/remotes/origin/HEAD\n";
    assert_eq!(super::pulls::branch_refs(listing, "main"), (true, None));
}

#[test]
fn a_branch_only_the_remote_holds_is_pushed_and_not_local() {
    let listing = "refs/remotes/origin/feat/login\n";
    assert_eq!(
        super::pulls::branch_refs(listing, "feat/login"),
        (false, Some("origin/feat/login".to_owned()))
    );
}

#[test]
fn a_local_branch_named_origin_main_does_not_stand_in_for_the_origin_ref() {
    let listing = "refs/heads/origin/main\n";
    assert_eq!(
        super::pulls::branch_refs(listing, "origin/main"),
        (true, None)
    );
}

#[test]
fn another_remote_does_not_answer_for_origin() {
    let listing = "refs/remotes/upstream/feat/login\n";
    assert_eq!(
        super::pulls::branch_refs(listing, "feat/login"),
        (false, None)
    );
}

#[test]
fn a_branch_with_slashes_is_matched_whole() {
    let listing = "refs/remotes/origin/feat/login\n";
    assert_eq!(super::pulls::branch_refs(listing, "feat"), (false, None));
    assert_eq!(super::pulls::branch_refs(listing, "login"), (false, None));
}
