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
