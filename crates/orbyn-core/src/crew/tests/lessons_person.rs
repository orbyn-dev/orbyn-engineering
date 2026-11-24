//! KC5 slice 3 on a real `Core`: a person's list, detail, add, edit, on/off, promote, demote, delete and undo. Every
//! method is behind Crew Preview, every write is audited, and a stale `rev` changes nothing. A lesson stays data in a
//! brief: nothing here routes, verifies or changes a check.

use std::sync::Arc;

use orbyn_contracts::{AnswerKey, Brief, CrewObject};
use serde_json::Value;

use super::harness::{Turn, World, world};
use crate::crew::lessons::LessonScope;
use crate::crew::lessons_view::LessonFilter;
use crate::error::CoreError;
use crate::store::{CrewLesson, LessonEvidence, NewCrewLesson};
use crate::util::{format_unix, parse_time, unix_now};

macro_rules! example {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../../docs/crew/protocol/schemas/examples/",
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

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

fn lesson_row(w: &World, id: &str) -> CrewLesson {
    w.core.store.crew_lesson(id).unwrap().unwrap()
}

fn lesson_row_opt(w: &World, id: &str) -> Option<CrewLesson> {
    w.core.store.crew_lesson(id).unwrap()
}

fn actions(w: &World) -> Vec<String> {
    let conn = w.core.store.reader();
    let mut statement = conn
        .prepare(
            "SELECT action FROM audit_events
             WHERE action LIKE 'crew.lesson.%' ORDER BY id",
        )
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap()
}

fn seed(
    w: &World,
    scope: &str,
    project: Option<&str>,
    source: &str,
    fingerprint: Option<&str>,
    text: &str,
) -> CrewLesson {
    w.core
        .store
        .write_crew_lesson(NewCrewLesson {
            scope: scope.to_owned(),
            project_id: project.map(str::to_owned),
            source: source.to_owned(),
            text: text.to_owned(),
            fingerprint: fingerprint.map(str::to_owned),
            evidence: Vec::new(),
            edited_by_person: source == "person",
            names_path: false,
        })
        .unwrap()
        .unwrap()
}

fn add_repo(w: &World, project: &str, text: &str) -> CrewLesson {
    w.core
        .crew_lesson_add(LessonScope::Repo(project.to_owned()), text)
        .unwrap()
}

fn add_global(w: &World, text: &str) -> CrewLesson {
    w.core.crew_lesson_add(LessonScope::Global, text).unwrap()
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

fn assert_preview_off<T: std::fmt::Debug>(result: crate::error::Result<T>) {
    let error = result.expect_err("Crew Preview is off");
    let CoreError::NotFound(what) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(what, "route");
    assert_eq!(error.status(), 404);
    assert_eq!(error.code(), "not_found");
    assert_eq!(error.to_string(), "route not found");
}

#[tokio::test]
async fn the_list_filters_counts_and_in_next_brief_follow_the_brief() {
    let w = quiet().await;
    let project = project_of(&w);
    // Fifty-one repository lessons: the cap hides one older visible lesson and deletes nothing.
    for n in 0..51 {
        seed(
            &w,
            "repo",
            Some(&project),
            "person",
            None,
            &format!("Repository lesson {n}."),
        );
    }
    let all_repo = w.core.store.crew_lessons("repo", Some(&project)).unwrap();
    let hidden = all_repo
        .iter()
        .find(|lesson| lesson.hidden_by_cap)
        .expect("the cap hides one")
        .clone();
    // Disable an older visible lesson, outside the newest five, so the newest five stay the ones a brief carries.
    let disabled = all_repo
        .iter()
        .rev()
        .find(|lesson| !lesson.hidden_by_cap)
        .expect("a visible lesson")
        .clone();
    w.core
        .crew_lesson_set_enabled(&disabled.id, false, disabled.rev)
        .unwrap();
    let global = add_global(&w, "Global lesson.");

    let list = w
        .core
        .crew_lessons_list(&project, LessonFilter::All)
        .unwrap();
    assert_eq!(list.repo, project);
    assert!(list.use_lessons);
    assert_eq!((list.brief_cap_lessons, list.brief_cap_chars), (5, 1500));
    assert_eq!(list.counts, (50, 1, 1));
    assert_eq!(list.lessons.len(), 52, "repo lessons then the global one");
    let last_repo = list
        .lessons
        .iter()
        .rposition(|view| view.lesson.scope == "repo")
        .unwrap();
    let global_at = list
        .lessons
        .iter()
        .position(|view| view.lesson.id == global.id)
        .unwrap();
    assert!(last_repo < global_at, "repository lessons come first");

    // The newest five active visible repository lessons would go into the brief; the sixth does not.
    let eligible: Vec<&CrewLesson> = all_repo
        .iter()
        .filter(|lesson| lesson.state == "active" && !lesson.hidden_by_cap)
        .collect();
    for lesson in eligible.iter().take(5) {
        let view = list
            .lessons
            .iter()
            .find(|view| view.lesson.id == lesson.id)
            .unwrap();
        assert!(view.in_next_brief, "{}", lesson.text);
    }
    let sixth = eligible[5];
    assert!(
        !list
            .lessons
            .iter()
            .find(|view| view.lesson.id == sixth.id)
            .unwrap()
            .in_next_brief,
        "the sixth-newest lesson stays out"
    );
    assert!(
        !list
            .lessons
            .iter()
            .find(|view| view.lesson.id == disabled.id)
            .unwrap()
            .in_next_brief,
        "a disabled lesson stays out"
    );
    assert!(
        !list
            .lessons
            .iter()
            .find(|view| view.lesson.id == hidden.id)
            .unwrap()
            .in_next_brief,
        "a hidden lesson stays out"
    );
    assert!(
        !list
            .lessons
            .iter()
            .find(|view| view.lesson.id == global.id)
            .unwrap()
            .in_next_brief,
        "the global lesson is sixth in the brief's order"
    );

    // The filters ask for one state and the hidden ones ask for the over-cap row.
    let active = w
        .core
        .crew_lessons_list(&project, LessonFilter::Active)
        .unwrap();
    assert_eq!(active.lessons.len(), 50);
    let only_disabled = w
        .core
        .crew_lessons_list(&project, LessonFilter::Disabled)
        .unwrap();
    assert_eq!(
        only_disabled
            .lessons
            .iter()
            .map(|view| view.lesson.id.as_str())
            .collect::<Vec<_>>(),
        [disabled.id.as_str()]
    );
    let hidden_list = w
        .core
        .crew_lessons_list(&project, LessonFilter::Hidden)
        .unwrap();
    assert_eq!(
        hidden_list
            .lessons
            .iter()
            .map(|view| view.lesson.id.as_str())
            .collect::<Vec<_>>(),
        [hidden.id.as_str()]
    );

    // A repository lesson ignores the `repo` query, even one that does not exist.
    let newest = eligible[0].id.as_str();
    let detail = w
        .core
        .crew_lesson_detail(newest, Some("no-such-repository"))
        .unwrap();
    assert!(detail.lesson.in_next_brief);

    // The switch off makes every lesson false; on flips the newest five back.
    w.core.crew_lessons_set_use(&project, false).unwrap();
    let off = w
        .core
        .crew_lessons_list(&project, LessonFilter::Active)
        .unwrap();
    assert!(!off.use_lessons);
    assert!(off.lessons.iter().all(|view| !view.in_next_brief));
    assert!(
        !w.core
            .crew_lesson_detail(newest, None)
            .unwrap()
            .lesson
            .in_next_brief
    );
    w.core.crew_lessons_set_use(&project, true).unwrap();
    let on = w
        .core
        .crew_lessons_list(&project, LessonFilter::Active)
        .unwrap();
    assert!(
        on.lessons
            .iter()
            .find(|view| view.lesson.id == newest)
            .unwrap()
            .in_next_brief,
        "the switch on carries the newest five again"
    );

    let missing = w
        .core
        .crew_lessons_list("no-such-repository", LessonFilter::All)
        .unwrap_err();
    assert_eq!(
        (missing.status(), missing.code(), missing.to_string()),
        (404, "not_found", "project not found".to_owned())
    );
    let missing_use = w
        .core
        .crew_lessons_set_use("no-such-repository", false)
        .unwrap_err();
    assert_eq!(
        (
            missing_use.status(),
            missing_use.code(),
            missing_use.to_string()
        ),
        (404, "not_found", "project not found".to_owned())
    );
}

#[tokio::test]
async fn a_global_lessons_detail_follows_the_repositorys_switch() {
    let w = quiet().await;
    let project = project_of(&w);
    let global = add_global(&w, "Global lesson.");

    let detail = w
        .core
        .crew_lesson_detail(&global.id, Some(&project))
        .unwrap();
    assert!(detail.lesson.in_next_brief);
    assert_eq!(detail.evidence, global.evidence);
    assert!(
        !w.core
            .crew_lesson_detail(&global.id, None)
            .unwrap()
            .lesson
            .in_next_brief,
        "without a repository the global answer is false"
    );

    w.core.crew_lessons_set_use(&project, false).unwrap();
    assert!(
        !w.core
            .crew_lesson_detail(&global.id, Some(&project))
            .unwrap()
            .lesson
            .in_next_brief
    );
    w.core.crew_lessons_set_use(&project, true).unwrap();
    assert!(
        w.core
            .crew_lesson_detail(&global.id, Some(&project))
            .unwrap()
            .lesson
            .in_next_brief
    );

    let missing = w
        .core
        .crew_lesson_detail(&global.id, Some("no-such-repository"))
        .unwrap_err();
    assert_eq!(
        (missing.status(), missing.code(), missing.to_string()),
        (404, "not_found", "project not found".to_owned())
    );
}

#[tokio::test]
async fn editing_a_lesson_cleans_marks_and_rescans_it() {
    let w = quiet().await;
    let project = project_of(&w);
    let fingerprint = "f".repeat(64);
    let seeded = seed(
        &w,
        "repo",
        Some(&project),
        "fingerprint",
        Some(&fingerprint),
        "Checks failed before.",
    );
    assert!(!seeded.edited_by_person && !seeded.names_path);

    let edited = w
        .core
        .crew_lesson_edit(&seeded.id, "Update parser.c before the build.", 1)
        .unwrap();
    assert_eq!(edited.text, "Update parser.c before the build.");
    assert!(edited.edited_by_person);
    assert!(edited.names_path);
    assert_eq!((edited.rev, edited.source.as_str()), (2, "fingerprint"));
    assert_eq!(edited.fingerprint.as_deref(), Some(fingerprint.as_str()));

    let secret = format!("The token is ghp_{}", "q".repeat(36));
    let refused = w.core.crew_lesson_edit(&seeded.id, &secret, 2).unwrap_err();
    match refused {
        CoreError::Refused {
            status,
            code,
            message,
            ..
        } => assert_eq!(
            (status, code, message.as_str()),
            (
                400,
                "secret_refused",
                "Not saved: this looks like it holds a secret. Edit it and save again."
            )
        ),
        other => panic!("{other:?}"),
    }
    let unchanged = lesson_row(&w, &seeded.id);
    assert_eq!(
        (unchanged.text.as_str(), unchanged.rev),
        ("Update parser.c before the build.", 2)
    );

    let global = add_global(&w, "Use pnpm.");
    let refused = w
        .core
        .crew_lesson_edit(&global.id, "Update parser.c.", 1)
        .unwrap_err();
    match refused {
        CoreError::Invalid(message) => {
            assert_eq!(message, "A global lesson names path classes, not paths.")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(lesson_row(&w, &global.id).text, "Use pnpm.");

    let cleared = w
        .core
        .crew_lesson_edit(&seeded.id, "Prefer small commits.", 2)
        .unwrap();
    assert!(!cleared.names_path);
    assert_eq!(cleared.rev, 3);
}

#[tokio::test]
async fn a_stale_rev_is_refused_on_every_write_and_changes_nothing() {
    let w = quiet().await;
    let project = project_of(&w);
    let repo = add_repo(&w, &project, "Use pnpm.");
    let global = add_global(&w, "Global lesson.");

    assert_stale(
        w.core
            .crew_lesson_edit(&repo.id, "Changed.", 0)
            .unwrap_err(),
    );
    assert_stale(
        w.core
            .crew_lesson_set_enabled(&repo.id, false, 0)
            .unwrap_err(),
    );
    assert_stale(
        w.core
            .crew_lesson_set_enabled(&repo.id, true, 0)
            .unwrap_err(),
    );
    assert_stale(w.core.crew_lesson_promote(&repo.id, 0).unwrap_err());
    assert_stale(w.core.crew_lesson_delete(&repo.id, 0).unwrap_err());
    assert_stale(
        w.core
            .crew_lesson_demote(&global.id, &project, 0)
            .unwrap_err(),
    );

    assert_eq!(lesson_row(&w, &repo.id), repo);
    assert_eq!(lesson_row(&w, &global.id), global);
    assert_eq!(
        actions(&w),
        ["crew.lesson.learn", "crew.lesson.learn"],
        "a refused write writes no audit row"
    );
}

#[tokio::test]
async fn promoting_and_demoting_move_the_scope_and_respect_the_global_cap() {
    let w = quiet().await;
    let project = project_of(&w);
    let path = add_repo(&w, &project, "Update parser.c.");
    let refused = w.core.crew_lesson_promote(&path.id, path.rev).unwrap_err();
    match refused {
        CoreError::Invalid(message) => assert_eq!(
            message,
            "Edit this lesson first: it names a path, and a global lesson names path classes only."
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        lesson_row(&w, &path.id),
        path,
        "a refused promote changes nothing"
    );

    for n in 0..20 {
        seed(
            &w,
            "global",
            None,
            "person",
            None,
            &format!("Global lesson {n}."),
        );
    }
    let repo = add_repo(&w, &project, "Use pnpm.");
    let promoted = w.core.crew_lesson_promote(&repo.id, repo.rev).unwrap();
    assert_eq!(
        (
            promoted.scope.as_str(),
            promoted.project_id.as_deref(),
            promoted.rev
        ),
        ("global", None, 2)
    );
    let globals = w.core.store.crew_lessons("global", None).unwrap();
    assert_eq!(globals.len(), 21, "the over-cap lesson stays readable");
    let hidden: Vec<&CrewLesson> = globals
        .iter()
        .filter(|lesson| lesson.hidden_by_cap)
        .collect();
    assert_eq!(hidden.len(), 1);
    assert_ne!(
        hidden[0].id, promoted.id,
        "the promoted lesson is not hidden"
    );
    assert_eq!(hidden[0].rev, 2, "a cap hide bumps the rev");

    let demoted = w
        .core
        .crew_lesson_demote(&promoted.id, &project, promoted.rev)
        .unwrap();
    assert_eq!(
        (
            demoted.scope.as_str(),
            demoted.project_id.as_deref(),
            demoted.rev
        ),
        ("repo", Some(project.as_str()), 3)
    );
}

#[tokio::test]
async fn a_deleted_lesson_is_restored_by_undo_with_a_new_rev() {
    let w = quiet().await;
    let project = project_of(&w);
    let lesson = add_repo(&w, &project, "Use pnpm.");
    let (id, undo_until) = w.core.crew_lesson_delete(&lesson.id, lesson.rev).unwrap();
    assert_eq!(id, lesson.id);
    let until = parse_time(&undo_until).expect("an RFC 3339 undo time");
    assert!(
        until > unix_now() && until <= unix_now() + 12,
        "the undo window is about ten seconds: {undo_until}"
    );

    assert!(w.core.store.crew_lesson(&id).unwrap().is_none());
    assert!(
        w.core
            .crew_lessons_list(&project, LessonFilter::All)
            .unwrap()
            .lessons
            .is_empty()
    );
    let deleted = w
        .core
        .store
        .crew_lesson_including_deleted(&id)
        .unwrap()
        .unwrap();
    assert_eq!(deleted.rev, lesson.rev);
    assert!(deleted.deleted_at.is_some());
    let missing = w.core.crew_lesson_detail(&id, None).unwrap_err();
    assert_eq!(
        (missing.status(), missing.code(), missing.to_string()),
        (404, "not_found", "lesson not found".to_owned())
    );

    let restored = w.core.crew_lesson_undo(&id).unwrap();
    assert_eq!(restored.rev, lesson.rev + 1);
    assert_eq!(restored.deleted_at, None);
    assert_eq!(lesson_row(&w, &id), restored);
    assert_eq!(
        w.core
            .crew_lessons_list(&project, LessonFilter::All)
            .unwrap()
            .lessons
            .len(),
        1
    );
    let again = w.core.crew_lesson_undo(&id).unwrap_err();
    assert_eq!((again.status(), again.code()), (404, "not_found"));
    assert_eq!(
        actions(&w),
        [
            "crew.lesson.learn",
            "crew.lesson.delete",
            "crew.lesson.undo"
        ]
    );
}

#[tokio::test]
async fn purging_after_the_window_remembers_the_fingerprint_for_storage() {
    let w = quiet().await;
    let project = project_of(&w);
    let fingerprint = "a".repeat(64);
    let seeded = seed(
        &w,
        "repo",
        Some(&project),
        "fingerprint",
        Some(&fingerprint),
        "Checks failed before.",
    );
    w.core.crew_lesson_delete(&seeded.id, seeded.rev).unwrap();
    let purged = w
        .core
        .store
        .purge_deleted_lessons(&format_unix(unix_now() + 60))
        .unwrap();
    assert_eq!(purged.as_slice(), std::slice::from_ref(&seeded.id));
    assert!(
        w.core
            .store
            .crew_lesson_including_deleted(&seeded.id)
            .unwrap()
            .is_none()
    );
    let forgotten: String = w
        .core
        .store
        .reader()
        .query_row(
            "SELECT fingerprint FROM crew_lessons_forgotten",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(forgotten, fingerprint);

    let rewritten = w
        .core
        .store
        .write_crew_lesson(NewCrewLesson {
            scope: "repo".to_owned(),
            project_id: Some(project.clone()),
            source: "fingerprint".to_owned(),
            text: "The same failure again.".to_owned(),
            fingerprint: Some(fingerprint.clone()),
            evidence: Vec::new(),
            edited_by_person: false,
            names_path: false,
        })
        .unwrap();
    assert_eq!(
        rewritten, None,
        "a forgotten fingerprint is never rewritten"
    );
    assert!(
        w.core
            .store
            .crew_lessons("repo", Some(&project))
            .unwrap()
            .is_empty()
    );
    assert!(actions(&w).contains(&"crew.lesson.purge".to_owned()));
}

#[tokio::test]
async fn every_person_write_audits_its_action_in_its_own_transaction() {
    let w = quiet().await;
    let project = project_of(&w);
    let lesson = add_repo(&w, &project, "Use pnpm.");
    let edited = w
        .core
        .crew_lesson_edit(&lesson.id, "Use pnpm, not npm.", lesson.rev)
        .unwrap();
    let disabled = w
        .core
        .crew_lesson_set_enabled(&lesson.id, false, edited.rev)
        .unwrap();
    let enabled = w
        .core
        .crew_lesson_set_enabled(&lesson.id, true, disabled.rev)
        .unwrap();
    let promoted = w.core.crew_lesson_promote(&lesson.id, enabled.rev).unwrap();
    let demoted = w
        .core
        .crew_lesson_demote(&lesson.id, &project, promoted.rev)
        .unwrap();
    w.core.crew_lesson_delete(&lesson.id, demoted.rev).unwrap();
    let restored = w.core.crew_lesson_undo(&lesson.id).unwrap();
    w.core.crew_lesson_delete(&lesson.id, restored.rev).unwrap();
    w.core
        .store
        .purge_deleted_lessons(&format_unix(unix_now() + 60))
        .unwrap();

    assert_eq!(
        actions(&w),
        [
            "crew.lesson.learn",
            "crew.lesson.edit",
            "crew.lesson.disable",
            "crew.lesson.enable",
            "crew.lesson.promote",
            "crew.lesson.demote",
            "crew.lesson.delete",
            "crew.lesson.undo",
            "crew.lesson.delete",
            "crew.lesson.purge"
        ]
    );
    let conn = w.core.store.reader();
    let mut statement = conn
        .prepare(
            "SELECT target FROM audit_events
             WHERE action LIKE 'crew.lesson.%' ORDER BY id",
        )
        .unwrap();
    let targets: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(
        targets.iter().all(|target| target == &lesson.id),
        "every row names the lesson: {targets:?}"
    );
}

#[tokio::test]
async fn counting_a_use_does_not_bump_the_rev() {
    let w = quiet().await;
    let project = project_of(&w);
    let lesson = add_repo(&w, &project, "Use pnpm.");
    w.core
        .store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            std::slice::from_ref(&lesson.id),
            &[],
        )
        .unwrap();
    let used = lesson_row(&w, &lesson.id);
    assert_eq!(used.uses, 1);
    assert_eq!(used.rev, lesson.rev, "a use is bookkeeping, not a change");
    assert!(used.last_used_at.is_some());
}

#[tokio::test]
async fn preview_off_refuses_every_lesson_person_method() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let project = project_of(&w);
    let id = "les_missing";

    assert_preview_off(w.core.crew_lessons_list(&project, LessonFilter::All));
    assert_preview_off(w.core.crew_lesson_detail(id, None));
    assert_preview_off(
        w.core
            .crew_lesson_add(LessonScope::Repo(project.clone()), "A lesson."),
    );
    assert_preview_off(w.core.crew_lesson_edit(id, "Edited.", 1));
    assert_preview_off(w.core.crew_lesson_set_enabled(id, false, 1));
    assert_preview_off(w.core.crew_lesson_promote(id, 1));
    assert_preview_off(w.core.crew_lesson_demote(id, &project, 1));
    assert_preview_off(w.core.crew_lesson_delete(id, 1));
    assert_preview_off(w.core.crew_lesson_undo(id));
    assert_preview_off(w.core.crew_lessons_set_use(&project, false));
    assert_preview_off(w.core.crew_lesson_from_person_correction(
        LessonScope::Repo(project.clone()),
        "A lesson.",
        Vec::new(),
    ));
    assert!(
        w.core
            .store
            .crew_lessons("repo", Some(&project))
            .unwrap()
            .is_empty(),
        "nothing was written"
    );
}

#[tokio::test]
async fn a_person_add_keeps_the_secret_and_path_rules() {
    let w = quiet().await;
    let project = project_of(&w);
    let secret = format!("The token is ghp_{}", "q".repeat(36));
    let refused = w
        .core
        .crew_lesson_add(LessonScope::Repo(project.clone()), &secret)
        .unwrap_err();
    match refused {
        CoreError::Refused {
            status,
            code,
            message,
            ..
        } => assert_eq!(
            (status, code, message.as_str()),
            (
                400,
                "secret_refused",
                "Not saved: this looks like it holds a secret. Edit it and save again."
            )
        ),
        other => panic!("{other:?}"),
    }
    let global = w
        .core
        .crew_lesson_add(LessonScope::Global, "Update parser.c.")
        .unwrap_err();
    match global {
        CoreError::Invalid(message) => {
            assert_eq!(message, "A global lesson names path classes, not paths.")
        }
        other => panic!("{other:?}"),
    }
    assert!(
        w.core
            .store
            .crew_lessons("repo", Some(&project))
            .unwrap()
            .is_empty()
    );
    assert!(lesson_row_opt(&w, "les_missing").is_none());
    let added = add_repo(&w, &project, "Use pnpm.");
    assert_eq!(added.source, "person");
    assert_eq!(added.rev, 1);
    assert_eq!(
        added.evidence,
        Vec::<LessonEvidence>::new(),
        "adding by hand carries no evidence"
    );
}
