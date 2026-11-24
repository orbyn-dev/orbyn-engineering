//! Crew lessons at rest (migration 80, KC5 slice 1): the round trip, the dedupe key, the caps that hide instead of
//! deleting, the audit rows, the table's own checks and the read source 1 uses. Migration 81 adds the brief's
//! counters: a use and a use time for a lesson a brief carried, and `left_out_of` for one its key check left out.
//! Migration 84 adds the person's `rev`, the 10 s Undo and the forgotten fingerprints.

use orbyn_contracts::{AnswerKey, Brief, CrewObject};
use serde_json::Value;

use crate::testing::TempDir;
use crate::util::{format_unix, unix_now};

use super::crew_lessons::REPO_CAP;
use super::*;

macro_rules! example {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../docs/crew/protocol/schemas/examples/",
            $name,
            ".json"
        ))
    };
}

fn object(text: &str) -> CrewObject {
    CrewObject::parse(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

fn brief() -> Brief {
    match object(example!("brief.valid.basic")) {
        CrewObject::Brief(brief) => brief,
        _ => unreachable!(),
    }
}

fn key() -> AnswerKey {
    match object(example!("answer-key.valid.basic")) {
        CrewObject::AnswerKey(key) => key,
        _ => unreachable!(),
    }
}

/// One project row, for the project preferences the "use lessons" switch uses.
fn seed_project(store: &Store, project: &str) {
    store
        .writer()
        .execute(
            "INSERT OR IGNORE INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch)
             VALUES (?1, 'p', ?2, 0, 1, 't', 0, 0)",
            rusqlite::params![project, format!("/{project}")],
        )
        .unwrap();
}

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn lesson(
    scope: &str,
    project: Option<&str>,
    source: &str,
    text: &str,
    fingerprint: Option<&str>,
) -> NewCrewLesson {
    NewCrewLesson {
        scope: scope.to_owned(),
        project_id: project.map(str::to_owned),
        source: source.to_owned(),
        text: text.to_owned(),
        fingerprint: fingerprint.map(str::to_owned),
        evidence: vec![LessonEvidence {
            kind: "task".to_owned(),
            id: "sha256:one".to_owned(),
        }],
        edited_by_person: source == "person",
        names_path: false,
    }
}

fn actions(store: &Store) -> Vec<String> {
    let conn = store.reader();
    let mut statement = conn
        .prepare("SELECT action FROM audit_events ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap()
}

/// One project, its lead thread, and a task with a run carrying `fingerprint`, by raw SQL.
fn seed_task(
    store: &Store,
    project: &str,
    thread: &str,
    n: u32,
    fingerprint: &str,
    created_at: &str,
) {
    let conn = store.writer();
    conn.execute(
        "INSERT OR IGNORE INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch)
         VALUES (?1, 'p', ?2, 0, 1, 't', 0, 0)",
        rusqlite::params![project, format!("/{project}")],
    )
    .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO threads (id, project_id, title, engine, cwd, created_at, updated_at)
         VALUES (?1, ?2, 'lead', 'fake', '/p', 't', 't')",
        rusqlite::params![thread, project],
    )
    .unwrap();
    let task = format!("sha256:{n:064x}");
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at)
         VALUES (?1, 'crew-1', ?2, ?3, ?3)",
        rusqlite::params![task, format!("run-{n}"), created_at],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_runs (task_id, lead_thread_id, member, fingerprint)
         VALUES (?1, ?2, 'lead-01', ?3)",
        rusqlite::params![task, thread, fingerprint],
    )
    .unwrap();
}

#[test]
fn migration_80_follows_79_and_a_lesson_round_trips_with_its_evidence() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&80));

    let (_dir, store) = open();
    let fingerprint = "a".repeat(64);
    let written = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "fingerprint",
            "Checks like `file_scope` failed on files under `docs` before: Files changed outside the paths you were given.",
            Some(&fingerprint),
        ))
        .unwrap()
        .expect("a new fingerprint is written");
    assert!(written.id.starts_with("les_"), "{}", written.id);
    assert_eq!(
        (
            written.scope.as_str(),
            written.project_id.as_deref(),
            written.source.as_str(),
            written.state.as_str(),
        ),
        ("repo", Some("p1"), "fingerprint", "active")
    );
    assert_eq!(written.fingerprint.as_deref(), Some(fingerprint.as_str()));
    assert_eq!(written.evidence.len(), 1);
    assert_eq!((written.uses, written.hidden_by_cap), (0, false));
    assert!(!written.edited_by_person && !written.names_path);
    assert_eq!(written.last_used_at, None);
    assert_eq!(written.created_at, written.updated_at);

    assert_eq!(
        store.crew_lessons("repo", Some("p1")).unwrap(),
        vec![written.clone()]
    );
    assert_eq!(
        store.crew_lesson(&written.id).unwrap(),
        Some(written.clone())
    );
    assert_eq!(
        store.crew_lessons("global", None).unwrap(),
        Vec::<CrewLesson>::new()
    );
    // The audit row came first, in the same transaction, and names the lesson it wrote.
    assert_eq!(actions(&store), ["crew.lesson.learn"]);
    let conn = store.reader();
    let (actor, target): (String, String) = conn
        .query_row("SELECT actor, target FROM audit_events", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(
        (actor.as_str(), target.as_str()),
        ("core", written.id.as_str())
    );
}

#[test]
fn the_same_fingerprint_is_never_rewritten() {
    let (_dir, store) = open();
    let fingerprint = "b".repeat(64);
    assert!(
        store
            .write_crew_lesson(lesson(
                "repo",
                Some("p1"),
                "fingerprint",
                "Checks like `file_scope` failed before.",
                Some(&fingerprint),
            ))
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .write_crew_lesson(lesson(
                "repo",
                Some("p1"),
                "fingerprint",
                "Another sentence, the same fingerprint.",
                Some(&fingerprint),
            ))
            .unwrap()
            .is_none()
    );
    assert_eq!(store.crew_lessons("repo", Some("p1")).unwrap().len(), 1);
    assert_eq!(actions(&store), ["crew.lesson.learn"]);
}

#[test]
fn a_person_lesson_has_no_fingerprint_and_round_trips_as_the_persons() {
    let (_dir, store) = open();
    let written = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "Use pnpm, not npm.",
            None,
        ))
        .unwrap()
        .expect("a person's lesson is always written");
    assert_eq!(written.fingerprint, None);
    assert!(written.edited_by_person);
    assert_eq!(actions(&store), ["crew.lesson.learn"]);
}

#[test]
fn the_fifty_first_repository_lesson_hides_the_oldest_unused_one_and_deletes_nothing() {
    let (_dir, store) = open();
    let base = unix_now() - 10_000;
    for n in 0..50 {
        store
            .write_crew_lesson(lesson(
                "repo",
                Some("p1"),
                "person",
                &format!("Lesson number {n}."),
                None,
            ))
            .unwrap()
            .expect("written");
        // Ordered creation times, so "oldest" is the number and not whichever insert ran first.
        store
            .writer()
            .execute(
                "UPDATE crew_lessons SET created_at = ?1 WHERE text = ?2",
                rusqlite::params![format_unix(base + n), format!("Lesson number {n}.")],
            )
            .unwrap();
    }
    // The oldest one was used, so the next oldest *unused* one is the one that hides.
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET uses = 3 WHERE text = 'Lesson number 0.'",
            [],
        )
        .unwrap();
    assert_eq!(
        store
            .crew_lessons("repo", Some("p1"))
            .unwrap()
            .iter()
            .filter(|lesson| lesson.hidden_by_cap)
            .count(),
        0
    );

    let newest = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "Lesson number 50.",
            None,
        ))
        .unwrap()
        .expect("written");
    assert!(
        !newest.hidden_by_cap,
        "the new lesson is not the one hidden"
    );
    let rows = store.crew_lessons("repo", Some("p1")).unwrap();
    assert_eq!(rows.len(), 51, "over the cap nothing is deleted");
    let hidden: Vec<&CrewLesson> = rows.iter().filter(|lesson| lesson.hidden_by_cap).collect();
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].text, "Lesson number 1.");
    assert!(
        rows.iter()
            .any(|lesson| lesson.text == "Lesson number 0." && !lesson.hidden_by_cap)
    );
    assert_eq!(
        actions(&store)
            .iter()
            .filter(|action| action.as_str() == "crew.lesson.hide")
            .count(),
        1
    );
}

#[test]
fn each_repository_keeps_its_own_fifty_and_the_twenty_first_global_lesson_hides() {
    let (_dir, store) = open();
    let base = unix_now() - 10_000;
    for n in 0..50 {
        store
            .write_crew_lesson(lesson(
                "repo",
                Some("p1"),
                "person",
                &format!("P1 lesson {n}."),
                None,
            ))
            .unwrap();
        store
            .writer()
            .execute(
                "UPDATE crew_lessons SET created_at = ?1 WHERE text = ?2",
                rusqlite::params![format_unix(base + n), format!("P1 lesson {n}.")],
            )
            .unwrap();
    }
    // Another repository starts from nothing: its cap is its own.
    for n in 0..50 {
        store
            .write_crew_lesson(lesson(
                "repo",
                Some("p2"),
                "person",
                &format!("P2 lesson {n}."),
                None,
            ))
            .unwrap();
    }
    assert_eq!(
        store
            .crew_lessons("repo", Some("p2"))
            .unwrap()
            .iter()
            .filter(|lesson| lesson.hidden_by_cap)
            .count(),
        0
    );

    for n in 0..20 {
        store
            .write_crew_lesson(lesson(
                "global",
                None,
                "person",
                &format!("Global lesson {n}."),
                None,
            ))
            .unwrap();
        store
            .writer()
            .execute(
                "UPDATE crew_lessons SET created_at = ?1 WHERE text = ?2",
                rusqlite::params![format_unix(base + n), format!("Global lesson {n}.")],
            )
            .unwrap();
    }
    store
        .write_crew_lesson(lesson("global", None, "person", "Global lesson 20.", None))
        .unwrap();
    let global = store.crew_lessons("global", None).unwrap();
    assert_eq!(global.len(), 21);
    assert_eq!(
        global.iter().filter(|lesson| lesson.hidden_by_cap).count(),
        1
    );
    assert_eq!(
        store
            .crew_lessons("repo", Some("p1"))
            .unwrap()
            .iter()
            .filter(|lesson| lesson.hidden_by_cap)
            .count(),
        0,
        "a global cap never hides a repository lesson"
    );
}

#[test]
fn the_table_refuses_a_bad_scope_pair_text_or_fingerprint() {
    let (_dir, store) = open();
    let insert = |scope: &str,
                  project: Option<&str>,
                  text: &str,
                  fingerprint: Option<&str>|
     -> rusqlite::Result<usize> {
        store.writer().execute(
            "INSERT INTO crew_lessons
                (id, scope, project_id, source, text, fingerprint, evidence, created_at, updated_at)
             VALUES ('x', ?1, ?2, 'person', ?3, ?4, '[]', 't', 't')",
            rusqlite::params![scope, project, text, fingerprint],
        )
    };
    assert!(insert("global", Some("p1"), "A lesson.", None).is_err());
    assert!(insert("repo", None, "A lesson.", None).is_err());
    assert!(insert("repo", Some("p1"), "", None).is_err());
    assert!(insert("repo", Some("p1"), &"x".repeat(301), None).is_err());
    assert!(insert("repo", Some("p1"), "A lesson.", Some("short")).is_err());
    assert!(insert("repo", Some("p1"), "A lesson.", None).is_ok());
}

#[test]
fn the_same_fingerprint_is_found_only_in_one_repository_and_within_sixty_days() {
    let (_dir, store) = open();
    let now = unix_now();
    let day = 24 * 60 * 60;
    let fingerprint = "c".repeat(64);
    seed_task(
        &store,
        "p1",
        "lead-p1",
        1,
        &fingerprint,
        &format_unix(now - 50 * day),
    );
    seed_task(
        &store,
        "p1",
        "lead-p1",
        2,
        &fingerprint,
        &format_unix(now - 2 * day),
    );
    seed_task(
        &store,
        "p1",
        "lead-p1",
        3,
        &fingerprint,
        &format_unix(now - 61 * day),
    );
    seed_task(
        &store,
        "p2",
        "lead-p2",
        4,
        &fingerprint,
        &format_unix(now - 2 * day),
    );
    // A different fingerprint on another task of the same repository is not a peer.
    seed_task(
        &store,
        "p1",
        "lead-p1",
        5,
        &"d".repeat(64),
        &format_unix(now - 2 * day),
    );

    let window = 60 * 24 * 60 * 60;
    let peers = store
        .crew_tasks_with_fingerprint("p1", &fingerprint, now - window, "")
        .unwrap();
    assert_eq!(
        peers,
        [format!("sha256:{:064x}", 1), format!("sha256:{:064x}", 2)]
    );
    // The task itself is left out, and an older peer can still be found around a newer one.
    let peers = store
        .crew_tasks_with_fingerprint(
            "p1",
            &fingerprint,
            now - window,
            &format!("sha256:{:064x}", 1),
        )
        .unwrap();
    assert_eq!(peers, [format!("sha256:{:064x}", 2)]);
}

#[test]
fn migration_81_adds_the_brief_counters_and_a_new_lesson_starts_at_zero() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&81));

    let (_dir, store) = open();
    let written = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "Use pnpm, not npm.",
            None,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(written.left_out_of, 0);
    assert_eq!(
        store.crew_lesson(&written.id).unwrap().unwrap().left_out_of,
        0
    );
}

#[test]
fn a_briefs_use_and_left_out_are_counted_in_the_transaction_that_stores_it() {
    let (_dir, store) = open();
    let used = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "The lesson this brief carries.",
            None,
        ))
        .unwrap()
        .unwrap();
    let left_out = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "The lesson this brief's key leaves out.",
            None,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(
        (used.uses, used.left_out_of, used.last_used_at),
        (0, 0, None)
    );

    let task = store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            std::slice::from_ref(&used.id),
            std::slice::from_ref(&left_out.id),
        )
        .unwrap();
    assert_eq!(task.id, brief().id.as_str());
    let carried = store.crew_lesson(&used.id).unwrap().unwrap();
    assert_eq!(carried.uses, 1);
    assert!(
        carried.last_used_at.is_some(),
        "a carried lesson is stamped used"
    );
    assert_eq!(carried.updated_at, carried.last_used_at.clone().unwrap());
    let omitted = store.crew_lesson(&left_out.id).unwrap().unwrap();
    assert_eq!((omitted.uses, omitted.left_out_of), (0, 1));

    // The same brief again is the same task, not a second use.
    store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            std::slice::from_ref(&used.id),
            std::slice::from_ref(&left_out.id),
        )
        .unwrap();
    assert_eq!(store.crew_lesson(&used.id).unwrap().unwrap().uses, 1);
    assert_eq!(
        store
            .crew_lesson(&left_out.id)
            .unwrap()
            .unwrap()
            .left_out_of,
        1
    );
}

#[test]
fn the_use_lessons_switch_is_on_until_a_person_turns_it_off() {
    let (_dir, store) = open();
    seed_project(&store, "p1");
    assert!(store.crew_use_lessons("p1").unwrap(), "it ships on");
    assert!(store.crew_use_lessons("no-such-project").unwrap());

    store.set_crew_use_lessons("p1", false, "user").unwrap();
    assert!(!store.crew_use_lessons("p1").unwrap());
    assert_eq!(actions(&store).last().unwrap(), "crew.lesson.settings");
    let stored: String = store
        .reader()
        .query_row(
            "SELECT value FROM project_preferences WHERE project_id = 'p1' AND key = 'crew_use_lessons'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, "off");

    // On is the default, so it leaves no row behind.
    store.set_crew_use_lessons("p1", true, "user").unwrap();
    assert!(store.crew_use_lessons("p1").unwrap());
    let rows: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM project_preferences WHERE key = 'crew_use_lessons'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);

    // A value this build does not understand reads off: it never injects a lesson the person did not choose.
    store
        .writer()
        .execute(
            "INSERT INTO project_preferences (project_id, key, value) VALUES ('p1', 'crew_use_lessons', 'maybe')",
            [],
        )
        .unwrap();
    assert!(!store.crew_use_lessons("p1").unwrap());
}

#[test]
fn a_write_keeps_at_most_five_evidence_ids_in_the_row_and_the_audit() {
    let (_dir, store) = open();
    let mut new = lesson("repo", Some("p1"), "person", "Use pnpm.", None);
    new.evidence = (0..6)
        .map(|n| LessonEvidence {
            kind: "task".to_owned(),
            id: format!("sha256:{n}"),
        })
        .collect();
    let written = store.write_crew_lesson(new).unwrap().unwrap();
    assert_eq!(written.evidence.len(), EVIDENCE_MAX);
    assert_eq!(written.evidence[4].id, "sha256:4");
    let detail: String = store
        .reader()
        .query_row(
            "SELECT detail FROM audit_events WHERE action = 'crew.lesson.learn'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        detail.contains("sha256:4") && !detail.contains("sha256:5"),
        "{detail}"
    );
}

#[test]
fn a_failed_lesson_counter_never_refuses_the_task() {
    let (_dir, store) = open();
    seed_project(&store, "p1");
    let used = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Use pnpm.", None))
        .unwrap()
        .unwrap();
    // A counter that cannot be written: the update is refused by the table itself.
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_counts BEFORE UPDATE ON crew_lessons
             BEGIN SELECT RAISE(ABORT, 'counter refused'); END;",
        )
        .unwrap();
    let task = store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            std::slice::from_ref(&used.id),
            &[],
        )
        .unwrap();
    assert_eq!(task.id, brief().id.as_str());
    assert!(
        store.crew_task(&task.id).unwrap().is_some(),
        "the task is stored"
    );
    assert_eq!(store.crew_lesson(&used.id).unwrap().unwrap().uses, 0);
}

#[test]
fn the_brief_read_holds_only_active_visible_lessons_newest_first() {
    let (_dir, store) = open();
    let a = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "First.", None))
        .unwrap()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let b = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Second.", None))
        .unwrap()
        .unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET state = 'disabled' WHERE id = ?1",
            [&a.id],
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let c = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Third.", None))
        .unwrap()
        .unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 1 WHERE id = ?1",
            [&c.id],
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let d = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Fourth.", None))
        .unwrap()
        .unwrap();
    let ids: Vec<String> = store
        .crew_lessons_for_brief("repo", Some("p1"))
        .unwrap()
        .into_iter()
        .map(|lesson| lesson.id)
        .collect();
    // Two are eligible, and the newer one comes first; the disabled and the hidden one are not there.
    assert_eq!(ids, [d.id, b.id]);
}

#[test]
fn a_counter_that_fails_after_another_succeeded_rolls_both_back() {
    let (_dir, store) = open();
    seed_project(&store, "p1");
    let first = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Use pnpm.", None))
        .unwrap()
        .unwrap();
    let second = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Run lint.", None))
        .unwrap()
        .unwrap();
    // Only the second counter is refused, so the first one has already been written when the failure comes.
    store
        .writer()
        .execute_batch(&format!(
            "CREATE TRIGGER refuse_second BEFORE UPDATE ON crew_lessons WHEN NEW.id = '{}'
             BEGIN SELECT RAISE(ABORT, 'counter refused'); END;",
            second.id
        ))
        .unwrap();
    let task = store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            &[first.id.clone(), second.id.clone()],
            &[],
        )
        .unwrap();
    assert!(store.crew_task(&task.id).unwrap().is_some());
    assert_eq!(store.crew_lesson(&first.id).unwrap().unwrap().uses, 0);
    assert_eq!(store.crew_lesson(&second.id).unwrap().unwrap().uses, 0);
}

fn assert_stale(error: CoreError) {
    match error {
        CoreError::Refused {
            status,
            code,
            message,
            detail,
        } => {
            assert_eq!(status, 409);
            assert_eq!(code, "stale");
            assert_eq!(message, "This lesson changed. Reload it and try again.");
            assert!(detail.is_null());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn migration_84_adds_rev_the_undo_and_the_forgotten_table() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&84));

    let (_dir, store) = open();
    let written = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "Use pnpm, not npm.",
            None,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(written.rev, 1);
    assert_eq!(written.deleted_at, None);
    let forgotten: i64 = store
        .reader()
        .query_row("SELECT COUNT(*) FROM crew_lessons_forgotten", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(forgotten, 0);
}

#[test]
fn a_soft_deleted_lesson_is_hidden_from_every_read_and_undo_restores_it_with_a_new_rev() {
    let (_dir, store) = open();
    let written = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "Use pnpm, not npm.",
            None,
        ))
        .unwrap()
        .unwrap();
    let deleted_at = store.delete_crew_lesson(&written.id, 1).unwrap();
    assert!(!deleted_at.is_empty());

    assert_eq!(store.crew_lesson(&written.id).unwrap(), None);
    assert!(store.crew_lessons("repo", Some("p1")).unwrap().is_empty());
    assert!(
        store
            .crew_lessons_for_brief("repo", Some("p1"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.crew_lessons_counts("p1").unwrap(), (0, 0, 0));
    let hidden = store
        .crew_lesson_including_deleted(&written.id)
        .unwrap()
        .unwrap();
    assert_eq!(hidden.rev, 1, "a soft delete keeps the rev the person saw");
    assert_eq!(hidden.deleted_at.as_deref(), Some(deleted_at.as_str()));

    let restored = store.undo_delete_crew_lesson(&written.id).unwrap();
    assert_eq!(restored.rev, 2);
    assert_eq!(restored.deleted_at, None);
    assert_eq!(
        store.crew_lesson(&written.id).unwrap().unwrap(),
        restored,
        "the restored lesson reads again"
    );
    assert_eq!(
        actions(&store),
        [
            "crew.lesson.learn",
            "crew.lesson.delete",
            "crew.lesson.undo"
        ]
    );
    assert!(matches!(
        store.undo_delete_crew_lesson(&written.id),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn undo_after_the_window_or_without_a_delete_is_not_found() {
    let (_dir, store) = open();
    let deleted = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Deleted.", None))
        .unwrap()
        .unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET deleted_at = ?1 WHERE id = ?2",
            rusqlite::params![format_unix(unix_now() - 60), &deleted.id],
        )
        .unwrap();
    assert!(matches!(
        store.undo_delete_crew_lesson(&deleted.id),
        Err(CoreError::NotFound(_))
    ));

    let live = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Live.", None))
        .unwrap()
        .unwrap();
    assert!(matches!(
        store.undo_delete_crew_lesson(&live.id),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn undo_into_a_refilled_repository_scope_hides_one_instead_of_exceeding_the_cap() {
    let (_dir, store) = open();
    let mut written = Vec::new();
    for n in 0..REPO_CAP {
        written.push(
            store
                .write_crew_lesson(lesson(
                    "repo",
                    Some("p1"),
                    "person",
                    &format!("Repository lesson {n}."),
                    None,
                ))
                .unwrap()
                .unwrap(),
        );
    }
    let undone = written[0].clone();
    store.delete_crew_lesson(&undone.id, undone.rev).unwrap();
    // The deleted lesson is not counted, so this write fills the scope back up to the cap.
    store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "person",
            "A replacement lesson.",
            None,
        ))
        .unwrap()
        .unwrap();

    let restored = store.undo_delete_crew_lesson(&undone.id).unwrap();
    assert_eq!(restored.deleted_at, None);
    assert!(
        !restored.hidden_by_cap,
        "the undone lesson is never the one hidden"
    );

    let rows = store.crew_lessons("repo", Some("p1")).unwrap();
    let visible: Vec<&CrewLesson> = rows.iter().filter(|lesson| !lesson.hidden_by_cap).collect();
    assert_eq!(visible.len(), REPO_CAP as usize);
    assert!(visible.iter().any(|lesson| lesson.id == undone.id));
    let hidden: Vec<&CrewLesson> = rows.iter().filter(|lesson| lesson.hidden_by_cap).collect();
    assert_eq!(hidden.len(), 1);
    assert_ne!(hidden[0].id, undone.id);
    assert_eq!(
        store.crew_lessons_counts("p1").unwrap(),
        (REPO_CAP, 0, 1),
        "the scope is still full, and the overflow is hidden rather than deleted"
    );

    let hide_rows: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE action = 'crew.lesson.hide' AND target = ?1",
            [&hidden[0].id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hide_rows, 1, "the hidden lesson has its cap audit row");
}

#[test]
fn a_wrong_rev_refuses_every_lesson_change_and_writes_nothing() {
    let (_dir, store) = open();
    let written = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Use pnpm.", None))
        .unwrap()
        .unwrap();
    assert_stale(
        store
            .edit_crew_lesson(&written.id, "Changed.", false, 9)
            .unwrap_err(),
    );
    assert_stale(
        store
            .set_crew_lesson_state(&written.id, false, 9)
            .unwrap_err(),
    );
    assert_stale(
        store
            .set_crew_lesson_state(&written.id, true, 9)
            .unwrap_err(),
    );
    assert_stale(store.promote_crew_lesson(&written.id, 9).unwrap_err());
    assert_stale(store.delete_crew_lesson(&written.id, 9).unwrap_err());
    let global = store
        .write_crew_lesson(lesson("global", None, "person", "Global.", None))
        .unwrap()
        .unwrap();
    assert_stale(store.demote_crew_lesson(&global.id, "p1", 9).unwrap_err());

    assert_eq!(store.crew_lesson(&written.id).unwrap().unwrap(), written);
    assert_eq!(
        store.crew_lesson(&global.id).unwrap().unwrap(),
        global,
        "a refused write leaves the lesson as it was"
    );
    assert_eq!(actions(&store), ["crew.lesson.learn", "crew.lesson.learn"]);
}

#[test]
fn a_rev_bumps_on_every_visible_change_and_not_on_use_counting_or_the_soft_delete() {
    let (_dir, store) = open();
    let written = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Use pnpm.", None))
        .unwrap()
        .unwrap();
    assert_eq!(written.rev, 1);

    let edited = store
        .edit_crew_lesson(&written.id, "Use pnpm, not npm.", false, 1)
        .unwrap();
    assert_eq!(edited.rev, 2);
    assert!(edited.edited_by_person);
    assert_eq!(edited.source, written.source);
    assert_eq!(edited.fingerprint, written.fingerprint);

    let off = store.set_crew_lesson_state(&written.id, false, 2).unwrap();
    assert_eq!((off.state.as_str(), off.rev), ("disabled", 3));
    let on = store.set_crew_lesson_state(&written.id, true, 3).unwrap();
    assert_eq!((on.state.as_str(), on.rev), ("active", 4));

    let promoted = store.promote_crew_lesson(&written.id, 4).unwrap();
    assert_eq!(
        (
            promoted.scope.as_str(),
            promoted.project_id.as_deref(),
            promoted.rev
        ),
        ("global", None, 5)
    );
    let demoted = store.demote_crew_lesson(&written.id, "p1", 5).unwrap();
    assert_eq!(
        (
            demoted.scope.as_str(),
            demoted.project_id.as_deref(),
            demoted.rev
        ),
        ("repo", Some("p1"), 6)
    );

    store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            std::slice::from_ref(&written.id),
            &[],
        )
        .unwrap();
    let used = store.crew_lesson(&written.id).unwrap().unwrap();
    assert_eq!(used.uses, 1);
    assert_eq!(used.rev, 6, "a brief's use count is not a lesson change");
    assert!(used.last_used_at.is_some());

    store.delete_crew_lesson(&written.id, 6).unwrap();
    assert_eq!(
        store
            .crew_lesson_including_deleted(&written.id)
            .unwrap()
            .unwrap()
            .rev,
        6,
        "the soft delete leaves the rev the person saw"
    );
    assert_eq!(
        store.undo_delete_crew_lesson(&written.id).unwrap().rev,
        7,
        "undo is a visible change"
    );
}

#[test]
fn counts_cover_the_repository_and_global_lessons_and_leave_deleted_out() {
    let (_dir, store) = open();
    let hidden = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Hidden.", None))
        .unwrap()
        .unwrap();
    let disabled = store
        .write_crew_lesson(lesson("repo", Some("p1"), "person", "Disabled.", None))
        .unwrap()
        .unwrap();
    store.set_crew_lesson_state(&disabled.id, false, 1).unwrap();
    let global = store
        .write_crew_lesson(lesson("global", None, "person", "Global.", None))
        .unwrap()
        .unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 1 WHERE id = ?1",
            [&hidden.id],
        )
        .unwrap();
    assert_eq!(store.crew_lessons_counts("p1").unwrap(), (1, 1, 1));

    store.delete_crew_lesson(&hidden.id, 1).unwrap();
    assert_eq!(
        store.crew_lessons_counts("p1").unwrap(),
        (1, 1, 0),
        "a deleted lesson is in no count"
    );
    assert_eq!(
        store.crew_lessons_counts("another-repo").unwrap(),
        (1, 0, 0),
        "another repository still sees the global lesson, never p1's"
    );
    assert_eq!(global.rev, 1);
}

#[test]
fn purge_after_the_undo_window_remembers_the_fingerprint_and_no_write_rewrites_it() {
    let (_dir, store) = open();
    let fingerprint = "e".repeat(64);
    let written = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "fingerprint",
            "Checks failed before.",
            Some(&fingerprint),
        ))
        .unwrap()
        .unwrap();
    store.delete_crew_lesson(&written.id, 1).unwrap();

    // A cutoff before the deletion leaves the row alone.
    assert!(
        store
            .purge_deleted_lessons(&format_unix(unix_now() - 60))
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .crew_lesson_including_deleted(&written.id)
            .unwrap()
            .is_some()
    );

    // A cutoff after the window purges it, remembers its key and audits the purge.
    let purged = store
        .purge_deleted_lessons(&format_unix(unix_now() + 60))
        .unwrap();
    assert_eq!(purged.as_slice(), std::slice::from_ref(&written.id));
    assert_eq!(
        store.crew_lesson_including_deleted(&written.id).unwrap(),
        None
    );
    assert_eq!(
        actions(&store),
        [
            "crew.lesson.learn",
            "crew.lesson.delete",
            "crew.lesson.purge"
        ]
    );
    let forgotten: String = store
        .reader()
        .query_row(
            "SELECT fingerprint FROM crew_lessons_forgotten",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(forgotten, fingerprint);

    let again = store
        .write_crew_lesson(lesson(
            "repo",
            Some("p1"),
            "fingerprint",
            "The same failure again.",
            Some(&fingerprint),
        ))
        .unwrap();
    assert_eq!(again, None, "a forgotten fingerprint is never rewritten");
    assert!(store.crew_lessons("repo", Some("p1")).unwrap().is_empty());
    assert_eq!(
        actions(&store),
        [
            "crew.lesson.learn",
            "crew.lesson.delete",
            "crew.lesson.purge"
        ],
        "a refused rewrite writes no learn row"
    );
}

#[test]
fn demoting_into_a_full_repository_hides_one_and_bumps_its_rev() {
    let (_dir, store) = open();
    for n in 0..50 {
        store
            .write_crew_lesson(lesson(
                "repo",
                Some("p1"),
                "person",
                &format!("Repository lesson {n}."),
                None,
            ))
            .unwrap()
            .unwrap();
    }
    let global = store
        .write_crew_lesson(lesson("global", None, "person", "Global lesson.", None))
        .unwrap()
        .unwrap();
    let demoted = store.demote_crew_lesson(&global.id, "p1", 1).unwrap();
    assert_eq!(
        (
            demoted.scope.as_str(),
            demoted.project_id.as_deref(),
            demoted.rev
        ),
        ("repo", Some("p1"), 2)
    );
    let rows = store.crew_lessons("repo", Some("p1")).unwrap();
    assert_eq!(rows.len(), 51);
    let hidden: Vec<&CrewLesson> = rows.iter().filter(|lesson| lesson.hidden_by_cap).collect();
    assert_eq!(hidden.len(), 1);
    assert_ne!(hidden[0].id, demoted.id, "the demoted lesson is not hidden");
    assert_eq!(
        hidden[0].rev, 2,
        "a cap hide is a visible change and bumps rev"
    );
    assert_eq!(store.crew_lessons_counts("p1").unwrap(), (50, 0, 1));
}
