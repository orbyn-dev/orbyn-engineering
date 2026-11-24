//! Crew skill proposals at rest (migration 85, KC9 part 1): the group read, the idempotent create, the audited approve
//! and dismiss, the stale and not-open refusals, the source check and the table's own checks.

use super::crew_skill_proposals::proposal_group_key;
use super::*;
use crate::testing::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    store
        .writer()
        .execute(
            "INSERT INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch)
             VALUES ('p1', 'p', '/p1', 0, 1, 't', 0, 0)",
            [],
        )
        .unwrap();
    (dir, store)
}

fn actions(store: &Store) -> Vec<String> {
    let conn = store.reader();
    let mut statement = conn
        .prepare(
            "SELECT action FROM audit_events
             WHERE action LIKE 'crew.proposal.%' ORDER BY id",
        )
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn text(name: &str) -> String {
    format!("---\nname: {name}\ndescription: A template.\n---\n\n- A lesson.\n")
}

#[test]
fn migration_85_follows_84_and_a_proposal_round_trips_once_per_group() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&85));

    let (_dir, store) = open();
    let group = proposal_group_key("escape", None, Some("src"));
    let created = store
        .create_skill_proposal(NewCrewSkillProposal {
            project_id: "p1".to_owned(),
            group_key: group.clone(),
            text: text("crew-escape-src"),
            lessons: vec!["les_a".to_owned(), "les_b".to_owned(), "les_c".to_owned()],
        })
        .unwrap()
        .expect("a new group writes one proposal");
    assert!(created.id.starts_with("prop_"), "{}", created.id);
    assert_eq!(
        (
            created.project_id.as_str(),
            created.group_key.as_str(),
            created.state.as_str(),
            created.rev,
        ),
        ("p1", group.as_str(), "open", 1)
    );
    assert_eq!(
        created.lessons,
        ["les_a", "les_b", "les_c"],
        "ids only, in the order the group read gave"
    );
    assert!(created.skill_name.is_none() && created.approved_at.is_none());
    assert_eq!(created.created_at, created.updated_at);

    assert_eq!(store.skill_proposals("p1").unwrap(), vec![created.clone()]);
    assert_eq!(
        store.skill_proposal(&created.id).unwrap(),
        Some(created.clone())
    );
    assert_eq!(store.skill_proposals("p2").unwrap(), Vec::new());
    assert_eq!(actions(&store), ["crew.proposal.create"]);
    let (actor, target): (String, String) = {
        let conn = store.reader();
        conn.query_row("SELECT actor, target FROM audit_events", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
    };
    assert_eq!(
        (actor.as_str(), target.as_str()),
        ("core", created.id.as_str())
    );

    // The unique group makes a second create a no-op, and writes no second audit row.
    assert_eq!(
        store
            .create_skill_proposal(NewCrewSkillProposal {
                project_id: "p1".to_owned(),
                group_key: group,
                text: text("crew-escape-src"),
                lessons: vec!["les_d".to_owned()],
            })
            .unwrap(),
        None
    );
    assert_eq!(store.skill_proposals("p1").unwrap().len(), 1);
    assert_eq!(actions(&store), ["crew.proposal.create"]);

    // Another group is its own proposal.
    assert!(
        store
            .create_skill_proposal(NewCrewSkillProposal {
                project_id: "p1".to_owned(),
                group_key: proposal_group_key("revert", None, Some("docs")),
                text: text("crew-revert-docs"),
                lessons: vec!["les_e".to_owned(), "les_f".to_owned(), "les_g".to_owned()],
            })
            .unwrap()
            .is_some()
    );
    assert_eq!(store.skill_proposals("p1").unwrap().len(), 2);
}

#[test]
fn approve_and_dismiss_are_audited_first_and_refuse_a_stale_or_closed_proposal() {
    let (_dir, store) = open();
    let created = store
        .create_skill_proposal(NewCrewSkillProposal {
            project_id: "p1".to_owned(),
            group_key: proposal_group_key("escape", None, Some("src")),
            text: text("crew-escape-src"),
            lessons: vec!["les_a".to_owned(), "les_b".to_owned(), "les_c".to_owned()],
        })
        .unwrap()
        .unwrap();
    let path = store.skill_proposal(&created.id).unwrap().unwrap();

    // A stale screen writes nothing.
    let stale = store
        .approve_skill_proposal(
            &created.id,
            path.rev + 1,
            &text("crew-escape-src"),
            "crew-escape-src",
        )
        .unwrap_err();
    assert_stale(&stale);
    assert_eq!(store.skill_proposal(&created.id).unwrap().unwrap(), path);

    let approved = store
        .approve_skill_proposal(
            &created.id,
            created.rev,
            &text("crew-escape-src-edited"),
            "crew-escape-src",
        )
        .unwrap();
    assert_eq!(
        (
            approved.state.as_str(),
            approved.skill_name.as_deref(),
            approved.rev,
        ),
        ("approved", Some("crew-escape-src"), 2)
    );
    assert_eq!(approved.text, text("crew-escape-src-edited"));
    assert!(approved.approved_at.is_some());
    assert_eq!(approved.updated_at, approved.approved_at.clone().unwrap());
    assert_eq!(
        actions(&store),
        ["crew.proposal.create", "crew.proposal.approve"]
    );

    // An approved proposal is not open, whatever revision a screen sends.
    let not_open = store
        .dismiss_skill_proposal(&created.id, approved.rev)
        .unwrap_err();
    assert_not_open(&not_open);

    // Dismiss keeps the row and bumps the revision.
    let second = store
        .create_skill_proposal(NewCrewSkillProposal {
            project_id: "p1".to_owned(),
            group_key: proposal_group_key("revert", None, Some("docs")),
            text: text("crew-revert-docs"),
            lessons: vec!["les_e".to_owned()],
        })
        .unwrap()
        .unwrap();
    let dismissed = store
        .dismiss_skill_proposal(&second.id, second.rev)
        .unwrap();
    assert_eq!((dismissed.state.as_str(), dismissed.rev), ("dismissed", 2));
    assert_eq!(dismissed.skill_name, None);
    assert_eq!(
        actions(&store),
        [
            "crew.proposal.create",
            "crew.proposal.approve",
            "crew.proposal.create",
            "crew.proposal.dismiss",
        ]
    );
    assert_not_open(&store.dismiss_skill_proposal(&second.id, 1).unwrap_err());
}

#[test]
fn the_group_read_finds_three_active_lessons_and_skips_people_hidden_disabled_and_proposed_groups()
{
    let (_dir, store) = open();
    let seed = |source: &str, kind: Option<&str>, class: Option<&str>, text: &str| {
        store
            .write_crew_lesson(NewCrewLesson {
                scope: "repo".to_owned(),
                project_id: Some("p1".to_owned()),
                source: source.to_owned(),
                check_kind: kind.map(|value| value.to_owned()),
                path_class: class.map(|value| value.to_owned()),
                text: text.to_owned(),
                fingerprint: None,
                evidence: Vec::new(),
                edited_by_person: source == "person",
                names_path: false,
            })
            .unwrap()
            .unwrap()
    };

    // A ready group: three active escape lessons under `src`, newest first.
    let newest = seed("escape", None, Some("src"), "Escape lesson three.");
    let middle = seed("escape", None, Some("src"), "Escape lesson two.");
    let oldest = seed("escape", None, Some("src"), "Escape lesson one.");
    // Two active revert lessons are not three; one disabled and one hidden do not count.
    let revert = seed("revert", None, Some("docs"), "Revert lesson one.");
    let disabled = seed("revert", None, Some("docs"), "Revert lesson two.");
    store
        .set_crew_lesson_state(&disabled.id, false, disabled.rev)
        .unwrap();
    let hidden = seed("revert", None, Some("docs"), "Revert lesson three.");
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 1 WHERE id = ?1",
            [&hidden.id],
        )
        .unwrap();
    // A person's lesson has neither column and never groups.
    for n in 0..3 {
        seed("person", None, None, &format!("Person lesson {n}."));
    }
    // A soft-deleted lesson does not count either.
    let deleted = seed(
        "fingerprint",
        Some("command"),
        Some("src"),
        "Deleted lesson.",
    );
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET deleted_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
            [&deleted.id],
        )
        .unwrap();
    for n in 0..2 {
        seed(
            "fingerprint",
            Some("command"),
            Some("src"),
            &format!("Fingerprint lesson {n}."),
        );
    }

    let ready = store.crew_group_ready("p1").unwrap();
    assert_eq!(ready.len(), 1, "{ready:?}");
    let group = &ready[0];
    assert_eq!(
        (
            group.source.as_str(),
            group.check_kind.as_deref(),
            group.path_class.as_deref()
        ),
        ("escape", None, Some("src"))
    );
    assert_eq!(group.lessons.len(), 3);
    for lesson in [&newest, &middle, &oldest] {
        assert!(
            group.lessons.iter().any(|member| member.id == lesson.id),
            "{}",
            lesson.id
        );
    }

    // Once its proposal exists, the group is not ready again.
    store
        .create_skill_proposal(NewCrewSkillProposal {
            project_id: "p1".to_owned(),
            group_key: group.group_key.clone(),
            text: text("crew-escape-src"),
            lessons: group
                .lessons
                .iter()
                .map(|lesson| lesson.id.clone())
                .collect(),
        })
        .unwrap()
        .unwrap();
    assert!(store.crew_group_ready("p1").unwrap().is_empty());
    assert_eq!(store.crew_group_ready("p2").unwrap(), Vec::new());
    assert!(revert.id.starts_with("les_"));
}

#[test]
fn sources_are_ready_only_while_every_lesson_is_active_visible_and_live() {
    let (_dir, store) = open();
    let make = |text: &str| {
        store
            .write_crew_lesson(NewCrewLesson {
                scope: "repo".to_owned(),
                project_id: Some("p1".to_owned()),
                source: "escape".to_owned(),
                check_kind: None,
                path_class: Some("src".to_owned()),
                text: text.to_owned(),
                fingerprint: None,
                evidence: Vec::new(),
                edited_by_person: false,
                names_path: false,
            })
            .unwrap()
            .unwrap()
    };
    let one = make("Escape one.");
    let two = make("Escape two.");
    let three = make("Escape three.");
    let ids = vec![one.id.clone(), two.id.clone(), three.id.clone()];
    assert!(store.proposal_sources_ready(&ids).unwrap());
    assert!(!store.proposal_sources_ready(&[]).unwrap());

    store
        .set_crew_lesson_state(&two.id, false, two.rev)
        .unwrap();
    assert!(!store.proposal_sources_ready(&ids).unwrap());

    let third = store.crew_lesson(&three.id).unwrap().unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 1 WHERE id = ?1",
            [&third.id],
        )
        .unwrap();
    assert!(!store.proposal_sources_ready(&ids).unwrap());

    // A hard delete of a source leaves the proposal row untouched.
    let proposal = store
        .create_skill_proposal(NewCrewSkillProposal {
            project_id: "p1".to_owned(),
            group_key: proposal_group_key("escape", None, Some("src")),
            text: text("crew-escape-src"),
            lessons: ids.clone(),
        })
        .unwrap()
        .unwrap();
    store
        .writer()
        .execute("DELETE FROM crew_lessons WHERE id = ?1", [&one.id])
        .unwrap();
    assert!(!store.proposal_sources_ready(&ids).unwrap());
    assert_eq!(
        store.skill_proposal(&proposal.id).unwrap(),
        Some(proposal),
        "a purge never deletes a proposal"
    );
}

#[test]
fn the_table_refuses_a_bad_state_text_or_lessons() {
    let (_dir, store) = open();
    let insert = |id: &str,
                  group: &str,
                  state: &str,
                  text: &str,
                  lessons: &str|
     -> rusqlite::Result<usize> {
        store.writer().execute(
            "INSERT INTO crew_skill_proposals
                (id, project_id, group_key, state, text, lessons, rev, created_at, updated_at)
             VALUES (?1, 'p1', ?2, ?3, ?4, ?5, 1, 't', 't')",
            rusqlite::params![id, group, state, text, lessons],
        )
    };
    assert!(insert("a", "a", "open", "", "[]").is_err(), "empty text");
    assert!(
        insert("b", "b", "open", &"x".repeat(601), "[]").is_err(),
        "601"
    );
    assert!(
        insert("c", "c", "paused", "A text.", "[]").is_err(),
        "state"
    );
    assert!(
        insert("d", "d", "open", "A text.", "not json").is_err(),
        "json"
    );
    assert!(insert("e", "e", "open", "A text.", "[]").is_ok());
    assert!(
        insert("f", "e", "open", "Another text.", "[]").is_err(),
        "unique group"
    );
}

fn assert_stale(error: &CoreError) {
    match error {
        CoreError::Refused {
            status,
            code,
            message,
            ..
        } => assert_eq!(
            (*status, *code, message.as_str()),
            (
                409,
                "stale",
                "This proposal changed. Reload it and try again."
            )
        ),
        other => panic!("{other:?}"),
    }
}

fn assert_not_open(error: &CoreError) {
    match error {
        CoreError::Refused {
            status,
            code,
            message,
            ..
        } => assert_eq!(
            (*status, *code, message.as_str()),
            (
                409,
                "not_open",
                "This proposal is not open. Reload it and try again."
            )
        ),
        other => panic!("{other:?}"),
    }
}
