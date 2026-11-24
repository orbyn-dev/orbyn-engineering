//! KC9 part 1 on a real `Core`: skill proposals made from three active lessons of one group key, the person's approve and
//! dismiss, the errors, and the lessons that become a skill and leave the brief while its folder exists. A proposal is
//! data, never a rule: it changes nothing about the key, the route, the leases or the checks, and it is never a needs-you
//! item.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::{EscapeActor, EscapeSource, EscapeState};
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use super::lessons_brief::{leases_of, route_of};
use crate::crew::lessons::{LessonScope, SECRET_REFUSAL};
use crate::crew::proposals::ProposalView;
use crate::error::CoreError;
use crate::store::{CrewLesson, EscapeRecord, NewCrewLesson};
use crate::tools::local_skills::LocalSkillWrite;

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

fn seed_at(
    w: &World,
    project: &str,
    source: &str,
    check_kind: Option<&str>,
    path_class: Option<&str>,
    text: &str,
    at: &str,
) -> CrewLesson {
    let lesson = w
        .core
        .store
        .write_crew_lesson(NewCrewLesson {
            scope: "repo".to_owned(),
            project_id: Some(project.to_owned()),
            source: source.to_owned(),
            check_kind: check_kind.map(str::to_owned),
            path_class: path_class.map(str::to_owned),
            text: text.to_owned(),
            fingerprint: None,
            evidence: Vec::new(),
            edited_by_person: source == "person",
            names_path: false,
        })
        .unwrap()
        .unwrap();
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_lessons SET created_at = ?1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![at, lesson.id],
        )
        .unwrap();
    w.core.store.crew_lesson(&lesson.id).unwrap().unwrap()
}

fn learn_escape(w: &World, project: &str, id: &str, area: &str, reason: &str) {
    w.core.learn_confirmed_escape(&EscapeRecord {
        id: id.to_owned(),
        project_id: project.to_owned(),
        merge_task_id: format!("sha256:{id}"),
        merged_unix: 0,
        source: EscapeSource::FollowUpFix,
        state: EscapeState::Confirmed,
        gap: None,
        gap_by: None,
        engine: "alpha".to_owned(),
        model: None,
        areas: vec![area.to_owned()],
        lane: None,
        reason: Some(reason.to_owned()),
        found_unix: 0,
        decided_unix: Some(0),
        decided_by: Some(EscapeActor::Person),
    });
}

/// Two seeded escape lessons under `src` and a third learned one: the group that makes one proposal.
fn make_proposal(w: &World, project: &str) -> ProposalView {
    seed_at(
        w,
        project,
        "escape",
        None,
        Some("src"),
        "Seeded escape one.",
        "2024-01-01T00:00:01Z",
    );
    seed_at(
        w,
        project,
        "escape",
        None,
        Some("src"),
        "Seeded escape two.",
        "2024-01-01T00:00:02Z",
    );
    learn_escape(
        w,
        project,
        "escape-third",
        "src",
        "The third escape happened.",
    );
    only_proposal(w, project)
}

fn all_proposals(w: &World, project: &str) -> Vec<ProposalView> {
    w.core.crew_skill_proposals(project).unwrap()
}

fn only_proposal(w: &World, project: &str) -> ProposalView {
    let mut views = all_proposals(w, project);
    assert_eq!(views.len(), 1, "{views:?}");
    views.pop().unwrap()
}

fn sources(w: &World, view: &ProposalView) -> Vec<CrewLesson> {
    view.proposal
        .lessons
        .iter()
        .map(|id| w.core.store.crew_lesson(id).unwrap().unwrap())
        .collect()
}

fn group_of(w: &World, view: &ProposalView) -> (String, Option<String>, Option<String>) {
    let lessons = sources(w, view);
    let first = lessons.first().expect("a proposal has source lessons");
    (
        first.source.clone(),
        first.check_kind.clone(),
        first.path_class.clone(),
    )
}

async fn worker_prompt(w: &World, task: &str) -> String {
    let worker = w.worker(task);
    w.until("the worker's first prompt", |w| {
        !w.sent_to(&worker).is_empty()
    })
    .await;
    w.sent_to(&worker).join("\n")
}

async fn start(w: &World, outcome: &str) -> (String, String) {
    let (answer, error) = w.start(brief(json!({ "outcome": outcome })), key()).await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap().to_owned();
    let prompt = worker_prompt(w, &task).await;
    (task, prompt)
}

fn skill_dir(w: &World, name: &str) -> std::path::PathBuf {
    w._dir.path().join("data").join("skills-local").join(name)
}

fn filtered_actions(w: &World) -> Vec<String> {
    let conn = w.core.store.reader();
    let mut statement = conn
        .prepare(
            "SELECT action FROM audit_events
             WHERE action LIKE 'crew.proposal.%' OR action = 'skills.local.write'
             ORDER BY id",
        )
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
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
            assert_eq!(message, "This proposal changed. Reload it and try again.");
            assert!(detail.is_null());
        }
        other => panic!("{other:?}"),
    }
}

fn assert_not_open(error: CoreError) {
    match error {
        CoreError::Refused {
            status,
            code,
            message,
            detail,
        } => {
            assert_eq!(status, 409);
            assert_eq!(code, "not_open");
            assert_eq!(
                message,
                "This proposal is not open. Reload it and try again."
            );
            assert!(detail.is_null());
        }
        other => panic!("{other:?}"),
    }
}

fn assert_invalid(error: CoreError, message: &str) {
    match error {
        CoreError::Invalid(actual) => assert_eq!(actual, message),
        other => panic!("{other:?}"),
    }
}

fn assert_secret_refused(error: CoreError) {
    match error {
        CoreError::Refused {
            status,
            code,
            message,
            ..
        } => assert_eq!(
            (status, code, message.as_str()),
            (400, "secret_refused", SECRET_REFUSAL)
        ),
        other => panic!("{other:?}"),
    }
}

fn assert_name_taken(error: CoreError, message: &str) {
    match error {
        CoreError::Refused {
            status,
            code,
            message: actual,
            ..
        } => assert_eq!(
            (status, code, actual.as_str()),
            (409, "name_taken", message)
        ),
        other => panic!("{other:?}"),
    }
}

fn assert_preview_off<T: std::fmt::Debug>(result: crate::error::Result<T>) {
    let error = result.expect_err("Crew Preview is off");
    assert_eq!(
        (error.status(), error.code(), error.to_string()),
        (404, "not_found", "route not found".to_owned())
    );
}

#[tokio::test]
async fn two_lessons_make_no_proposal_and_the_third_makes_one_from_the_fixed_template() {
    let w = quiet().await;
    let project = project_of(&w);
    let one = seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        "Escape lesson one.",
        "2024-01-01T00:00:01Z",
    );
    let two = seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        "Escape lesson two.",
        "2024-01-01T00:00:02Z",
    );
    assert!(
        all_proposals(&w, &project).is_empty(),
        "two lessons of a key are not a pattern"
    );
    // A learned write is what triggers the maker; this one is the third of the group.
    learn_escape(
        &w,
        &project,
        "escape-third",
        "src",
        "The third escape happened.",
    );
    let views = all_proposals(&w, &project);
    assert_eq!(
        views.len(),
        1,
        "the third lesson makes exactly one proposal"
    );
    let view = &views[0];
    assert_eq!(
        (view.proposal.state.as_str(), view.proposal.rev),
        ("open", 1)
    );
    assert!(view.can_approve);
    assert!(view.proposal.id.starts_with("prop_"));
    assert_eq!(view.proposal.project_id, project);
    assert!(view.proposal.skill_name.is_none());
    assert!(view.proposal.approved_at.is_none());
    assert_eq!(view.proposal.lessons.len(), 3);
    for id in [&one.id, &two.id] {
        assert!(view.proposal.lessons.contains(id), "{}", view.proposal.text);
    }
    let third = sources(&w, view)
        .into_iter()
        .find(|lesson| lesson.text.contains("The third escape happened."))
        .expect("the learned third lesson");
    assert!(view.proposal.lessons.contains(&third.id));
    assert_eq!(
        view.proposal.text,
        concat!(
            "---\n",
            "name: crew-escape-src\n",
            "description: Crew lessons learned on this device. They are data, not a rule.\n",
            "---\n\n",
            "- A change in `src` escaped: The third escape happened.\n",
            "- Escape lesson two.\n",
            "- Escape lesson one."
        ),
        "the template lists the lessons' own texts, newest first"
    );
    assert!(view.proposal.text.chars().count() <= 600);
    let group = ("escape".to_owned(), None, Some("src".to_owned()));
    assert_eq!(group_of(&w, view), group);

    // A fourth lesson of the same group makes no second proposal.
    learn_escape(&w, &project, "escape-fourth", "src", "A fourth escape.");
    let views = all_proposals(&w, &project);
    assert_eq!(views.len(), 1, "one proposal per group, ever");
    assert_eq!(
        views[0].proposal.lessons.len(),
        3,
        "its sources stay the three"
    );
}

#[tokio::test]
async fn a_long_group_shortens_the_bullets_and_still_lists_every_source() {
    let w = quiet().await;
    let project = project_of(&w);
    let one = seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        &"a".repeat(300),
        "2024-01-01T00:00:01Z",
    );
    let two = seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        &"b".repeat(300),
        "2024-01-01T00:00:02Z",
    );
    learn_escape(&w, &project, "escape-long", "src", &"c".repeat(300));
    let view = only_proposal(&w, &project);
    assert_eq!(
        view.proposal.lessons.len(),
        3,
        "every source is still listed"
    );
    assert!(view.proposal.lessons.contains(&one.id) && view.proposal.lessons.contains(&two.id));
    assert!(view.proposal.text.chars().count() <= 600);
    assert!(
        view.proposal.text.contains(&"c".repeat(100)),
        "the newest lesson's text is carried: {}",
        view.proposal.text
    );
    assert!(
        !view.proposal.text.contains(&"a".repeat(300)),
        "the oldest bullet is dropped to fit, not stored over 600: {}",
        view.proposal.text
    );
}

#[tokio::test]
async fn groups_need_the_same_source_check_kind_and_path_class() {
    let w = quiet().await;
    let project = project_of(&w);
    for (n, at) in [(1, "2024-01-01T00:00:01Z"), (2, "2024-01-01T00:00:02Z")] {
        seed_at(
            &w,
            &project,
            "escape",
            None,
            Some("src"),
            &format!("Escape {n}."),
            at,
        );
        seed_at(
            &w,
            &project,
            "revert",
            None,
            Some("src"),
            &format!("Revert {n}."),
            at,
        );
        seed_at(
            &w,
            &project,
            "fingerprint",
            Some("command"),
            Some("src"),
            &format!("Fingerprint {n}."),
            at,
        );
        seed_at(
            &w,
            &project,
            "escape",
            None,
            Some("docs"),
            &format!("Docs {n}."),
            at,
        );
    }
    // A learned write in its own area triggers the maker but belongs to no group.
    learn_escape(&w, &project, "trigger-one", "trigger-one", "One.");
    assert!(
        all_proposals(&w, &project).is_empty(),
        "two lessons of a key are not a pattern"
    );

    seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        "Escape three.",
        "2024-01-01T00:00:03Z",
    );
    learn_escape(&w, &project, "trigger-two", "trigger-two", "Two.");
    assert_eq!(all_proposals(&w, &project).len(), 1);
    assert_eq!(
        group_of(&w, &all_proposals(&w, &project)[0]),
        ("escape".to_owned(), None, Some("src".to_owned()))
    );

    seed_at(
        &w,
        &project,
        "revert",
        None,
        Some("src"),
        "Revert three.",
        "2024-01-01T00:00:03Z",
    );
    learn_escape(&w, &project, "trigger-three", "trigger-three", "Three.");
    seed_at(
        &w,
        &project,
        "fingerprint",
        Some("command"),
        Some("src"),
        "Fingerprint three.",
        "2024-01-01T00:00:03Z",
    );
    learn_escape(&w, &project, "trigger-four", "trigger-four", "Four.");
    seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("docs"),
        "Docs three.",
        "2024-01-01T00:00:03Z",
    );
    learn_escape(&w, &project, "trigger-five", "trigger-five", "Five.");

    let views = all_proposals(&w, &project);
    assert_eq!(views.len(), 4, "{views:?}");
    let groups: Vec<(String, Option<String>, Option<String>)> =
        views.iter().map(|view| group_of(&w, view)).collect();
    for expected in [
        ("escape".to_owned(), None, Some("src".to_owned())),
        ("revert".to_owned(), None, Some("src".to_owned())),
        (
            "fingerprint".to_owned(),
            Some("command".to_owned()),
            Some("src".to_owned()),
        ),
        ("escape".to_owned(), None, Some("docs".to_owned())),
    ] {
        assert!(groups.contains(&expected), "{groups:?}");
    }
}

#[tokio::test]
async fn a_person_lesson_never_groups() {
    let w = quiet().await;
    let project = project_of(&w);
    for text in ["Person one.", "Person two.", "Person three."] {
        let lesson = w
            .core
            .crew_lesson_add(LessonScope::Repo(project.clone()), text)
            .unwrap();
        assert_eq!((lesson.check_kind, lesson.path_class), (None, None));
    }
    learn_escape(&w, &project, "trigger", "trigger", "One.");
    assert!(all_proposals(&w, &project).is_empty());
}

#[tokio::test]
async fn hidden_and_disabled_lessons_do_not_count() {
    let w = quiet().await;
    let project = project_of(&w);
    seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        "Escape one.",
        "2024-01-01T00:00:01Z",
    );
    seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        "Escape two.",
        "2024-01-01T00:00:02Z",
    );
    let disabled = seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("src"),
        "Escape off.",
        "2024-01-01T00:00:03Z",
    );
    let disabled = w
        .core
        .crew_lesson_set_enabled(&disabled.id, false, disabled.rev)
        .unwrap();
    learn_escape(&w, &project, "trigger-one", "trigger-one", "One.");
    assert!(
        all_proposals(&w, &project).is_empty(),
        "a disabled lesson is not an active source"
    );
    let _enabled = w
        .core
        .crew_lesson_set_enabled(&disabled.id, true, disabled.rev)
        .unwrap();
    learn_escape(&w, &project, "trigger-two", "trigger-two", "Two.");
    assert_eq!(all_proposals(&w, &project).len(), 1);

    // The same for a hidden lesson under `docs`.
    seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("docs"),
        "Docs one.",
        "2024-01-01T00:00:01Z",
    );
    seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("docs"),
        "Docs two.",
        "2024-01-01T00:00:02Z",
    );
    let hidden = seed_at(
        &w,
        &project,
        "escape",
        None,
        Some("docs"),
        "Docs hidden.",
        "2024-01-01T00:00:03Z",
    );
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 1 WHERE id = ?1",
            [&hidden.id],
        )
        .unwrap();
    learn_escape(&w, &project, "trigger-three", "trigger-three", "Three.");
    let groups: Vec<_> = all_proposals(&w, &project)
        .iter()
        .map(|view| group_of(&w, view))
        .collect();
    assert!(
        !groups.contains(&("escape".to_owned(), None, Some("docs".to_owned()))),
        "a hidden lesson does not count: {groups:?}"
    );
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 0 WHERE id = ?1",
            [&hidden.id],
        )
        .unwrap();
    learn_escape(&w, &project, "trigger-four", "trigger-four", "Four.");
    let groups: Vec<_> = all_proposals(&w, &project)
        .iter()
        .map(|view| group_of(&w, view))
        .collect();
    assert!(
        groups.contains(&("escape".to_owned(), None, Some("docs".to_owned()))),
        "{groups:?}"
    );
}

#[tokio::test]
async fn a_dismissed_pattern_is_not_proposed_again_even_with_a_fifth_lesson() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let dismissed = w
        .core
        .crew_skill_proposal_dismiss(&view.proposal.id, view.proposal.rev)
        .unwrap();
    assert_eq!(
        (dismissed.proposal.state.as_str(), dismissed.proposal.rev),
        ("dismissed", 2)
    );
    assert!(!dismissed.can_approve);

    learn_escape(&w, &project, "escape-fourth", "src", "A fourth escape.");
    learn_escape(&w, &project, "escape-fifth", "src", "A fifth escape.");
    let views = all_proposals(&w, &project);
    assert_eq!(views.len(), 1, "the dismissed row is remembered");
    assert_eq!(views[0].proposal.state, "dismissed");
    assert_eq!(views[0].proposal.lessons.len(), 3);
}

#[tokio::test]
async fn can_approve_is_false_after_a_source_lesson_is_disabled_or_deleted_and_the_proposal_stays()
{
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let lessons = sources(&w, &view);
    assert!(view.can_approve);

    let disabled = w
        .core
        .crew_lesson_set_enabled(&lessons[0].id, false, lessons[0].rev)
        .unwrap();
    let view_now = only_proposal(&w, &project);
    assert!(!view_now.can_approve, "a disabled source cannot approve");
    assert_eq!(view_now.proposal.state, "open");
    assert_eq!(view_now.proposal.rev, view.proposal.rev, "the row is kept");

    let enabled = w
        .core
        .crew_lesson_set_enabled(&disabled.id, true, disabled.rev)
        .unwrap();
    assert!(only_proposal(&w, &project).can_approve);

    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 1 WHERE id = ?1",
            [&lessons[0].id],
        )
        .unwrap();
    assert!(
        !only_proposal(&w, &project).can_approve,
        "a hidden source cannot approve"
    );
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_lessons SET hidden_by_cap = 0 WHERE id = ?1",
            [&lessons[0].id],
        )
        .unwrap();
    assert!(only_proposal(&w, &project).can_approve);

    let deleted = w
        .core
        .crew_lesson_delete(&lessons[1].id, lessons[1].rev)
        .unwrap();
    assert!(deleted.0.starts_with("les_"));
    let view_now = only_proposal(&w, &project);
    assert!(!view_now.can_approve, "a deleted source cannot approve");
    assert_eq!(view_now.proposal.state, "open");
    assert!(w.core.store.crew_lesson(&enabled.id).unwrap().is_some());
}

#[tokio::test]
async fn approve_writes_the_skill_sets_the_state_and_audits_file_then_approval() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let lessons = sources(&w, &view);
    let revs: Vec<u32> = lessons.iter().map(|lesson| lesson.rev).collect();
    let name = "crew-escape-src";

    let approved = w
        .core
        .crew_skill_proposal_approve(&view.proposal.id, &view.proposal.text, view.proposal.rev)
        .unwrap();
    assert_eq!(
        (
            approved.proposal.state.as_str(),
            approved.proposal.skill_name.as_deref(),
            approved.proposal.rev,
        ),
        ("approved", Some(name), 2)
    );
    assert!(approved.proposal.approved_at.is_some());
    assert!(!approved.can_approve, "an approved proposal is not open");
    assert!(w.core.skills.local_skill_exists(name));
    let file = skill_dir(&w, name).join("SKILL.md");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("name: crew-escape-src"), "{text}");
    assert!(text.contains("- Seeded escape two."), "{text}");
    assert_eq!(
        filtered_actions(&w),
        [
            "crew.proposal.create",
            "skills.local.write",
            "crew.proposal.approve",
        ]
    );
    // "Now a skill" is read, never stored: the source lessons keep their revision.
    for (lesson, rev) in lessons.iter().zip(revs) {
        assert_eq!(
            w.core.store.crew_lesson(&lesson.id).unwrap().unwrap().rev,
            rev
        );
    }
}

#[tokio::test]
async fn a_stale_rev_writes_no_file() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let stale = w
        .core
        .crew_skill_proposal_approve(
            &view.proposal.id,
            &view.proposal.text,
            view.proposal.rev + 1,
        )
        .unwrap_err();
    assert_stale(stale);
    assert!(
        !skill_dir(&w, "crew-escape-src").exists(),
        "the revision is checked before the file"
    );
    let unchanged = only_proposal(&w, &project);
    assert_eq!(
        (unchanged.proposal.state.as_str(), unchanged.proposal.rev),
        ("open", 1)
    );
}

#[tokio::test]
async fn approve_or_dismiss_of_a_dismissed_proposal_is_not_open() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let dismissed = w
        .core
        .crew_skill_proposal_dismiss(&view.proposal.id, view.proposal.rev)
        .unwrap();
    assert_not_open(
        w.core
            .crew_skill_proposal_approve(
                &dismissed.proposal.id,
                &view.proposal.text,
                dismissed.proposal.rev,
            )
            .unwrap_err(),
    );
    assert!(!skill_dir(&w, "crew-escape-src").exists());
    assert_not_open(
        w.core
            .crew_skill_proposal_dismiss(&dismissed.proposal.id, dismissed.proposal.rev)
            .unwrap_err(),
    );
}

#[tokio::test]
async fn approve_refuses_empty_long_or_secret_text_and_writes_no_file() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let id = &view.proposal.id;

    assert_invalid(
        w.core
            .crew_skill_proposal_approve(id, " \n\t ", view.proposal.rev)
            .unwrap_err(),
        "A skill is 1 to 600 characters.",
    );
    assert_invalid(
        w.core
            .crew_skill_proposal_approve(id, &"x".repeat(601), view.proposal.rev)
            .unwrap_err(),
        "A skill is 1 to 600 characters.",
    );
    let secret = format!(
        "---\nname: crew-escape-secret\ndescription: A template.\n---\n\nThe token is ghp_{}.",
        "q".repeat(36)
    );
    assert_secret_refused(
        w.core
            .crew_skill_proposal_approve(id, &secret, view.proposal.rev)
            .unwrap_err(),
    );
    assert!(
        !w._dir.path().join("data").join("skills-local").exists(),
        "a refused text writes nothing"
    );
    let unchanged = only_proposal(&w, &project);
    assert_eq!(
        (unchanged.proposal.state.as_str(), unchanged.proposal.rev),
        ("open", 1)
    );
}

#[tokio::test]
async fn approve_refuses_a_name_the_library_or_a_local_skill_holds() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let library = "---\nname: how\ndescription: A template.\n---\n\n- One.\n";
    assert_name_taken(
        w.core
            .crew_skill_proposal_approve(&view.proposal.id, library, view.proposal.rev)
            .unwrap_err(),
        "A skill named how already exists in the shared library. Choose another name.",
    );
    assert!(!skill_dir(&w, "how").exists());

    let existing = "---\nname: crew-escape-src\ndescription: Existing.\n---\n\n- One.\n";
    w.core
        .skills
        .write_local_skill(LocalSkillWrite::create(existing))
        .unwrap();
    assert_name_taken(
        w.core
            .crew_skill_proposal_approve(&view.proposal.id, &view.proposal.text, view.proposal.rev)
            .unwrap_err(),
        "A skill named crew-escape-src already exists on this device. Edit it instead.",
    );
    let unchanged = only_proposal(&w, &project);
    assert_eq!(unchanged.proposal.state, "open");
}

#[tokio::test]
async fn approved_lessons_leave_the_brief_and_return_when_the_skill_folder_is_removed() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let lessons = sources(&w, &view);
    let texts: Vec<String> = lessons.iter().map(|lesson| lesson.text.clone()).collect();
    let revs: Vec<u32> = lessons.iter().map(|lesson| lesson.rev).collect();
    let approved = w
        .core
        .crew_skill_proposal_approve(&view.proposal.id, &view.proposal.text, view.proposal.rev)
        .unwrap();
    let name = approved.proposal.skill_name.clone().unwrap();

    // The list says Now a skill and that the brief would skip it; the detail says the same.
    let list = w
        .core
        .crew_lessons_list(&project, crate::crew::lessons_view::LessonFilter::All)
        .unwrap();
    for lesson in &lessons {
        let view = list
            .lessons
            .iter()
            .find(|view| view.lesson.id == lesson.id)
            .unwrap();
        assert_eq!(view.skill.as_deref(), Some(name.as_str()));
        assert!(!view.in_next_brief);
    }
    let detail = w
        .core
        .crew_lesson_detail(&lessons[0].id, Some(&project))
        .unwrap();
    assert_eq!(detail.lesson.skill.as_deref(), Some(name.as_str()));
    assert!(!detail.lesson.in_next_brief);

    // The brief a start sends leaves them out while the skill exists.
    let (_, prompt) = start(&w, "A skill carries it.").await;
    for text in &texts {
        assert!(!prompt.contains(text.as_str()), "{prompt}");
    }

    // Removing the folder returns them to normal, without a revision change.
    std::fs::remove_dir_all(skill_dir(&w, &name)).unwrap();
    assert!(!w.core.skills.local_skill_exists(&name));
    let list = w
        .core
        .crew_lessons_list(&project, crate::crew::lessons_view::LessonFilter::All)
        .unwrap();
    for (lesson, rev) in lessons.iter().zip(&revs) {
        let view = list
            .lessons
            .iter()
            .find(|view| view.lesson.id == lesson.id)
            .unwrap();
        assert!(view.skill.is_none());
        assert!(view.in_next_brief);
        assert_eq!(view.lesson.rev, *rev);
    }
    let detail = w
        .core
        .crew_lesson_detail(&lessons[0].id, Some(&project))
        .unwrap();
    assert!(detail.lesson.skill.is_none());
    assert!(detail.lesson.in_next_brief);

    let thread = w.core.store.thread(&w.lead).unwrap();
    let stamper = crate::crew::stamp::Stamper {
        crew_id: wire::CrewId::new(thread.id.clone()).unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: crate::crew::tool::stamp_now().unwrap(),
    };
    let draft = stamper.brief(&brief(json!({}))).unwrap();
    let answer_key = stamper.answer_key(&key(), &draft.id).unwrap();
    let injection = w.core.crew_lessons_brief(&project, &answer_key);
    let block = injection.block.expect("the lessons return to the brief");
    for text in &texts {
        assert!(block.contains(text.as_str()), "{block}");
    }
    for lesson in &lessons {
        assert!(injection.used.contains(&lesson.id), "{}", lesson.id);
    }
}

#[tokio::test]
async fn proposals_change_nothing_about_the_key_the_route_the_leases_or_the_checks() {
    use crate::crew::start::{Launch, Links};

    let with = quiet().await;
    let without = quiet().await;
    let project = project_of(&with);
    let view = make_proposal(&with, &project);
    with.core
        .crew_skill_proposal_approve(&view.proposal.id, &view.proposal.text, view.proposal.rev)
        .unwrap();
    assert_eq!(
        with.core.crew_needs_you().unwrap().count,
        0,
        "a proposal is never a needs-you item"
    );

    // One brief and one key, made once, so both worlds carry the same content addresses.
    let now = crate::crew::tool::stamp_now().unwrap();
    let stamper = crate::crew::stamp::Stamper {
        crew_id: wire::CrewId::new("crew-fixed").unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: now.clone(),
    };
    let stamped_brief = stamper
        .brief(&brief(
            json!({ "outcome": "One task, with and without a proposal." }),
        ))
        .unwrap();
    let stamped_key = stamper.answer_key(&key(), &stamped_brief.id).unwrap();
    let task = stamped_brief.id.as_str().to_owned();

    for w in [&with, &without] {
        let thread = w.core.store.thread(&w.lead).unwrap();
        let answer = w
            .core
            .launch_crew_task(Launch {
                lead: &thread,
                now: now.clone(),
                brief: stamped_brief.clone(),
                key: stamped_key.clone(),
                role: None,
                lane: None,
                lane_keeps_lead: false,
                author_engine: None,
                prefix: "worker",
                prompt: None,
                links: Links::default(),
            })
            .await
            .unwrap();
        assert_eq!(answer["routed"], true, "{answer}");
    }

    let key_with = with.core.store.crew_answer_key(&task).unwrap().unwrap();
    let key_without = without.core.store.crew_answer_key(&task).unwrap().unwrap();
    assert_eq!(key_with, stamped_key);
    assert_eq!(key_without, stamped_key);
    assert_eq!(route_of(&with, &task), route_of(&without, &task));
    assert_eq!(leases_of(&with, &task), leases_of(&without, &task));
}

#[tokio::test]
async fn a_proposal_answers_each_sources_own_evidence_and_a_purged_source_answers_none() {
    let w = quiet().await;
    let project = project_of(&w);
    let view = make_proposal(&w, &project);
    let source_lessons: Vec<CrewLesson> = sources(&w, &view);
    let listed = w
        .core
        .crew_skill_proposal_lessons(&view.proposal.lessons)
        .unwrap();
    assert_eq!(listed.len(), source_lessons.len());
    for (listed, source) in listed.iter().zip(&source_lessons) {
        assert_eq!(listed.id, source.id);
        assert_eq!(listed.evidence, source.evidence);
    }
    let with_evidence = source_lessons
        .iter()
        .find(|lesson| !lesson.evidence.is_empty())
        .expect("the learned source carries evidence");
    assert_eq!(
        with_evidence
            .evidence
            .first()
            .map(|evidence| evidence.kind.as_str()),
        Some("escape")
    );

    w.core
        .crew_lesson_delete(&with_evidence.id, with_evidence.rev)
        .unwrap();
    w.core
        .store
        .purge_deleted_lessons("9999-01-01T00:00:00Z")
        .unwrap();
    let after = w
        .core
        .crew_skill_proposal_lessons(&view.proposal.lessons)
        .unwrap();
    let purged = after
        .iter()
        .find(|lesson| lesson.id == with_evidence.id)
        .expect("the proposal keeps the id");
    assert!(purged.evidence.is_empty());
    for (before, after) in listed.iter().zip(&after) {
        if before.id != with_evidence.id {
            assert_eq!(before, after);
        }
    }
}

#[tokio::test]
async fn an_unknown_repository_is_not_found() {
    let w = quiet().await;
    let missing = w
        .core
        .crew_skill_proposals("no-such-repository")
        .unwrap_err();
    assert_eq!(
        (missing.status(), missing.code(), missing.to_string()),
        (404, "not_found", "project not found".to_owned())
    );
}

#[tokio::test]
async fn preview_off_refuses_every_proposal_method() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let project = project_of(&w);
    assert_preview_off(w.core.crew_skill_proposals(&project));
    assert_preview_off(w.core.crew_skill_proposal_lessons(&[]));
    assert_preview_off(w.core.crew_skill_proposal_approve("prop_x", "Text.", 1));
    assert_preview_off(w.core.crew_skill_proposal_dismiss("prop_x", 1));
    assert!(
        w.core.store.skill_proposals(&project).unwrap().is_empty(),
        "nothing was written"
    );
}
