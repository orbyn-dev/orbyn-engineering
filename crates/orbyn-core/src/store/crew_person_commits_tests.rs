//! The person signal at rest and at a real merge: migration 55, the `crew_person_commits` table and its audit row, the
//! read a merge is attributed against, and the three-valued attribution on temp git repositories built here (worker
//! address, trailer, author against committer, forged addresses, a person's commit, the app's conflict resolution).
//! No engine runs.

use std::path::Path;

use rusqlite::{Connection, params};

use super::*;
use crate::crew::capacity::{Author, NOTE_REF, record_at_merge};
use crate::crew::fixtures::{git, merge_record};
use crate::testing::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn task_id(n: u32) -> String {
    format!("sha256:{n:064x}")
}

fn add_task(store: &Store, n: u32) -> String {
    let id = task_id(n);
    store
        .writer()
        .execute(
            "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', ?2, 't', 't')",
            params![id, format!("run-{n}")],
        )
        .unwrap();
    id
}

fn project(store: &Store, path: &str) -> String {
    store
        .add_project(NewProject {
            path,
            name: "p",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id
}

fn lead_thread(store: &Store, project_id: &str) -> String {
    store
        .create_thread(NewThread {
            project_id: project_id.to_owned(),
            title: "lead".into(),
            engine: "fake".into(),
            model: None,
            cwd: "/repo".into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id
}

fn commit_id(n: u8) -> String {
    format!("{n:02x}").repeat(20)
}

#[test]
fn migration_55_follows_54_keeps_old_hunks_and_adds_the_table() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1), "contiguous");
    assert!(versions.contains(&55));

    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)").unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 54) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    // A merge with an old hunk, written before this migration.
    let task = task_id(1);
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'c', 'r', 't', 't')",
        [&task],
    )
    .unwrap();
    let record = merge_record(&task, "/repo", "main", "x");
    conn.execute(
        "INSERT INTO crew_merges (task_id, crew_id, repo, branch, before_commit, merge_commit, candidate, risk, reasons, areas, engine, mode, files, lines, proof, not_verified, merged_unix, review_state, watch)
         VALUES (?4,'c','/repo','main',?1,?2,?3,'high','[]','[]','claude','full',1,1,'{}','[]',1,'waiting','unknown')",
        params![record.before_commit, record.merge_commit, record.candidate, task],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_hunks (merge_id, path, start_line, line_count, author_kind, engine, model, note_ref)
         VALUES (1, 'a.rs', 3, 2, 'agent', 'claude', 'm', 'refs/notes/x')",
        [],
    )
    .unwrap();
    drop(conn);

    let store = Store::open(&path, "upgrade").unwrap();
    let upgrade = store.upgrade().expect("55 applies on an old database");
    assert_eq!((upgrade.from, upgrade.to), (54, 55));
    let rows = store.crew_hunks_of_merge(1).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].hunk.author,
        Author::Agent {
            engine: "claude".into(),
            model: Some("m".into())
        }
    );
    assert_eq!(rows[0].note_ref.as_deref(), Some("refs/notes/x"));
    // The rebuilt table takes the new kind and still refuses a bad one.
    let insert = |kind: &str| {
        store.writer().execute(
            "INSERT INTO crew_hunks (merge_id, path, start_line, line_count, author_kind, engine) VALUES (1, 'b.rs', 1, 1, ?1, 'claude')",
            [kind],
        )
    };
    assert!(insert("unknown").is_ok());
    assert!(insert("nobody").is_err());
    drop(store);
    assert!(Store::open(&path, "reopen").unwrap().upgrade().is_none());
}

#[test]
fn a_person_commit_is_stored_once_with_one_audit_row_and_read_back() {
    let (_db, store) = open();
    let p = project(&store, "/repo");
    let (a, b) = (commit_id(0xaa), commit_id(0xbb));
    store
        .record_person_commit(&p, &a, "project_created", 10)
        .unwrap();
    store
        .record_person_commit(&p, &b, "project_created", 20)
        .unwrap();
    store
        .record_person_commit(&p, &a, "project_created", 30)
        .unwrap();
    assert_eq!(store.crew_person_commits(&p).unwrap(), [a.clone(), b]);
    let audit: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.person_commit' AND target = ?1",
            [&a],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit, 1, "a repeat adds no second audit row");
    assert!(store.crew_person_commits("other").unwrap().is_empty());
}

#[test]
fn a_person_commit_needs_a_full_lowercase_id_and_a_plain_action_and_leaves_no_row_when_refused() {
    let (_db, store) = open();
    let p = project(&store, "/repo");
    for bad in ["", "abc", &"A".repeat(40), &"g".repeat(40), &"a".repeat(41)] {
        assert!(
            store.record_person_commit(&p, bad, "x", 1).is_err(),
            "{bad}"
        );
    }
    for bad in ["", "Has Space", "x;y", &"x".repeat(65)] {
        assert!(
            store
                .record_person_commit(&p, &commit_id(1), bad, 1)
                .is_err(),
            "{bad}"
        );
    }
    assert!(store.crew_person_commits(&p).unwrap().is_empty());
    let audits: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.person_commit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audits, 0);
    // The table's own check holds even for a writer that skips the API.
    assert!(
        store
            .writer()
            .execute(
                "INSERT INTO crew_person_commits VALUES (?1, 'ABCD', 1, 'x')",
                [&p]
            )
            .is_err()
    );
}

#[test]
fn the_list_a_merge_reads_is_its_own_projects() {
    let (_db, store) = open();
    let (mine, theirs) = (project(&store, "/mine"), project(&store, "/theirs"));
    let (lead_mine, lead_theirs) = (lead_thread(&store, &mine), lead_thread(&store, &theirs));
    let (t1, t2, t3) = (
        add_task(&store, 1),
        add_task(&store, 2),
        add_task(&store, 3),
    );
    store.add_crew_run(&t1, &lead_mine, "m-1").unwrap();
    store.add_crew_run(&t2, &lead_theirs, "m-2").unwrap();
    store
        .record_person_commit(&mine, &commit_id(1), "project_created", 1)
        .unwrap();
    store
        .record_person_commit(&theirs, &commit_id(2), "project_created", 1)
        .unwrap();
    assert_eq!(
        store.crew_person_commits_for_task(&t1).unwrap(),
        [commit_id(1)]
    );
    assert_eq!(
        store.crew_person_commits_for_task(&t2).unwrap(),
        [commit_id(2)]
    );
    assert!(
        store.crew_person_commits_for_task(&t3).unwrap().is_empty(),
        "a task with no lead reads nothing"
    );
    assert!(
        store
            .crew_person_commits_for_task("nope")
            .unwrap()
            .is_empty()
    );
}

// ---- attribution on real repositories ----

const PERSON: &str = "person@example.com";

fn worker(task: &str) -> String {
    format!("worker+{task}@orbyn.invalid")
}

/// Commits `path` with `body` on the current branch: `author` and `committer` are emails; `message` is the full text.
fn commit_as(repo: &Path, path: &str, body: &str, author: &str, committer: &str, message: &str) {
    std::fs::write(repo.join(path), body).unwrap();
    git(repo, &["add", path]);
    git(
        repo,
        &[
            "-c",
            &format!("user.email={committer}"),
            "-c",
            "user.name=C",
            "commit",
            "-q",
            &format!("--author=A <{author}>"),
            "-m",
            message,
        ],
    );
}

fn lines_of(prefix: &str) -> String {
    format!("{prefix}1\n{prefix}2\n")
}

struct Repo {
    _dir: TempDir,
    path: std::path::PathBuf,
    before: String,
    merge: String,
    listed_person: String,
    listed_but_worker: String,
}

/// A feature branch of one commit per case, merged with `--no-ff` by the app's own identity, where `known` is the
/// task that exists and `ghost` one that does not.
fn scenario(known: &str, ghost: &str) -> Repo {
    let dir = TempDir::new();
    let repo = dir.path().join("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("c.txt"), "base\nsecond\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);

    commit_as(
        &repo,
        "w_author.txt",
        &lines_of("wa"),
        &worker(known),
        PERSON,
        "work",
    );
    commit_as(
        &repo,
        "w_committer.txt",
        &lines_of("wc"),
        PERSON,
        &worker(known),
        "work",
    );
    commit_as(
        &repo,
        "w_both.txt",
        &lines_of("wb"),
        &worker(known),
        &worker(known),
        "work",
    );
    commit_as(
        &repo,
        "trailer.txt",
        &lines_of("tr"),
        PERSON,
        PERSON,
        &format!("work\n\nOrbyn-Task: {known}"),
    );
    commit_as(
        &repo,
        "trailer_ghost.txt",
        &lines_of("tg"),
        PERSON,
        PERSON,
        &format!("work\n\nOrbyn-Task: {ghost}"),
    );
    commit_as(
        &repo,
        "trailer_text.txt",
        &lines_of("tt"),
        PERSON,
        PERSON,
        &format!("Orbyn-Task: {known} is in the subject, not a trailer"),
    );
    commit_as(
        &repo,
        "ghost_addr.txt",
        &lines_of("ga"),
        &worker(ghost),
        &worker(ghost),
        "work",
    );
    for (file, email) in [
        (
            "lookalike.txt",
            format!("worker+{known}@orbyn.invalid.example.com"),
        ),
        ("upper.txt", format!("Worker+{known}@orbyn.invalid")),
        ("short.txt", "worker+sha256:abc@orbyn.invalid".to_owned()),
    ] {
        commit_as(&repo, file, &lines_of(file), &email, &email, "work");
    }
    commit_as(
        &repo,
        "manual.txt",
        &lines_of("ma"),
        PERSON,
        PERSON,
        "by hand",
    );
    commit_as(
        &repo,
        "listed.txt",
        &lines_of("li"),
        PERSON,
        PERSON,
        "by the app",
    );
    let listed_person = git(&repo, &["rev-parse", "HEAD"]);
    commit_as(
        &repo,
        "listed_worker.txt",
        &lines_of("lw"),
        &worker(known),
        PERSON,
        "also listed",
    );
    let listed_but_worker = git(&repo, &["rev-parse", "HEAD"]);

    git(&repo, &["checkout", "-q", "main"]);
    // The conflict: both sides change line 1 of c.txt, and the app resolves it with its own identity. A trailer in
    // that message must not make the resolution the agent's.
    std::fs::write(repo.join("c.txt"), "main side\nsecond\n").unwrap();
    git(&repo, &["add", "c.txt"]);
    git(&repo, &["commit", "-q", "-m", "main edit"]);
    git(&repo, &["checkout", "-q", "feature"]);
    commit_as(
        &repo,
        "c.txt",
        "feature side\nsecond\n",
        PERSON,
        PERSON,
        "feature edit",
    );
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    let status = std::process::Command::new("git")
        .args(["merge", "--no-ff", "--no-commit", "feature"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(!status.status.success(), "the merge conflicts on c.txt");
    std::fs::write(repo.join("c.txt"), "resolved line\nsecond\n").unwrap();
    git(&repo, &["add", "c.txt"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Orbyn Crew",
            "-c",
            "user.email=crew@orbyn.invalid",
            "commit",
            "-q",
            "-m",
            &format!("merge\n\nOrbyn-Task: {known}"),
        ],
    );
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    Repo {
        _dir: dir,
        path: repo,
        before,
        merge,
        listed_person,
        listed_but_worker,
    }
}

fn agent() -> Author {
    Author::Agent {
        engine: "claude".into(),
        model: Some("model-1".into()),
    }
}

fn label(author: &Author) -> &'static str {
    match author {
        Author::Agent { .. } => "agent",
        Author::Person => "person",
        Author::Unknown { .. } => "unknown",
    }
}

async fn recorded(
    store: &Store,
    repo: &Repo,
    task: &str,
    people: &[String],
) -> Vec<(String, String)> {
    let mut record = merge_record(task, &repo.path.to_string_lossy(), "main", "x");
    record.before_commit.clone_from(&repo.before);
    record.merge_commit.clone_from(&repo.merge);
    record.id = store.add_crew_merge(&record).unwrap();
    let done = record_at_merge(store, &record, &agent(), people, 1000)
        .await
        .unwrap();
    done.note.expect("the note is written");
    store
        .crew_hunks_of_merge(record.id)
        .unwrap()
        .iter()
        .map(|row| {
            (
                row.hunk.hunk.path.clone(),
                label(&row.hunk.author).to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_line_is_agent_by_worker_address_or_trailer_person_only_when_listed_else_unknown() {
    let (_db, store) = open();
    let known = add_task(&store, 7);
    let ghost = task_id(8);
    let repo = scenario(&known, &ghost);
    let people = vec![repo.listed_person.clone(), repo.listed_but_worker.clone()];
    let got = recorded(&store, &repo, &known, &people).await;
    let of = |path: &str| {
        got.iter()
            .find(|(p, _)| p == path)
            .unwrap_or_else(|| panic!("no hunk for {path}: {got:?}"))
            .1
            .as_str()
    };
    // The worker address as author, as committer, or both; and the trailer on a real task.
    for path in [
        "w_author.txt",
        "w_committer.txt",
        "w_both.txt",
        "trailer.txt",
    ] {
        assert_eq!(of(path), "agent", "{path}");
    }
    // A trailer for a task that does not exist, a trailer outside the message's last block, a well-formed address of a
    // missing task and look-alike addresses: nothing is confirmed.
    for path in [
        "trailer_ghost.txt",
        "trailer_text.txt",
        "ghost_addr.txt",
        "lookalike.txt",
        "upper.txt",
        "short.txt",
        "manual.txt",
    ] {
        assert_eq!(of(path), "unknown", "{path}");
    }
    // Only the listed commit is a person's, and a listed commit that carries a worker address is the agent's.
    assert_eq!(of("listed.txt"), "person");
    assert_eq!(of("listed_worker.txt"), "agent");
    // The app's own conflict resolution is unknown even with a real task's trailer in its message.
    assert_eq!(of("c.txt"), "unknown");
    assert!(
        got.iter().filter(|(_, kind)| kind == "person").count() == 1,
        "nothing but the listed commit is a person: {got:?}"
    );
}

#[tokio::test]
async fn emails_alone_never_make_a_person_and_an_empty_list_leaves_agent_or_unknown() {
    let (_db, store) = open();
    let known = add_task(&store, 7);
    let repo = scenario(&known, &task_id(8));
    let got = recorded(&store, &repo, &known, &[]).await;
    assert!(!got.is_empty());
    assert!(got.iter().all(|(_, kind)| kind != "person"), "{got:?}");
    assert!(got.contains(&("manual.txt".into(), "unknown".into())));
    // A list entry that is not a full id never matches.
    let (_db2, store2) = open();
    let known2 = add_task(&store2, 7);
    let junk = vec![
        "HEAD".to_owned(),
        "listed".to_owned(),
        repo.listed_person[..12].to_owned(),
    ];
    let got = recorded(&store2, &repo, &known2, &junk).await;
    assert!(got.iter().all(|(_, kind)| kind != "person"), "{got:?}");
}

#[tokio::test]
async fn the_stored_note_and_rows_carry_unknown_with_the_tasks_engine() {
    let (_db, store) = open();
    let known = add_task(&store, 7);
    let repo = scenario(&known, &task_id(8));
    let mut record = merge_record(&known, &repo.path.to_string_lossy(), "main", "x");
    record.before_commit.clone_from(&repo.before);
    record.merge_commit.clone_from(&repo.merge);
    record.id = store.add_crew_merge(&record).unwrap();
    record_at_merge(&store, &record, &agent(), &[], 5)
        .await
        .unwrap()
        .note
        .unwrap();
    let (engine, model): (String, Option<String>) = store
        .reader()
        .query_row(
            "SELECT engine, model FROM crew_hunks WHERE author_kind = 'unknown' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (engine.as_str(), model.as_deref()),
        ("claude", Some("model-1"))
    );
    let text = git(
        &repo.path,
        &["notes", &format!("--ref={NOTE_REF}"), "show", &repo.merge],
    );
    assert!(text.contains("\"unknown\""));
}

#[tokio::test]
async fn a_trailer_holding_separator_bytes_cannot_forge_a_record_for_another_commit() {
    let (_db, store) = open();
    let known = add_task(&store, 7);
    let dir = TempDir::new();
    let repo = dir.path().join("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("c.txt"), "base\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit_as(&repo, "victim.txt", "v\n", PERSON, PERSON, "by hand");
    let victim = git(&repo, &["rev-parse", "HEAD"]);
    // The next commit's trailer tries to end its record and start one for the victim, with a real task.
    let forged = format!("work\n\nOrbyn-Task: {known}\x1e{victim}\x1f{known}");
    commit_as(&repo, "forger.txt", "f\n", PERSON, PERSON, &forged);
    git(&repo, &["checkout", "-q", "main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    let r = Repo {
        _dir: dir,
        path: repo,
        before,
        merge,
        listed_person: String::new(),
        listed_but_worker: String::new(),
    };
    let got = recorded(&store, &r, &known, &[]).await;
    let of = |path: &str| got.iter().find(|(p, _)| p == path).unwrap().1.clone();
    assert_eq!(of("victim.txt"), "unknown", "{got:?}");
}
