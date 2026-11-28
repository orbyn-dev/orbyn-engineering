//! The debug-only dev seed on a real `Core` (KC9): generic escape-source lessons make one proposal at three, two make
//! none, repeated calls join the same group without a second proposal, bad counts and unknown repositories are refused,
//! every use audits with counts only, and Crew Preview still gates the method.

use std::sync::Arc;

use serde_json::json;

use super::harness::{Turn, World, world};
use crate::error::CoreError;
use crate::store::{LessonEvidence, proposal_group_key};

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

#[tokio::test]
async fn three_lessons_make_one_proposal_from_generic_escape_lessons() {
    let w = quiet().await;
    let repo = project_of(&w);
    let seeded = w.core.dev_seed_proposal(&repo, 3, "src/dev-seed").unwrap();
    assert_eq!(seeded.lessons.len(), 3);
    let proposal_id = seeded.proposal_id.expect("three lessons make a proposal");

    let views = w.core.crew_skill_proposals(&repo).unwrap();
    assert_eq!(views.len(), 1, "three lessons make one proposal");
    let view = views.first().expect("the crew's one proposal");
    assert_eq!(view.proposal.id, proposal_id);
    assert_eq!(view.proposal.state, "open");
    assert_eq!(
        view.proposal.group_key,
        proposal_group_key("escape", None, Some("src/dev-seed"))
    );
    assert!(view.can_approve);
    assert_eq!(view.proposal.lessons.len(), 3);
    for id in &seeded.lessons {
        assert!(view.proposal.lessons.contains(id), "{id}");
    }

    // The seed answers its lesson ids oldest first, so the first lesson is found by its own evidence id rather
    // than by the place the seed put it in.
    let lesson = w
        .core
        .store
        .crew_lessons("repo", Some(&repo))
        .unwrap()
        .into_iter()
        .find(|lesson| {
            lesson
                .evidence
                .iter()
                .any(|evidence| evidence.id == "dev-seed-1")
        })
        .expect("the first seeded lesson");
    assert_eq!(lesson.scope, "repo");
    assert_eq!(lesson.project_id.as_deref(), Some(repo.as_str()));
    assert_eq!(lesson.source, "escape");
    assert_eq!(lesson.check_kind, None);
    assert_eq!(lesson.path_class.as_deref(), Some("src/dev-seed"));
    assert_eq!(lesson.text, "Dev seed lesson 1 for src/dev-seed.");
    assert_eq!(lesson.fingerprint, None);
    assert_eq!(
        lesson.evidence,
        vec![LessonEvidence {
            kind: "escape".to_owned(),
            id: "dev-seed-1".to_owned(),
        }]
    );
    assert_eq!(lesson.state, "active");
}

#[tokio::test]
async fn two_lessons_make_no_proposal() {
    let w = quiet().await;
    let repo = project_of(&w);
    let seeded = w.core.dev_seed_proposal(&repo, 2, "src/dev-seed").unwrap();
    assert_eq!(seeded.lessons.len(), 2);
    assert_eq!(seeded.proposal_id, None);
    assert!(w.core.crew_skill_proposals(&repo).unwrap().is_empty());
}

#[tokio::test]
async fn one_then_two_lessons_make_one_proposal_and_a_repeat_keeps_it() {
    let w = quiet().await;
    let repo = project_of(&w);
    let first = w.core.dev_seed_proposal(&repo, 1, "src/dev-seed").unwrap();
    assert_eq!(first.proposal_id, None);
    let second = w.core.dev_seed_proposal(&repo, 2, "src/dev-seed").unwrap();
    let proposal_id = second
        .proposal_id
        .clone()
        .expect("the second call completes the group");
    assert_eq!(second.lessons.len(), 2);

    let repeated = w.core.dev_seed_proposal(&repo, 3, "src/dev-seed").unwrap();
    assert_eq!(repeated.proposal_id.as_deref(), Some(proposal_id.as_str()));
    assert_eq!(repeated.lessons.len(), 3);

    let views = w.core.crew_skill_proposals(&repo).unwrap();
    assert_eq!(views.len(), 1, "a repeated 3 never makes a second proposal");
    let view = views.first().expect("the crew's one proposal");
    assert_eq!(view.proposal.id, proposal_id);
    assert_eq!(view.proposal.lessons.len(), 3);
    let lessons = w.core.store.crew_lessons("repo", Some(&repo)).unwrap();
    assert_eq!(lessons.len(), 6, "every call adds its lessons to the group");
    assert!(lessons.iter().all(|lesson| {
        lesson.source == "escape" && lesson.path_class.as_deref() == Some("src/dev-seed")
    }));
}

#[tokio::test]
async fn a_count_outside_one_to_six_is_invalid() {
    let w = quiet().await;
    let repo = project_of(&w);
    for lessons in [0, 7] {
        match w.core.dev_seed_proposal(&repo, lessons, "src/dev-seed") {
            Err(CoreError::Invalid(message)) => assert_eq!(message, "lessons must be 1 to 6"),
            other => panic!("lessons {lessons}: {other:?}"),
        }
    }
    assert!(
        w.core
            .store
            .crew_lessons("repo", Some(&repo))
            .unwrap()
            .is_empty(),
        "a refused count writes nothing"
    );
}

#[tokio::test]
async fn an_unknown_repository_is_not_found() {
    let w = quiet().await;
    let error = w
        .core
        .dev_seed_proposal("no-such-repository", 3, "src/dev-seed")
        .unwrap_err();
    assert_eq!(
        (error.status(), error.code(), error.to_string()),
        (404, "not_found", "project not found".to_owned())
    );
}

#[tokio::test]
async fn every_seed_audits_before_its_lessons_with_counts_only() {
    let w = quiet().await;
    let repo = project_of(&w);
    w.core.dev_seed_proposal(&repo, 2, "src/dev-seed").unwrap();
    let rows = w.core.store.list_audit(None, 500).unwrap();
    let seed = rows
        .iter()
        .find(|row| row.action == "crew.dev.seed_proposal")
        .expect("every use writes an audit row");
    assert_eq!(seed.actor, "dev");
    assert_eq!(seed.source, "dev");
    assert_eq!(seed.target, repo);
    assert_eq!(seed.detail, json!({ "lessons": 2 }));
    assert!(
        !seed.detail.to_string().contains("Dev seed lesson"),
        "the detail holds counts only, never text"
    );
    let lessons: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.action == "crew.lesson.learn"
                && row.detail["project"].as_str() == Some(repo.as_str())
        })
        .collect();
    assert_eq!(lessons.len(), 2);
    assert!(
        lessons.iter().all(|row| row.id > seed.id),
        "the audit row is written before the lesson rows"
    );
}

#[tokio::test]
async fn preview_off_refuses_the_seed() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let repo = project_of(&w);
    let error = w
        .core
        .dev_seed_proposal(&repo, 3, "src/dev-seed")
        .unwrap_err();
    assert_eq!(
        (error.status(), error.code(), error.to_string()),
        (404, "not_found", "route not found".to_owned())
    );
    assert!(
        w.core
            .store
            .crew_lessons("repo", Some(&repo))
            .unwrap()
            .is_empty(),
        "Preview off writes nothing"
    );
}
