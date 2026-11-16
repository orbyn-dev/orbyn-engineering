//! The default-branch pin (issue #574): what a project remembers across tasks, when a start blocks instead of reading
//! HEAD alone, and the person's confirm and reset. Fake engines only; the only processes are `git`.

use std::path::Path;

use serde_json::{Value, json};

use super::classes::{commit_crew, file, pin_origin_here, rule, two};
use super::harness::{World, brief, key};
use crate::error::CoreError;
use crate::testing::run_git;

fn git_out(repo: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn head(repo: &Path) -> String {
    git_out(repo, &["rev-parse", "HEAD"])
}

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// The refusal texts of a start that routed nothing.
fn refused(answer: &Value) -> Vec<String> {
    assert_eq!(answer["routed"], false, "{answer}");
    answer["refusal"]["rejected"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["text"].as_str().map(str::to_owned))
        .collect()
}

fn says(texts: &[String], words: &str) {
    assert!(
        texts.iter().any(|text| text.contains(words)),
        "no refusal says {words:?}: {texts:?}"
    );
}

/// A brief that leases its own paths, so several starts in one world do not collide on one lease.
fn brief_writing(n: usize) -> Value {
    brief(json!({ "files": [{ "path": format!("work/{n}/**"), "mode": "write" }] }))
}

async fn routed(w: &World, n: usize) -> Value {
    let (answer, error) = w.start(brief_writing(n), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["routed"], true, "{answer}");
    answer
}

#[tokio::test]
async fn a_start_remembers_the_pin_and_a_fast_forward_moves_it() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let project = project_of(&w);
    let first = head(&w.repo);
    routed(&w, 1).await;
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert_eq!(view.current_ref.as_deref(), Some("origin/main"));
    assert_eq!(view.stored_ref.as_deref(), Some("origin/main"));
    assert_eq!(view.stored_commit.as_deref(), Some(first.as_str()));
    assert!(view.accepted && view.blocked.is_none());

    // A normal fast-forward is accepted on the next start, and the memory moves with it.
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let second = head(&w.repo);
    assert_ne!(first, second);
    routed(&w, 2).await;
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert_eq!(view.stored_commit.as_deref(), Some(second.as_str()));

    // Rewinding the ref to the first commit is not a descendant of the newest accepted one: block, never read HEAD.
    run_git(&w.repo, &["update-ref", "refs/remotes/origin/main", &first]);
    let (answer, error) = w.start(brief_writing(3), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "default branch changed");
    says(&texts, "does not descend");
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(!view.accepted && !view.missing);
    assert_eq!(view.stored_commit.as_deref(), Some(second.as_str()));
    assert_eq!(view.current_commit.as_deref(), Some(first.as_str()));
}

#[tokio::test]
async fn removing_the_remote_blocks_until_a_person_confirms_the_local_branch() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    run_git(
        &w.repo,
        &[
            "config",
            "remote.origin.url",
            "https://example.test/repo.git",
        ],
    );
    let project = project_of(&w);
    routed(&w, 1).await;
    let tip = head(&w.repo);

    // With no remote, the same commit is read from the local branch, but the remembered `origin/main` is gone.
    run_git(&w.repo, &["config", "--unset", "remote.origin.url"]);
    run_git(
        &w.repo,
        &["update-ref", "--no-deref", "-d", "refs/remotes/origin/HEAD"],
    );
    run_git(&w.repo, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let (answer, error) = w.start(brief_writing(2), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "Orbyn remembered the default branch origin/main");
    says(&texts, "It is gone now");
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(view.missing && !view.accepted);
    assert_eq!(view.stored_ref.as_deref(), Some("origin/main"));
    assert_eq!(view.stored_commit.as_deref(), Some(tip.as_str()));
    // The local branch the checkout reads now is still shown, so the person can confirm that commit.
    assert_eq!(view.current_ref.as_deref(), Some("main"));
    assert_eq!(view.current_commit.as_deref(), Some(tip.as_str()));

    // A person confirms the commit they saw; the memory follows to the ref the checkout has now.
    let view = w
        .core
        .crew_confirm_pinned_ref(&project, &tip)
        .await
        .unwrap();
    assert!(view.accepted);
    assert_eq!(view.stored_ref.as_deref(), Some("main"));
    assert_eq!(view.stored_commit.as_deref(), Some(tip.as_str()));
    routed(&w, 3).await;
}

#[tokio::test]
async fn deleting_the_ref_blocks_every_engine_and_a_reset_clears_the_memory() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    run_git(
        &w.repo,
        &[
            "config",
            "remote.origin.url",
            "https://example.test/repo.git",
        ],
    );
    let project = project_of(&w);
    routed(&w, 1).await;
    run_git(
        &w.repo,
        &["update-ref", "--no-deref", "-d", "refs/remotes/origin/HEAD"],
    );
    run_git(&w.repo, &["update-ref", "-d", "refs/remotes/origin/main"]);

    let (answer, error) = w.start(brief_writing(2), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "default branch");
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(!view.accepted);
    assert!(view.blocked.is_some(), "{view:?}");
    assert_eq!(view.stored_ref.as_deref(), Some("origin/main"));

    // Reset clears what was remembered; the still-unreadable origin is what blocks now.
    let view = w.core.crew_reset_pinned_ref(&project).await.unwrap();
    assert!(view.stored_ref.is_none() && view.stored_commit.is_none());
    assert!(!view.accepted);
    assert!(w.core.store.crew_pinned_ref(&project).unwrap().is_none());
}

#[tokio::test]
async fn a_local_branch_named_origin_main_does_not_stand_in_for_the_remote_ref() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    run_git(
        &w.repo,
        &[
            "config",
            "remote.origin.url",
            "https://example.test/repo.git",
        ],
    );
    let project = project_of(&w);
    routed(&w, 1).await;

    // Remove the remote, then leave a local branch with the remote ref's own short name.
    run_git(&w.repo, &["config", "--unset", "remote.origin.url"]);
    run_git(
        &w.repo,
        &["update-ref", "--no-deref", "-d", "refs/remotes/origin/HEAD"],
    );
    run_git(&w.repo, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let main = git_out(&w.repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
    run_git(&w.repo, &["branch", "-q", "-m", &main, "renamed"]);
    run_git(&w.repo, &["branch", "-q", "-m", "renamed", "origin/main"]);
    run_git(&w.repo, &["config", "init.defaultBranch", "origin/main"]);
    assert_eq!(
        git_out(&w.repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "origin/main"
    );

    let (answer, error) = w.start(brief_writing(2), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "Orbyn remembered the default branch origin/main");
    says(&texts, "It is gone now");
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(view.missing && !view.accepted);
    assert_eq!(view.current_ref.as_deref(), Some("origin/main"));
}

#[tokio::test]
async fn losing_the_local_default_branch_with_no_remote_blocks() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    let project = project_of(&w);
    routed(&w, 1).await;
    let branch = git_out(&w.repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
    // A detached HEAD and no local default branch leaves nothing to read, but the project remembered one.
    run_git(&w.repo, &["checkout", "-q", "--detach"]);
    run_git(&w.repo, &["branch", "-q", "-D", &branch]);
    let (answer, error) = w.start(brief_writing(2), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "Orbyn remembered the default branch");
    says(&texts, "It is gone now");
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(view.missing && !view.accepted);
    assert_eq!(view.stored_ref.as_deref(), Some(branch.as_str()));
    assert!(view.current_ref.is_none());
}

#[tokio::test]
async fn a_broken_stored_row_and_a_failed_write_block_instead_of_opening() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let project = project_of(&w);
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO project_preferences (project_id, key, value) VALUES (?1, 'crew_pinned_ref', 'not json')",
        rusqlite::params![project],
    )
    .unwrap();
    assert!(w.core.crew_pinned_ref_view(&project).await.is_err());
    let (answer, error) = w.start(brief_writing(1), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "cannot be read");
    says(&texts, "cannot tell whether a ref was weakened");

    // A failed write is the same: the task does not start on a ref it could not remember.
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_pin BEFORE INSERT ON project_preferences WHEN NEW.key = 'crew_pinned_ref'
         BEGIN SELECT RAISE(ABORT, 'fixture pin'); END;",
    )
    .unwrap();
    let (answer, error) = w.start(brief_writing(1), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "cannot be remembered");
}

#[tokio::test]
async fn the_dry_run_reads_the_default_branch_but_never_remembers_it() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let project = project_of(&w);
    let (answer, error) = w
        .call(
            &w.lead_token,
            "crew_route",
            json!({ "brief": brief_writing(1) }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["dry_run"], true, "{answer}");
    assert!(w.core.store.crew_pinned_ref(&project).unwrap().is_none());
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(view.stored_ref.is_none() && view.stored_commit.is_none());
}

#[tokio::test]
async fn a_rewritten_default_branch_blocks_until_a_person_confirms_that_commit() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let project = project_of(&w);
    routed(&w, 1).await;
    let accepted = head(&w.repo);

    // A commit with no history in common: the default branch was rewritten, not moved forward.
    run_git(&w.repo, &["checkout", "-q", "--orphan", "rewrite"]);
    commit_crew(
        &w.repo,
        &file(vec![rule("keep", &["src/**"], &["alpha", "bravo"])]),
    );
    pin_origin_here(&w.repo);
    let rewritten = head(&w.repo);
    assert_ne!(accepted, rewritten);

    let (answer, error) = w.start(brief_writing(2), key()).await;
    assert!(!error, "{answer}");
    let texts = refused(&answer);
    says(&texts, "default branch changed");
    says(&texts, "does not descend");
    // The read-only views say the same as a start, instead of showing a clean card.
    let classes = w
        .core
        .crew_classes_view(None, Some(&project))
        .await
        .unwrap();
    assert!(
        classes
            .notes
            .iter()
            .any(|note| note.contains("default branch changed")),
        "{:?}",
        classes.notes
    );
    let test = w
        .core
        .crew_class_test(None, Some(&project), &["src/a.rs".to_owned()])
        .await
        .unwrap();
    assert!(test.paths[0].route_card.contains("default branch changed"));
    assert!(
        test.paths[0]
            .allowed_engines
            .as_ref()
            .is_some_and(Vec::is_empty)
    );

    // The view showed the rewritten commit; confirming the old one, or anything else, is stale.
    let stale = w
        .core
        .crew_confirm_pinned_ref(&project, &accepted)
        .await
        .unwrap_err();
    match stale {
        CoreError::Invalid(words) => assert!(words.contains("not the commit that was shown")),
        other => panic!("not a refusal: {other:?}"),
    }
    let view = w
        .core
        .crew_confirm_pinned_ref(&project, &rewritten)
        .await
        .unwrap();
    assert!(view.accepted);
    assert_eq!(view.stored_commit.as_deref(), Some(rewritten.as_str()));
    routed(&w, 3).await;
}

#[tokio::test]
async fn a_reset_accepts_the_default_branch_as_it_is_now() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let project = project_of(&w);
    routed(&w, 1).await;

    // Rewrite, then ignore the block rather than confirm it.
    run_git(&w.repo, &["checkout", "-q", "--orphan", "rewrite"]);
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    pin_origin_here(&w.repo);
    let rewritten = head(&w.repo);
    let (answer, _) = w.start(brief_writing(2), key()).await;
    assert_eq!(answer["routed"], false, "{answer}");

    let view = w.core.crew_reset_pinned_ref(&project).await.unwrap();
    assert!(view.accepted);
    assert!(view.stored_ref.is_none() && view.stored_commit.is_none());
    // The next start reads the ref the checkout has now and remembers it, as a first one would.
    routed(&w, 3).await;
    let view = w.core.crew_pinned_ref_view(&project).await.unwrap();
    assert!(view.accepted);
    assert_eq!(view.stored_commit.as_deref(), Some(rewritten.as_str()));
}
