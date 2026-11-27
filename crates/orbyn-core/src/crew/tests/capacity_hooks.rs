//! The two hooks of review capacity (slice 4l): the start check before a worker is granted and the provenance write after a
//! merge row is stored. Real `Core`, fake engines, real git in a temporary repository. Nothing is pushed.

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::ErrorCode;
use serde_json::json;

use super::autopilot_merge::{gitq, merging, passed};
use super::harness::{Turn, World, brief, key, world};
use crate::crew::capacity::{
    Author, CapacitySettings, NOTE_REF, Priority, Sampling, record_at_merge, record_provenance,
    retry_provenance,
};
use crate::crew::fixtures::merge_record;
use crate::crew::{MergeOutcome, Mode};
use crate::store::{AutopilotEvent, MergeRecord, StoredHunk};

const NOW: i64 = 1_800_000_000;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// Starts `n` tasks and stores one waiting Medium merge for each, so `n` merges wait in the lead's project.
async fn waiting(w: &World, n: usize) {
    for i in 0..n {
        let (answer, error) = w
            .start(
                brief(json!({ "files": [{ "path": format!("src/w{i}/**"), "mode": "write" }], "outcome": format!("Task {i}.") })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap();
        let mut merge = merge_record(task, "/repo", "main", "x");
        merge.risk = "medium".into();
        merge.crew_id = w.task(task).crew_id;
        w.core.store.add_crew_merge(&merge).unwrap();
    }
}

fn limit(w: &World, n: u32) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit: n,
                sampling: Sampling::Off,
                pause_enabled: true,
            },
        )
        .unwrap();
}

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

#[tokio::test]
async fn a_low_start_that_crew_began_is_refused_over_the_limit_with_the_waiting_count_and_the_limit()
 {
    let w = quiet().await;
    limit(&w, 2);
    waiting(&w, 3).await;
    let error = w
        .core
        .capacity_start_check(&project(&w), Some((Priority::Low, true)), None)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotAuthorized);
    assert!(error.message.contains("3 merges wait"), "{}", error.message);
    assert!(error.message.contains("limit is 2"), "{}", error.message);
}

#[tokio::test]
async fn at_the_limit_a_low_crew_start_goes() {
    let w = quiet().await;
    limit(&w, 3);
    waiting(&w, 3).await;
    w.core
        .capacity_start_check(&project(&w), Some((Priority::Low, true)), None)
        .unwrap();
}

#[tokio::test]
async fn a_person_started_task_and_a_medium_or_high_task_go_over_the_limit() {
    let w = quiet().await;
    limit(&w, 1);
    waiting(&w, 3).await;
    let p = project(&w);
    for stored in [
        (Priority::Low, false),
        (Priority::Medium, true),
        (Priority::High, true),
    ] {
        w.core.capacity_start_check(&p, Some(stored), None).unwrap();
    }
}

#[tokio::test]
async fn crew_start_is_unchanged_for_now_because_a_new_task_has_no_stored_priority_yet() {
    let w = quiet().await;
    limit(&w, 1);
    waiting(&w, 3).await;
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/new/**", "mode": "write" }], "outcome": "One more." })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
}

#[tokio::test]
async fn with_the_preview_flag_off_the_start_check_does_nothing() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    limit(&w, 1);
    w.core
        .capacity_start_check(&project(&w), Some((Priority::Low, true)), None)
        .unwrap();
}

/// Waits for the background provenance write on the harness clock: a test's wait is not a product
/// budget, and a loaded machine must not turn a slow write into a missing one (#1063, tdd 3.7).
async fn hunks_of(w: &World, task: &str) -> Vec<StoredHunk> {
    let id = w.core.store.crew_merge(task).unwrap().unwrap().id;
    let give_up = std::time::Instant::now() + super::harness::WAIT_LIMIT;
    while std::time::Instant::now() < give_up {
        let hunks = w.core.store.crew_hunks_of_merge(id).unwrap();
        if !hunks.is_empty() {
            return hunks;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Vec::new()
}

/// Waits for a `provenance_failed` row on the task on the same test clock.
async fn failures_of(w: &World, task: &str) -> Vec<AutopilotEvent> {
    let give_up = std::time::Instant::now() + super::harness::WAIT_LIMIT;
    while std::time::Instant::now() < give_up {
        let rows: Vec<_> = w
            .core
            .store
            .crew_autopilot_events(task)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "provenance_failed")
            .collect();
        if !rows.is_empty() {
            return rows;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Vec::new()
}

/// The one row a read returned, unpacked by pattern so a test never reads a place in the list. A read that returns
/// none or several fails here with the rows attached.
fn one<'a, T: std::fmt::Debug>(rows: &'a [T], what: &str) -> &'a T {
    match rows {
        [row] => row,
        [] => panic!("no {what}: {rows:?}"),
        _ => panic!("{} {what}s where one is expected: {rows:?}", rows.len()),
    }
}

/// The stored hunk for `path`, found by the path it records: a merge can write more than one hunk, and the tests here
/// name the file they mean rather than its place in the read's path-then-line order.
fn hunk_at<'a>(hunks: &'a [StoredHunk], path: &str) -> &'a StoredHunk {
    hunks
        .iter()
        .find(|stored| stored.hunk.hunk.path == path)
        .expect("the stored hunk for that path")
}

/// The merge a one-link chain stores, so a test does not read place 0 of the chain's list.
fn one_link(merges: &[MergeRecord]) -> MergeRecord {
    match merges {
        [merge] => merge.clone(),
        _ => panic!("a one-link chain stores one merge, not {}", merges.len()),
    }
}

#[tokio::test]
async fn a_merge_through_the_queue_records_its_hunks_and_with_no_person_commit_or_worker_identity_every_line_is_unknown()
 {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let outcomes = m.process(NOW + 1).await;
    assert!(matches!(&outcomes[..], [MergeOutcome::Merged { .. }]));
    let hunks = hunks_of(&m.w, &id).await;
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    let hunk = hunk_at(&hunks, "src/feature.txt");
    // The fake engine's commit carries no worker identity and nothing is recorded as a person's: unconfirmed, counted
    // with the agent's lines and never a person's.
    assert!(matches!(
        &hunk.hunk.author,
        Author::Unknown { engine, .. } if engine == "alpha"
    ));
    let rows = m.w.core.store.crew_autopilot_events(&id).unwrap();
    assert!(
        rows.iter().all(|e| e.kind != "provenance_failed"),
        "a good write leaves no failure row"
    );
}

#[tokio::test]
async fn the_hook_reads_the_projects_recorded_person_commits() {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    assert!(matches!(
        &m.process(NOW + 1).await[..],
        [MergeOutcome::Merged { .. }]
    ));
    assert!(!hunks_of(&m.w, &id).await.is_empty());
    let merge = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    // The worker's commit is recorded as a person's by the app (as a future person-action path would), in this project.
    m.w.core
        .store
        .record_person_commit(&project(&m.w), &merge.candidate, "test_action", NOW)
        .unwrap();
    let agent = Author::Agent {
        engine: "alpha".into(),
        model: None,
    };
    record_provenance(&m.w.core.store, &merge, agent, NOW + 2)
        .await
        .unwrap();
    let hunks = m.w.core.store.crew_hunks_of_merge(merge.id).unwrap();
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    let hunk = hunk_at(&hunks, "src/feature.txt");
    assert_eq!(hunk.hunk.author, Author::Person);
}

#[tokio::test]
async fn a_failing_provenance_write_is_recorded_in_the_ledger_and_does_not_fail_or_undo_the_merge()
{
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    // Every hunk insert aborts, so the provenance write fails after the merge row is stored.
    m.w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER no_hunks BEFORE INSERT ON crew_hunks BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let before = m.tip().await;
    let outcomes = m.process(NOW + 1).await;
    assert!(matches!(&outcomes[..], [MergeOutcome::Merged { .. }]));
    assert_ne!(m.tip().await, before, "the branch moved");
    // The hunk insert is refused, so the task's ledger holds exactly one provenance failure row.
    let failed = failures_of(&m.w, &id).await;
    let failed = one(&failed, "provenance failure row");
    assert_eq!(failed.body["stage"], "hunks");
    assert_eq!(failed.body["resolved"], false);
    let merge = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    assert_eq!(merge.review_state, "waiting", "the merge row is intact");
    assert!(
        m.w.core
            .store
            .crew_hunks_of_merge(merge.id)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn provenance_for_a_range_git_cannot_read_returns_the_error_the_detached_task_discards() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let merge = merge_record("task", &w.repo.to_string_lossy(), "main", "x");
    let agent = Author::Agent {
        engine: "alpha".into(),
        model: None,
    };
    assert!(
        record_at_merge(&w.core.store, &merge, &agent, &[], NOW)
            .await
            .is_err()
    );
    assert!(
        record_provenance(&w.core.store, &merge, agent, NOW)
            .await
            .is_err()
    );
}

/// Commits `n` files in a chain on the world's repository and stores one merge row per link, each with its real
/// `before..merge` range, as if `n` merges had landed one after another. Returns the stored merges.
async fn chained_merges(w: &World, n: usize) -> Vec<MergeRecord> {
    let repo = w.repo.clone();
    let mut merges = Vec::new();
    for i in 0..n {
        let before = gitq(&repo, &["rev-parse", "HEAD"]).await;
        std::fs::write(
            repo.join(format!("src/chain{i}.txt")),
            format!("line {i}\nmore\n"),
        )
        .unwrap();
        gitq(&repo, &["add", "."]).await;
        gitq(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.test",
                "commit",
                "-q",
                "-m",
                &format!("chain {i}"),
            ],
        )
        .await;
        let after = gitq(&repo, &["rev-parse", "HEAD"]).await;
        let (answer, error) = w
            .start(
                brief(json!({ "files": [{ "path": format!("src/c{i}/**"), "mode": "write" }], "outcome": format!("Chain {i}.") })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap();
        let mut merge = merge_record(task, &repo.to_string_lossy(), "main", "x");
        merge.crew_id = w.task(task).crew_id;
        merge.before_commit = before;
        merge.merge_commit = after;
        merge.merged_unix = NOW;
        merge.id = w.core.store.add_crew_merge(&merge).unwrap();
        merges.push(merge);
    }
    merges
}

fn agent() -> Author {
    Author::Agent {
        engine: "claude".into(),
        model: None,
    }
}

#[tokio::test]
async fn merges_recorded_at_the_same_time_on_one_repository_each_keep_a_note_and_none_fails() {
    let w = quiet().await;
    let merges = chained_merges(&w, 8).await;
    let who = agent();
    let recorded = futures_util::future::join_all(
        merges
            .iter()
            .map(|m| record_at_merge(&w.core.store, m, &who, &[], NOW)),
    )
    .await;
    for (i, result) in recorded.into_iter().enumerate() {
        let recorded = result.unwrap_or_else(|e| panic!("merge {i}: {e}"));
        assert!(recorded.note.is_ok(), "merge {i}: {:?}", recorded.note);
    }
    let notes_ref = format!("--ref={NOTE_REF}");
    let listed = gitq(&w.repo, &["notes", &notes_ref, "list"]).await;
    assert_eq!(
        listed.lines().count(),
        8,
        "one note per merge commit: {listed}"
    );
    for merge in &merges {
        let body = gitq(&w.repo, &["notes", &notes_ref, "show", &merge.merge_commit]).await;
        let note: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(note["vcs"]["revision"], merge.merge_commit.as_str());
        // Each link wrote one file, so its merge stores one hunk; the test reads that one, not a list place.
        let stored = w.core.store.crew_hunks_of_merge(merge.id).unwrap();
        let stored = one(&stored, "stored hunk");
        assert_eq!(stored.note_ref.as_deref(), Some(NOTE_REF));
    }
    // The note history is a straight chain: no write was lost to a race.
    let commits = gitq(&w.repo, &["rev-list", "--count", NOTE_REF]).await;
    assert_eq!(commits.trim(), "8");
}

#[tokio::test]
async fn a_failure_row_has_a_fixed_reason_and_no_path_and_the_merge_reads_as_not_measured() {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    m.w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER no_hunks BEFORE INSERT ON crew_hunks BEGIN SELECT RAISE(ABORT, 'refused /secret/place'); END;",
        )
        .unwrap();
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    m.process(NOW + 1).await;
    let failed = failures_of(&m.w, &id).await;
    let failed = one(&failed, "provenance failure row");
    let text = failed.body.to_string();
    assert!(!text.contains("/secret"), "{text}");
    assert!(!text.contains(&*m.w.repo.to_string_lossy()), "{text}");
    assert_eq!(failed.body["stage"], "hunks");
    assert_eq!(failed.body["resolved"], false);
    assert_eq!(failed.body["coverage"], "Not measured");
    let coverage = m.w.core.crew_review_coverage(&project(&m.w), 0).unwrap();
    assert_eq!((coverage.merges, coverage.not_measured_merges), (0, 1));
    assert_eq!(coverage.agent_lines, 0);
}

#[tokio::test]
async fn a_git_failure_never_puts_git_output_or_a_path_in_the_row() {
    let w = quiet().await;
    let mut merge = one_link(&chained_merges(&w, 1).await);
    merge.repo = w
        .repo
        .join("no-such-dir-xyz")
        .to_string_lossy()
        .into_owned();
    assert!(
        record_provenance(&w.core.store, &merge, agent(), NOW)
            .await
            .is_err()
    );
    let rows = failures_of(&w, &merge.task_id).await;
    let row = one(&rows, "provenance failure row");
    assert!(!row.body.to_string().contains("no-such-dir-xyz"));
    assert!(row.body["why"].as_str().is_some_and(|why| !why.is_empty()));
}

#[tokio::test]
async fn a_failed_record_is_retried_once_the_cause_is_gone_and_is_then_marked_resolved() {
    let w = quiet().await;
    let merge = one_link(&chained_merges(&w, 1).await);
    w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER no_hunks BEFORE INSERT ON crew_hunks BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    assert!(
        record_provenance(&w.core.store, &merge, agent(), NOW)
            .await
            .is_err()
    );
    assert_eq!(failures_of(&w, &merge.task_id).await.len(), 1);
    // Still failing: a retry_failed row is written, the original stays unresolved.
    retry_provenance(&w.core.store).await;
    assert!(
        w.core
            .store
            .crew_hunks_of_merge(merge.id)
            .unwrap()
            .is_empty()
    );
    let rows = failures_of(&w, &merge.task_id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    // Each row names itself, so neither is read by its place in the oldest-first ledger.
    let original = rows
        .iter()
        .find(|row| row.body.get("retry_failed").is_none())
        .expect("the original failure's row");
    assert_eq!(
        original.body["resolved"], false,
        "the original stays unresolved"
    );
    let retried = rows
        .iter()
        .find(|row| row.body["retry_failed"] == true)
        .expect("the retry's row");
    assert_eq!(retried.body["attempt"], 1, "the first retry");
    w.core
        .store
        .reader()
        .execute_batch("DROP TRIGGER no_hunks;")
        .unwrap();
    retry_provenance(&w.core.store).await;
    let stored = w.core.store.crew_hunks_of_merge(merge.id).unwrap();
    let stored = one(&stored, "stored hunk");
    assert_eq!(stored.note_ref.as_deref(), Some(NOTE_REF));
    let rows = failures_of(&w, &merge.task_id).await;
    assert_eq!(rows.len(), 3, "{rows:?}");
    let resolved = rows
        .iter()
        .find(|row| row.body["resolved"] == true)
        .expect("the resolved row");
    assert_eq!(resolved.body["stage"], "hunks");
    // Resolved: a further pass changes nothing.
    retry_provenance(&w.core.store).await;
    assert_eq!(failures_of(&w, &merge.task_id).await.len(), 3);
    assert_eq!(w.core.store.crew_hunks_of_merge(merge.id).unwrap().len(), 1);
}

#[tokio::test]
async fn recording_the_same_merge_twice_replaces_its_hunks_instead_of_failing() {
    let w = quiet().await;
    let merge = one_link(&chained_merges(&w, 1).await);
    for _ in 0..2 {
        let recorded = record_at_merge(&w.core.store, &merge, &agent(), &[], NOW)
            .await
            .unwrap();
        assert!(recorded.note.is_ok());
    }
    assert_eq!(w.core.store.crew_hunks_of_merge(merge.id).unwrap().len(), 1);
}

#[tokio::test]
async fn a_failure_is_retried_three_times_then_left_so_it_cannot_hold_back_newer_ones() {
    let w = quiet().await;
    let merges = chained_merges(&w, 2).await;
    // The cap needs two different tasks: one fails and gives up after its retries, then another fails. Which chain
    // link either is does not matter, so the test takes one of them and finds the other by its id.
    let failing = merges.first().expect("a chain merge to fail");
    let later = merges
        .iter()
        .find(|merge| merge.id != failing.id)
        .expect("the other chain merge");
    w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER no_hunks BEFORE INSERT ON crew_hunks BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    assert!(
        record_provenance(&w.core.store, failing, agent(), NOW)
            .await
            .is_err()
    );
    for _ in 0..5 {
        retry_provenance(&w.core.store).await;
    }
    let rows = failures_of(&w, &failing.task_id).await;
    // The original and exactly three failed retries. The third retry names itself by its attempt, so the list's
    // oldest-first order is not what the test reads; the count pins that nothing followed it.
    assert_eq!(rows.len(), 4, "{rows:?}");
    let gave_up = rows
        .iter()
        .find(|row| row.body["attempt"] == 3)
        .expect("the third retry's row");
    assert!(
        gave_up.body["retry"]
            .as_str()
            .unwrap()
            .starts_with("gave up")
    );
    assert!(
        w.core
            .store
            .crew_unresolved_provenance_failures(10, 3)
            .unwrap()
            .is_empty()
    );
    // A newer failure is still picked up.
    assert!(
        record_provenance(&w.core.store, later, agent(), NOW)
            .await
            .is_err()
    );
    let pending = w
        .core
        .store
        .crew_unresolved_provenance_failures(10, 3)
        .unwrap();
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert_eq!(
        one(&pending, "unresolved provenance failure").task_id,
        later.task_id
    );
}

#[tokio::test]
async fn linked_worktrees_of_one_repository_share_the_note_lock_and_lose_no_note() {
    let w = quiet().await;
    let mut merges = chained_merges(&w, 8).await;
    let base = std::env::temp_dir().join(format!("orbyn-wt-{}-{}", std::process::id(), NOW));
    let mut paths = vec![w.repo.clone()];
    for name in ["a", "b"] {
        let path = base.join(name);
        std::fs::create_dir_all(&base).unwrap();
        gitq(
            &w.repo,
            &["worktree", "add", "-q", "--detach", &path.to_string_lossy()],
        )
        .await;
        paths.push(path);
    }
    for (i, merge) in merges.iter_mut().enumerate() {
        merge.repo = paths[i % 3].to_string_lossy().into_owned();
    }
    let who = agent();
    let recorded = futures_util::future::join_all(
        merges
            .iter()
            .map(|m| record_at_merge(&w.core.store, m, &who, &[], NOW)),
    )
    .await;
    for (i, result) in recorded.into_iter().enumerate() {
        let recorded = result.unwrap_or_else(|e| panic!("merge {i}: {e}"));
        assert!(recorded.note.is_ok(), "merge {i}: {:?}", recorded.note);
    }
    let notes_ref = format!("--ref={NOTE_REF}");
    let listed = gitq(&w.repo, &["notes", &notes_ref, "list"]).await;
    let count = gitq(&w.repo, &["rev-list", "--count", NOTE_REF]).await;
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(listed.lines().count(), 8, "{listed}");
    assert_eq!(count.trim(), "8");
}

#[tokio::test]
async fn a_retry_failure_row_never_copies_the_why_of_an_old_style_row() {
    let w = quiet().await;
    let merge = one_link(&chained_merges(&w, 1).await);
    // A row as written before the fixed reasons: its `why` is raw git output with an absolute path.
    w.core
        .store
        .add_crew_autopilot_event(
            &merge.task_id,
            &merge.crew_id,
            "provenance_failed",
            &json!({ "stage": "hunks", "why": "fatal: /Users/someone/secret/repo is not a repository", "resolved": false }),
        )
        .unwrap();
    w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER no_hunks BEFORE INSERT ON crew_hunks BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    retry_provenance(&w.core.store).await;
    let rows = failures_of(&w, &merge.task_id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let retried = rows
        .iter()
        .find(|row| row.body["retry_failed"] == true)
        .expect("the retry's row");
    assert_eq!(retried.body["attempt"], 1);
    assert!(
        !retried.body.to_string().contains("/Users/"),
        "{:?}",
        retried.body
    );
}
