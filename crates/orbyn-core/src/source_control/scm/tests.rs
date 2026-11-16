use crate::app::Core;
use orbyn_contracts::{ForgeStatus, PullRequest};
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
