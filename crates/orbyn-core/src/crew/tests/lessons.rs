//! Crew lessons (KC5 slice 1) on a real `Core`: the one write path and the three sources, on the harness's fake
//! engines and the escape and merge rigs. Nothing here routes or verifies differently: a lesson is data only.

use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::json;

use super::autopilot_areas::{DAY, rig};
use super::autopilot_merge::{merging, passed};
use super::autopilot_watch::NOW;
use super::harness::{Turn, World, brief, key, report, world};
use crate::crew::lessons::{LessonScope, TEXT_MAX};
use crate::crew::{Answering, Mode, Page};
use crate::error::CoreError;
use crate::store::{CrewLesson, LessonEvidence};

fn page() -> Page {
    Page::new(Some(200), None).unwrap()
}

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

fn lessons_of(w: &World, project: &str) -> Vec<CrewLesson> {
    w.core.store.crew_lessons("repo", Some(project)).unwrap()
}

fn evidence(kind: &str, id: &str) -> LessonEvidence {
    LessonEvidence {
        kind: kind.to_owned(),
        id: id.to_owned(),
    }
}

/// The lesson of `source` a read returned, found by the source it records rather than by the place the read put it:
/// the store's ordering is not a fixture a test can rely on.
fn sourced<'a>(lessons: &'a [CrewLesson], source: &str) -> &'a CrewLesson {
    lessons
        .iter()
        .find(|lesson| lesson.source == source)
        .unwrap_or_else(|| panic!("no {source} lesson: {lessons:?}"))
}

/// The lesson evidence of `kind`, found by the kind it records: the write path's order is not a fixture either.
fn evidence_of<'a>(lesson: &'a CrewLesson, kind: &str) -> &'a LessonEvidence {
    lesson
        .evidence
        .iter()
        .find(|recorded| recorded.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} evidence: {:?}", lesson.evidence))
}

fn decide(project: &str) -> wire::EscapeDecide {
    wire::EscapeDecide {
        project: project.to_owned(),
        seen: wire::EscapeSeen {
            state: wire::EscapeState::Possible,
            gap: None,
        },
        note: None,
    }
}

fn bad(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/bad.txt"), "outside\n").unwrap();
}

/// A start whose brief is its own, so two tasks of one repository are two tasks, not one.
async fn started_doing(w: &World, outcome: &str) -> String {
    let (answer, error) = w.start(brief(json!({ "outcome": outcome })), key()).await;
    assert!(!error, "{answer}");
    answer["task"].as_str().unwrap().to_owned()
}

/// The task's leases are released with its end; a following start of the same files waits for that.
async fn until_released(w: &World, task: &str) {
    let crew = w.task(task).crew_id.clone();
    w.until("the task's leases to be released", |w| {
        w.core
            .live_crew_leases(&crew, crate::util::unix_now())
            .unwrap()
            .is_empty()
    })
    .await;
}

/// A core with a preview on and no run, for the person's own write path.
async fn person_world() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

#[tokio::test]
async fn two_tasks_of_one_repository_with_the_same_failure_write_one_lesson() {
    // Every turn leaves the same file outside the lease, so every task fails the same check the same way.
    let w = world(
        true,
        Arc::new(|_, root| {
            bad(root);
            Turn::Report(report("done"))
        }),
    )
    .await;
    let project = project_of(&w);
    let first = started_doing(&w, "First task.").await;
    w.until_state(&first, "stopped").await;
    until_released(&w, &first).await;
    assert!(
        lessons_of(&w, &project).is_empty(),
        "one task with a failure is not a repeated failure"
    );

    // The second task's first failure is the same fingerprint on a second task of the repository.
    let second = started_doing(&w, "Second task.").await;
    w.until("the repeated-failure lesson", |w| {
        !lessons_of(w, &project).is_empty()
    })
    .await;
    let lessons = lessons_of(&w, &project);
    assert_eq!(lessons.len(), 1);
    let lesson = sourced(&lessons, "fingerprint");
    assert_eq!(lesson.scope.as_str(), "repo");
    assert_eq!(lesson.project_id.as_deref(), Some(project.as_str()));
    let ids: Vec<&str> = lesson
        .evidence
        .iter()
        .map(|evidence| evidence.id.as_str())
        .collect();
    assert!(
        ids.contains(&first.as_str()) && ids.contains(&second.as_str()),
        "{ids:?}"
    );
    // The sentence names the check's KIND, never its id: H1 keeps every part of the key out of a lesson.
    assert!(
        lesson
            .text
            .starts_with("Checks like `file_scope` failed on files under `docs` before:"),
        "{}",
        lesson.text
    );
    assert!(
        !lesson.text.contains("chk-"),
        "a check id never enters a lesson: {}",
        lesson.text
    );
    assert!(lesson.text.chars().count() <= TEXT_MAX, "{}", lesson.text);
    assert!(lesson.names_path, "{}", lesson.text);
    assert_eq!(
        (lesson.check_kind.as_deref(), lesson.path_class.as_deref()),
        (Some("file_scope"), Some("docs")),
        "the group key comes from the failing kind and path area"
    );
    assert_eq!(
        (lesson.uses, lesson.hidden_by_cap),
        (0, false),
        "a learned lesson starts unused and visible"
    );

    // A third task with the same failure rewrites nothing.
    w.until_state(&second, "stopped").await;
    until_released(&w, &second).await;
    let third = started_doing(&w, "Third task.").await;
    w.until_state(&third, "stopped").await;
    until_released(&w, &third).await;
    assert_eq!(lessons_of(&w, &project).len(), 1);
}

#[tokio::test]
async fn a_possible_escape_writes_nothing_and_a_confirmed_one_writes_one_lesson() {
    let mut rig = rig().await;
    let task = rig.merge(&["src/a"], NOW - 3 * DAY).await;
    rig.revert(&task, NOW);
    let log = rig
        .w
        .core
        .crew_escape_log(&rig.project, None, page())
        .unwrap();
    let escape = log
        .rows
        .iter()
        .find(|row| row.merge_task_id == task)
        .expect("the reverted merge's escape row")
        .clone();
    assert_eq!(escape.state, wire::EscapeState::Possible);
    assert!(
        lessons_of(&rig.w, &rig.project).is_empty(),
        "a possible escape is not learned"
    );

    rig.w
        .core
        .crew_escape_confirm(&escape.id, &decide(&rig.project))
        .unwrap();
    let lessons = lessons_of(&rig.w, &rig.project);
    assert_eq!(lessons.len(), 1);
    let lesson = sourced(&lessons, "revert");
    assert_eq!(
        lesson.text, "A change in `src/a` was reverted.",
        "{}",
        lesson.text
    );
    assert!(lesson.names_path);
    assert_eq!(
        (lesson.check_kind.as_deref(), lesson.path_class.as_deref()),
        (None, Some("src/a")),
        "an escape lesson groups by its first area"
    );
    assert_eq!(
        lesson.evidence,
        [evidence("escape", &escape.id), evidence("merge", &task)]
    );

    // The same confirm again is one lesson, still.
    rig.w
        .core
        .crew_escape_confirm(&escape.id, &decide(&rig.project))
        .unwrap();
    assert_eq!(lessons_of(&rig.w, &rig.project).len(), 1);

    // A person's mark is a confirm: one escape lesson, from the person's recorded reason.
    let marked = rig.merge(&["src/b"], NOW - 2 * DAY).await;
    let marked_escape = rig
        .w
        .core
        .crew_escape_mark(&wire::EscapeMark {
            project: rig.project.clone(),
            merge_task_id: marked.clone(),
            seen: None,
            note: Some("It broke the login page.".to_owned()),
        })
        .unwrap();
    let lessons = lessons_of(&rig.w, &rig.project);
    assert_eq!(lessons.len(), 2);
    let escape_lesson = sourced(&lessons, "escape");
    assert_eq!(
        escape_lesson.text,
        "A change in `src/b` escaped: It broke the login page."
    );
    assert_eq!(
        escape_lesson.evidence,
        [
            evidence("escape", &marked_escape.id),
            evidence("merge", &marked)
        ]
    );
}

#[tokio::test]
async fn a_person_who_reverts_a_merge_leaves_one_revert_lesson() {
    let m = merging(Mode::Full, |_| vec![("docs/note.md", "A note.\n")]).await;
    let id = passed(&m.w, &["docs/**"], "Add a note.").await;
    let _entry =
        m.w.core
            .crew_queue_merge(&id, &m.target, NOW)
            .await
            .unwrap();
    let outcomes = m.process(NOW).await;
    assert!(
        matches!(
            outcomes.as_slice(),
            [crate::crew::MergeOutcome::Merged { .. }]
        ),
        "{outcomes:?}"
    );
    let record = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    let project = project_of(&m.w);
    m.w.core
        .autopilot_answer(
            &id,
            &wire::AutopilotAnswerBody {
                merge_commit: record.merge_commit.clone(),
                text: None,
            },
            Answering::Revert,
            NOW + 10,
        )
        .await
        .unwrap();
    let lessons = lessons_of(&m.w, &project);
    assert_eq!(lessons.len(), 1);
    let lesson = sourced(&lessons, "revert");
    assert_eq!(
        lesson.text, "A change in `docs/note.md` was reverted.",
        "{}",
        lesson.text
    );
    assert_eq!(evidence_of(lesson, "merge"), &evidence("merge", &id));

    // A later confirm of the escape it logged does not write a second lesson for one merge.
    let escape = m.w.core.store.crew_escape_of_merge(&id).unwrap().unwrap();
    m.w.core
        .crew_escape_confirm(&escape.id, &decide(&project))
        .unwrap();
    assert_eq!(lessons_of(&m.w, &project).len(), 1);
}

#[tokio::test]
async fn a_persons_correction_stores_only_their_words_as_a_persons_lesson() {
    let w = person_world().await;
    let project = project_of(&w);
    let lesson = w
        .core
        .crew_lesson_from_person_correction(
            LessonScope::Repo(project.clone()),
            "Use pnpm, not npm; the lockfile is pnpm-lock.yaml.",
            vec![evidence("task", "sha256:one")],
        )
        .unwrap();
    assert_eq!(
        (lesson.source.as_str(), lesson.scope.as_str()),
        ("person", "repo")
    );
    assert_eq!(lesson.project_id.as_deref(), Some(project.as_str()));
    assert_eq!(
        lesson.text,
        "Use pnpm, not npm; the lockfile is pnpm-lock.yaml."
    );
    assert!(lesson.edited_by_person);
    assert!(lesson.names_path, "the text names a repository file");
    assert_eq!(lesson.evidence, [evidence("task", "sha256:one")]);
    assert_eq!(lessons_of(&w, &project), vec![lesson]);

    // A global lesson names path classes, not paths.
    let refused = w
        .core
        .crew_lesson_from_person_correction(
            LessonScope::Global,
            "Edit pnpm-lock.yaml before the install.",
            Vec::new(),
        )
        .unwrap_err();
    assert!(refused.to_string().contains("path"), "{refused}");
    let global = w
        .core
        .crew_lesson_from_person_correction(LessonScope::Global, "Use pnpm, not npm.", Vec::new())
        .unwrap();
    assert_eq!(
        (global.scope.as_str(), global.project_id.as_deref()),
        ("global", None)
    );
    assert!(!global.names_path);
    assert_eq!(w.core.store.crew_lessons("global", None).unwrap(), [global]);
}

#[tokio::test]
async fn the_person_write_path_refuses_empty_long_and_secret_words() {
    let w = person_world().await;
    let project = project_of(&w);
    let repo = || LessonScope::Repo(project.clone());
    assert!(matches!(
        w.core
            .crew_lesson_from_person_correction(repo(), "   ", Vec::new()),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        w.core
            .crew_lesson_from_person_correction(repo(), &"x".repeat(TEXT_MAX + 1), Vec::new()),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        w.core
            .crew_lesson_from_person_correction(repo(), "A note\u{7}here.", Vec::new()),
        Err(CoreError::Invalid(_))
    ));

    let secret = format!("The token is ghp_{}", "q".repeat(36));
    let refused = w
        .core
        .crew_lesson_from_person_correction(repo(), &secret, Vec::new())
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
                // Word for word from the wire contract (docs/crew-lessons.md, "Errors").
                "Not saved: this looks like it holds a secret. Edit it and save again."
            )
        ),
        other => panic!("{other}"),
    }
    assert!(
        lessons_of(&w, &project).is_empty(),
        "a refused lesson is not stored"
    );
}

#[tokio::test]
async fn a_global_lesson_refuses_every_way_of_naming_a_path() {
    let w = person_world().await;
    for words in [
        "Update parser.c before the build.",
        "Keep lessons.h in sync.",
        "Check /etc/hosts first.",
        "Look in ../secrets/notes first.",
    ] {
        let refused = w
            .core
            .crew_lesson_from_person_correction(LessonScope::Global, words, Vec::new())
            .unwrap_err();
        assert!(refused.to_string().contains("path"), "{words}: {refused}");
    }
    // An abbreviation is not a file name, and a path class stays allowed.
    for words in [
        "Prefer small commits, e.g. one per fix.",
        "Mind the test folders, i.e. all of them.",
    ] {
        let lesson = w
            .core
            .crew_lesson_from_person_correction(LessonScope::Global, words, Vec::new())
            .unwrap();
        assert!(!lesson.names_path, "{words}");
    }
}

#[tokio::test]
async fn a_lesson_keeps_at_most_five_evidence_ids_on_every_write_path() {
    let w = person_world().await;
    let project = project_of(&w);
    let six: Vec<LessonEvidence> = (0..6)
        .map(|n| evidence("task", &format!("sha256:{n}")))
        .collect();
    let lesson = w
        .core
        .crew_lesson_from_person_correction(LessonScope::Repo(project.clone()), "Use pnpm.", six)
        .unwrap();
    assert_eq!(lesson.evidence.len(), 5, "{:?}", lesson.evidence);
    // The cap keeps the first evidence ids the caller passed, so the first place here is the truncation contract, not
    // a fixture read.
    assert_eq!(lesson.evidence[0], evidence("task", "sha256:0"));
}
