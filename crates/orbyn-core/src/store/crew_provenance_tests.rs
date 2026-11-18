//! Provenance at rest and at a real merge: migration 48, the rows, one transaction, the start set once, and the local
//! note, all on temp git repositories built here. No engine runs.

use std::path::Path;

use rusqlite::params;

use super::*;
use crate::crew::capacity::{Author, AuthoredHunk, Hunk, NOTE_REF, hunks_of, record_at_merge};
use crate::crew::fixtures::{git, merge_record};
use crate::testing::TempDir;

/// File text that must never reach the note, built at run time.
fn canary() -> String {
    format!("canary-body-{}", 40 + 2)
}

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn hunk(path: &str, start: u32, len: u32) -> Hunk {
    Hunk {
        path: path.into(),
        start,
        len,
    }
}

fn agent() -> Author {
    Author::Agent {
        engine: "claude".into(),
        model: Some("model-1".into()),
    }
}

fn lines(prefix: &str, n: u32) -> String {
    (1..=n).map(|i| format!("{prefix}{i}\n")).collect()
}

/// A repository with a base commit, a feature branch of an agent commit and a person commit, and the `--no-ff` merge.
/// Returns the checkout and the commit before the merge and the merge commit.
fn merged_repo(root: &Path) -> (std::path::PathBuf, String, String) {
    let repo = root.join("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), lines("a", 10)).unwrap();
    std::fs::write(repo.join("gone.txt"), "x\n").unwrap();
    std::fs::write(repo.join("old_name.txt"), lines("o", 10)).unwrap();
    std::fs::write(repo.join("bin.dat"), [0u8, 1, 2, 3]).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);

    let mut a: Vec<String> = lines("a", 10).lines().map(str::to_owned).collect();
    a[1] = "changed 2".into();
    a[4] = "changed 5".into();
    std::fs::write(repo.join("a.txt"), a.join("\n") + "\n").unwrap();
    std::fs::write(
        repo.join("new.txt"),
        format!("{0}-1\n{0}-2\nthree\n", canary()),
    )
    .unwrap();
    std::fs::remove_file(repo.join("gone.txt")).unwrap();
    git(&repo, &["mv", "old_name.txt", "renamed.txt"]);
    let mut o: Vec<String> = lines("o", 10).lines().map(str::to_owned).collect();
    o[3] = "edited 4".into();
    std::fs::write(repo.join("renamed.txt"), o.join("\n") + "\n").unwrap();
    std::fs::write(repo.join("bin.dat"), [9u8, 0, 8, 0]).unwrap();
    std::fs::create_dir_all(repo.join("dir")).unwrap();
    std::fs::write(repo.join("dir/with space.txt"), "one\n").unwrap();
    std::fs::write(repo.join("\u{e9}.txt"), "accent\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &[
            "-c",
            "user.email=person@example.com",
            "commit",
            "-q",
            "-m",
            "agent work",
        ],
    );

    a[7] = "person edit 8".into();
    std::fs::write(repo.join("a.txt"), a.join("\n") + "\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &[
            "-c",
            "user.email=agent@example.com",
            "commit",
            "-q",
            "-m",
            "person fix",
        ],
    );
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    (repo, before, merge)
}

/// A stored merge row for the repository and the commits.
fn store_merge(store: &Store, n: u32, repo: &Path, before: &str, merge: &str) -> MergeRecord {
    let task = format!("sha256:{n:064x}");
    store
        .writer()
        .execute(
            "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', ?2, 't', 't')",
            params![task, format!("run-{n}")],
        )
        .unwrap();
    let mut record = merge_record(&task, &repo.to_string_lossy(), "main", "x");
    record.before_commit = before.to_owned();
    record.merge_commit = merge.to_owned();
    record.id = store.add_crew_merge(&record).unwrap();
    record
}

fn kinds(rows: &[StoredHunk]) -> Vec<(String, u32, u32, &'static str)> {
    rows.iter()
        .map(|r| {
            (
                r.hunk.hunk.path.clone(),
                r.hunk.hunk.start,
                r.hunk.hunk.len,
                if r.hunk.author == Author::Person {
                    "person"
                } else {
                    "agent"
                },
            )
        })
        .collect()
}

#[test]
fn migration_48_follows_47_and_the_tables_have_their_checks() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&48));
    let (_dir, store) = open();
    let conn = store.writer();
    let refused = conn.execute(
        "INSERT INTO crew_hunks (merge_id, path, start_line, line_count, author_kind) VALUES (999, 'a', 1, 1, 'agent')",
        [],
    );
    assert!(refused.is_err(), "a hunk needs a real merge");
    let started = conn.execute("INSERT INTO crew_provenance_start VALUES ('/r', 5)", []);
    assert_eq!(started.unwrap(), 1);
}

#[tokio::test]
async fn the_merged_range_gives_each_hunk_and_skips_deletes_and_binaries() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let mut found: Vec<(String, u32, u32)> = hunks_of(&repo, &before, &merge)
        .await
        .unwrap()
        .into_iter()
        .map(|h| (h.path, h.start, h.len))
        .collect();
    found.sort();
    let want: Vec<(String, u32, u32)> = [
        ("a.txt", 2, 1),
        ("a.txt", 5, 1),
        ("a.txt", 8, 1),
        ("dir/with space.txt", 1, 1),
        ("new.txt", 1, 3),
        ("renamed.txt", 4, 1),
        ("\u{e9}.txt", 1, 1),
    ]
    .into_iter()
    .map(|(p, s, l)| (p.to_owned(), s, l))
    .collect();
    assert_eq!(
        found, want,
        "gone.txt (deleted) and bin.dat (binary) give nothing"
    );
}

#[tokio::test]
async fn hunks_need_full_commit_ids() {
    let dir = TempDir::new();
    let (repo, _, merge) = merged_repo(dir.path());
    assert!(hunks_of(&repo, "--output=x", &merge).await.is_err());
    assert!(hunks_of(&repo, "HEAD", &merge).await.is_err());
}

#[tokio::test]
async fn a_merge_stores_agent_and_person_rows_and_a_local_note() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    // Only the person's commit is positively known. Emails are not read: the agent commit says "person" and the
    // person commit says "agent".
    let people = vec![git(&repo, &["rev-parse", "feature"])];
    let done = record_at_merge(&store, &record, &agent(), &people, 1000)
        .await
        .unwrap();
    assert_eq!(done.hunks, 7);
    done.note.expect("the note is written");

    let rows = store.crew_hunks_of_merge(record.id).unwrap();
    let got = kinds(&rows);
    assert!(got.contains(&("a.txt".into(), 2, 1, "agent")));
    assert!(got.contains(&("a.txt".into(), 5, 1, "agent")));
    assert!(got.contains(&("a.txt".into(), 8, 1, "person")));
    assert!(got.contains(&("new.txt".into(), 1, 3, "agent")));
    assert!(rows.iter().all(|r| r.note_ref.as_deref() == Some(NOTE_REF)));
    let first = &rows[0].hunk.author;
    assert_eq!(first, &agent());
    // A person's row carries no engine or model.
    let (engine, model): (Option<String>, Option<String>) = store
        .reader()
        .query_row(
            "SELECT engine, model FROM crew_hunks WHERE author_kind = 'person'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((engine, model), (None, None));

    // The note: valid JSON on the merge commit, ids and ranges only.
    let text = git(
        &repo,
        &["notes", &format!("--ref={NOTE_REF}"), "show", &merge],
    );
    let note: serde_json::Value = serde_json::from_str(&text).expect("the note is JSON");
    assert_eq!(note["vcs"]["revision"], merge);
    assert_eq!(note["counts"]["hunks"], 7);
    for marker in [
        canary().as_str(),
        "three",
        "agent work",
        "person fix",
        "changed 2",
    ] {
        assert!(!text.contains(marker), "the note must not hold {marker}");
    }
    let paths: Vec<&str> = note["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert!(
        paths
            .iter()
            .all(|p| !p.starts_with('/') && !p.contains(".."))
    );
    assert!(
        !text.contains(&repo.to_string_lossy().to_string()),
        "no absolute path"
    );
}

#[tokio::test]
async fn nobody_known_as_a_person_means_every_hunk_is_the_agents() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &record, &agent(), &[], 1)
        .await
        .unwrap()
        .note
        .unwrap();
    let rows = store.crew_hunks_of_merge(record.id).unwrap();
    assert!(rows.iter().all(|r| r.hunk.author == agent()));
}

#[tokio::test]
async fn the_start_is_set_once_and_a_second_merge_does_not_move_it() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let (_db, store) = open();
    assert_eq!(
        store
            .crew_provenance_began(&repo.to_string_lossy())
            .unwrap(),
        None
    );
    let first = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &first, &agent(), &[], 100)
        .await
        .unwrap();
    let second = store_merge(&store, 2, &repo, &before, &merge);
    record_at_merge(&store, &second, &agent(), &[], 900)
        .await
        .unwrap();
    assert_eq!(
        store
            .crew_provenance_began(&repo.to_string_lossy())
            .unwrap(),
        Some(100)
    );
    assert!(!store.crew_hunks_of_merge(second.id).unwrap().is_empty());
}

#[test]
fn a_failed_row_leaves_no_hunk_and_no_start_behind() {
    let (_db, store) = open();
    let repo = TempDir::new();
    let record = store_merge(&store, 1, repo.path(), &"a".repeat(40), &"b".repeat(40));
    let rows = vec![
        AuthoredHunk {
            hunk: hunk("a.txt", 1, 2),
            author: agent(),
        },
        AuthoredHunk {
            hunk: hunk("b.txt", 1, 2),
            author: Author::Person,
        },
        // The same path and start again: the primary key refuses it after two rows went in.
        AuthoredHunk {
            hunk: hunk("a.txt", 1, 5),
            author: agent(),
        },
    ];
    let repo_name = repo.path().to_string_lossy().to_string();
    assert!(
        store
            .record_crew_hunks(record.id, &repo_name, 50, &rows)
            .is_err()
    );
    assert!(store.crew_hunks_of_merge(record.id).unwrap().is_empty());
    assert_eq!(store.crew_provenance_began(&repo_name).unwrap(), None);
    // The merge row itself is untouched, and a good call afterwards works.
    store
        .record_crew_hunks(record.id, &repo_name, 60, &rows[..2])
        .unwrap();
    assert_eq!(store.crew_provenance_began(&repo_name).unwrap(), Some(60));
}

#[tokio::test]
async fn recording_the_same_merge_twice_replaces_its_hunks_and_keeps_the_first_start() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &record, &agent(), &[], 10)
        .await
        .unwrap();
    let first = store.crew_hunks_of_merge(record.id).unwrap();
    record_at_merge(&store, &record, &agent(), &[], 20)
        .await
        .unwrap();
    // A retry replaces the merge's hunks (no duplicate, no conflict) and never moves the start.
    assert_eq!(store.crew_hunks_of_merge(record.id).unwrap(), first);
    assert_eq!(
        store
            .crew_provenance_began(&repo.to_string_lossy())
            .unwrap(),
        Some(10)
    );
}

#[tokio::test]
async fn a_note_that_cannot_be_written_is_returned_not_raised_and_the_rows_stay() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    // A file where the notes folder must go makes the ref impossible to create.
    std::fs::write(repo.join(".git/refs/notes"), "not a folder").unwrap();
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    let done = record_at_merge(&store, &record, &agent(), &[], 5)
        .await
        .unwrap();
    assert!(done.note.is_err(), "the caller can log this");
    let rows = store.crew_hunks_of_merge(record.id).unwrap();
    assert_eq!(rows.len(), 7);
    assert!(
        rows.iter().all(|r| r.note_ref.is_none()),
        "no note, so no ref is claimed"
    );
}

#[tokio::test]
async fn the_note_ref_is_never_pushed_by_what_the_app_runs() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &record, &agent(), &[], 5)
        .await
        .unwrap()
        .note
        .unwrap();
    assert!(git(&repo, &["for-each-ref", NOTE_REF]).contains(NOTE_REF));

    // The app publishes with `git push -u origin <branch>` (source_control/scm/mod.rs) and fetches an explicit
    // `+refs/...:refs/heads/<head>`. Run the push and look at what the remote received.
    let remote = dir.path().join("remote.git");
    git(&remote, &["init", "-q", "--bare"]);
    git(
        &repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    let branch = git(&repo, &["branch", "--show-current"]);
    git(&repo, &["push", "-q", "-u", "origin", &branch]);
    let received = git(&remote, &["for-each-ref"]);
    assert!(received.contains("refs/heads/main"));
    assert!(
        !received.contains("notes"),
        "the remote must not receive the note ref"
    );
    let fetch = git(&repo, &["config", "--get-all", "remote.origin.fetch"]);
    assert_eq!(
        fetch, "+refs/heads/*:refs/remotes/origin/*",
        "no refs/* or notes refspec"
    );

    // And the code that pushes or fetches names no broad or notes refspec.
    for source in [
        include_str!("../source_control/scm/mod.rs"),
        include_str!("../source_control/scm/stack.rs"),
        include_str!("../source_control/scm/pulls.rs"),
    ] {
        for broad in [
            "refs/notes",
            "refs/*",
            "--mirror",
            "--all",
            "--tags",
            "--prune",
        ] {
            assert!(
                !source.contains(broad),
                "an SCM push or fetch mentions {broad}"
            );
        }
    }
}

/// An agent commit changes lines 3 to 7 and 10; a later person commit inserts three lines at the top and edits one
/// of the agent's lines. Returns the checkout, before, merge and the agent commit.
fn shifted_repo(root: &Path) -> (std::path::PathBuf, String, String, String) {
    let repo = root.join("shift");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), lines("a", 10)).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    let mut a: Vec<String> = lines("a", 10).lines().map(str::to_owned).collect();
    for i in [2, 3, 4, 5, 6, 9] {
        a[i] = format!("agent {i}");
    }
    std::fs::write(repo.join("a.txt"), a.join("\n") + "\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "agent work"]);
    let agent_commit = git(&repo, &["rev-parse", "HEAD"]);
    a[4] = "person edit".into();
    a.splice(0..0, ["p1".to_owned(), "p2".to_owned(), "p3".to_owned()]);
    std::fs::write(repo.join("a.txt"), a.join("\n") + "\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "person"]);
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    (repo, before, merge, agent_commit)
}

#[tokio::test]
async fn a_later_person_commit_that_shifts_lines_does_not_take_the_agents_hunk() {
    let dir = TempDir::new();
    let (repo, before, merge, _agent_commit) = shifted_repo(dir.path());
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    let person = git(&repo, &["rev-parse", "feature"]);
    record_at_merge(&store, &record, &agent(), &[person], 1)
        .await
        .unwrap()
        .note
        .unwrap();
    let got = kinds(&store.crew_hunks_of_merge(record.id).unwrap());
    assert_eq!(
        got,
        [
            ("a.txt".into(), 1, 3, "person"),
            ("a.txt".into(), 6, 2, "agent"),
            ("a.txt".into(), 8, 1, "person"),
            ("a.txt".into(), 9, 2, "agent"),
            ("a.txt".into(), 13, 1, "agent"),
        ]
    );
}

#[tokio::test]
async fn a_forged_email_on_an_agent_commit_stays_agent_and_no_recorded_commit_means_all_agent() {
    let dir = TempDir::new();
    let (repo, before, merge, agent_commit) = shifted_repo(dir.path());
    let (_db, store) = open();
    // The agent commit claims to be a person; nothing reads the email.
    git(&repo, &["config", "user.email", "x@example.com"]);
    let author = git(&repo, &["log", "-1", "--format=%ae", &agent_commit]);
    assert_ne!(author, "");
    let record = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &record, &agent(), &[], 1)
        .await
        .unwrap();
    let rows = store.crew_hunks_of_merge(record.id).unwrap();
    assert!(
        rows.iter().all(|r| r.hunk.author == agent()),
        "nothing recorded means nobody is a person"
    );
}

#[tokio::test]
async fn a_note_far_over_the_command_line_limit_is_written_and_read_back() {
    let dir = TempDir::new();
    let repo = dir.path().join("big");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("big.txt"), lines("l", 6000)).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    let changed: String = (1..=6000)
        .map(|i| {
            if i % 2 == 0 {
                format!("new{i}\n")
            } else {
                format!("l{i}\n")
            }
        })
        .collect();
    std::fs::write(repo.join("big.txt"), changed).unwrap();
    git(&repo, &["commit", "-q", "-am", "work"]);
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    let done = record_at_merge(&store, &record, &agent(), &[], 1)
        .await
        .unwrap();
    done.note.expect("a large note is written");
    assert_eq!(done.hunks, 3000);
    let text = git(
        &repo,
        &["notes", &format!("--ref={NOTE_REF}"), "show", &merge],
    );
    assert!(text.len() > 100_000, "{}", text.len());
    let note: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(note["counts"]["hunks"], 3000);
}

#[tokio::test]
async fn a_merge_with_no_hunk_has_no_note_and_no_note_ref() {
    let dir = TempDir::new();
    let repo = dir.path().join("empty");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "nothing"]);
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    let done = record_at_merge(&store, &record, &agent(), &[], 1)
        .await
        .unwrap();
    assert_eq!(done.hunks, 0);
    assert!(git(&repo, &["for-each-ref", NOTE_REF]).is_empty());
    assert!(store.crew_hunks_of_merge(record.id).unwrap().is_empty());
}

const SHIFTED: [(&str, u32, u32, &str); 5] = [
    ("a.txt", 1, 3, "person"),
    ("a.txt", 6, 2, "agent"),
    ("a.txt", 8, 1, "person"),
    ("a.txt", 9, 2, "agent"),
    ("a.txt", 13, 1, "agent"),
];

#[tokio::test]
async fn an_id_that_is_not_in_the_list_stays_agent_even_if_rewritten() {
    let dir = TempDir::new();
    let (repo, before, merge, _agent_commit) = shifted_repo(dir.path());
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    // The person's commit was rewritten, so the id the caller knows is no longer in the history.
    let rewritten = "c".repeat(40);
    let not_hex = "feature".to_owned();
    let list = [rewritten, not_hex];
    record_at_merge(&store, &record, &agent(), &list, 1)
        .await
        .unwrap();
    let rows = store.crew_hunks_of_merge(record.id).unwrap();
    assert!(rows.iter().all(|r| r.hunk.author == agent()));
    assert_eq!(
        rows.len(),
        3,
        "unsplit: the person's lines are the agent's too"
    );
}

#[tokio::test]
async fn a_repository_ignore_revs_file_cannot_reattribute_lines() {
    let dir = TempDir::new();
    let (repo, before, merge, _) = shifted_repo(dir.path());
    let person = git(&repo, &["rev-parse", "feature"]);
    std::fs::write(repo.join("ignore-revs"), format!("{person}\n")).unwrap();
    git(&repo, &["config", "blame.ignoreRevsFile", "ignore-revs"]);
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &record, &agent(), &[person], 1)
        .await
        .unwrap();
    assert_eq!(
        kinds(&store.crew_hunks_of_merge(record.id).unwrap()),
        SHIFTED.map(|(p, s, l, k)| (p.to_owned(), s, l, k))
    );
}

#[tokio::test]
async fn a_failed_blame_leaves_the_hunks_with_the_agent() {
    let dir = TempDir::new();
    let (repo, _before, merge, _) = shifted_repo(dir.path());
    let person: std::collections::HashSet<String> = [merge.clone()].into();
    let hunks = vec![hunk("missing.txt", 1, 3)];
    let out =
        crate::crew::capacity::attribute_by_blame(&repo, &merge, hunks, &person, &agent()).await;
    assert_eq!(
        out,
        [AuthoredHunk {
            hunk: hunk("missing.txt", 1, 3),
            author: agent()
        }]
    );
}

#[tokio::test]
async fn thousands_of_hunks_are_blamed_without_a_long_command_line() {
    let dir = TempDir::new();
    let repo = dir.path().join("big");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("big.txt"), lines("l", 6000)).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    let edit = |every: u32, tag: &str| -> String {
        (1..=6000)
            .map(|i| {
                if i % every == 0 {
                    format!("{tag}{i}\n")
                } else {
                    format!("l{i}\n")
                }
            })
            .collect()
    };
    std::fs::write(repo.join("big.txt"), edit(2, "agent")).unwrap();
    git(&repo, &["commit", "-q", "-am", "agent"]);
    // The person then rewrites line 2 only.
    let mut text = edit(2, "agent");
    text = text.replacen("agent2\n", "person2\n", 1);
    std::fs::write(repo.join("big.txt"), text).unwrap();
    git(&repo, &["commit", "-q", "-am", "person"]);
    let person = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    let (_db, store) = open();
    let record = store_merge(&store, 1, &repo, &before, &merge);
    let done = record_at_merge(&store, &record, &agent(), &[person], 1)
        .await
        .unwrap();
    done.note.unwrap();
    let rows = store.crew_hunks_of_merge(record.id).unwrap();
    assert_eq!(rows.len(), 3000);
    let people: Vec<u32> = rows
        .iter()
        .filter(|r| r.hunk.author == Author::Person)
        .map(|r| r.hunk.hunk.start)
        .collect();
    assert_eq!(people, [2]);
}

#[tokio::test]
async fn the_scratch_file_is_unique_inside_the_git_dir_and_gone_after_success_and_failure() {
    let dir = TempDir::new();
    let (repo, before, merge) = merged_repo(dir.path());
    let leftovers = |repo: &Path| -> Vec<String> {
        std::fs::read_dir(repo.join(".git"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("orbyn-note-"))
            .collect()
    };
    let (_db, store) = open();
    let first = store_merge(&store, 1, &repo, &before, &merge);
    record_at_merge(&store, &first, &agent(), &[], 1)
        .await
        .unwrap()
        .note
        .unwrap();
    assert!(leftovers(&repo).is_empty(), "removed after success");

    // A note that git refuses leaves nothing behind either.
    std::fs::remove_dir_all(repo.join(".git/refs/notes")).ok();
    let _ = std::fs::remove_file(repo.join(".git/refs/notes"));
    git(&repo, &["update-ref", "-d", NOTE_REF]);
    std::fs::remove_dir_all(repo.join(".git/refs/notes")).ok();
    std::fs::write(repo.join(".git/refs/notes"), "not a folder").unwrap();
    let second = store_merge(&store, 2, &repo, &before, &merge);
    let done = record_at_merge(&store, &second, &agent(), &[], 2)
        .await
        .unwrap();
    assert!(done.note.is_err());
    assert!(leftovers(&repo).is_empty(), "removed after failure");
}
