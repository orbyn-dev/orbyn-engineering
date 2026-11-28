//! A Crew-started fix (a merge-comment fix, a review fix) refused because review capacity is full: the person or the
//! lead is told plainly that it is waiting, nothing retries it on a timer, and a later trigger starts it once capacity
//! frees. Real `Core`, fake engines, real git in a temporary repository.

use serde_json::json;

use super::autopilot_merge::{merging, passed};
use super::harness::World;
use super::review_support::{arc, built, crew, finding, fixed, reviewed};
use crate::crew::capacity::{CapacitySettings, PausedKind, Sampling};
use crate::crew::fixtures::merge_record;
use crate::crew::{MergeOutcome, Mode, ReviewAnswer};
use crate::testing::run_git;

const NOW: i64 = 1_800_000_000;

/// The one row a read returned, unpacked by pattern so a test never reads a place in the list. A read that returns none
/// or several fails here with the rows attached.
fn one<'a, T: std::fmt::Debug>(rows: &'a [T], what: &str) -> &'a T {
    match rows {
        [row] => row,
        [] => panic!("no {what}: {rows:?}"),
        _ => panic!("{} {what}s where one is expected: {rows:?}", rows.len()),
    }
}

/// The review task of the one lane a test asks for, found by the name it asked for: a review answer lists its lanes in
/// start order, not in a fixture's order, so a lane a repo rule adds must not stand in for the lane the test waits on.
async fn one_lane(w: &World, subject: &str, asked: &str) -> String {
    let (answer, error) = w
        .lead("crew_review", json!({ "task": subject, "lanes": [asked] }))
        .await;
    assert!(!error, "{answer}");
    let lanes = answer["lanes"]
        .as_array()
        .expect("the review answer lists its lanes");
    assert_eq!(
        lanes.len(),
        1,
        "the test asks for one lane, and no rule adds another: {answer}"
    );
    let lane = lanes[0]["lane"].as_str().expect("the lane's name");
    assert_eq!(
        lane, asked,
        "the one lane is the lane the test asked for: {answer}"
    );
    lanes[0]["task"]
        .as_str()
        .expect("the lane's review task")
        .to_owned()
}

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// Pause on, with a limit of 1: two waiting medium merges are over it.
fn pause_over_the_limit(w: &World) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit: 1,
                sampling: Sampling::Off,
                pause_enabled: true,
            },
        )
        .unwrap();
}

/// A medium merge of `task` that waits for a person.
fn wait_for_a_person(w: &World, task: &str) {
    let mut merge = merge_record(task, "/repo", "main", "x");
    merge.risk = "medium".into();
    merge.crew_id = w.task(task).crew_id;
    w.core.store.add_crew_merge(&merge).unwrap();
}

fn capacity_frees(w: &World) {
    w.core
        .store
        .reader()
        .execute_batch(
            "UPDATE crew_merges SET review_state = 'looks_good' WHERE review_state = 'waiting'",
        )
        .unwrap();
}

#[tokio::test]
async fn a_paused_comment_fix_says_it_is_waiting_for_capacity_and_a_later_comment_starts_it() {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    let other = passed(&m.w, &["src/**"], "Add another feature.").await;
    let third = passed(&m.w, &["src/**"], "Add a third feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let outcomes = m.process(NOW + 1).await;
    let [MergeOutcome::Merged { merge_commit, .. }] = &outcomes[..] else {
        panic!("the merge did not happen: {outcomes:?}")
    };
    run_git(&m.w.repo, &["checkout", "--detach", "-q", merge_commit]);
    wait_for_a_person(&m.w, &other);
    wait_for_a_person(&m.w, &third);
    pause_over_the_limit(&m.w);
    let tasks = || {
        m.w.core
            .store
            .crew_work_tasks(&m.w.lead, 100)
            .unwrap()
            .len()
    };
    let before = tasks();

    m.w.core
        .crew_answer_review(
            &id,
            ReviewAnswer::Comment("Tidy the feature.".into()),
            &m.target,
            NOW + 2,
        )
        .await
        .unwrap();
    let record = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    let said = record.needs_person.clone().unwrap_or_default();
    assert!(
        said.contains("waiting") && said.contains("review capacity"),
        "the person is told the fix waits for review capacity, not a generic failure: {said:?}"
    );
    assert_eq!(
        record.review_state, "commented",
        "the Comment stays stored so a later trigger can use it"
    );
    assert_eq!(tasks(), before, "nothing was started");
    let rows = m.w.core.store.crew_paused_starts(&project(&m.w)).unwrap();
    let refused = one(&rows, "refused comment fix row");
    assert_eq!(refused.kind, PausedKind::CommentFix);
    assert_eq!(refused.key, id, "the row is the refused fix's own trigger");
    assert!(!refused.started);

    // No timer retries it: nothing changes by itself.
    let events = || m.w.core.store.crew_autopilot_events(&id).unwrap().len();
    let ledger = events();
    assert_eq!(tasks(), before);
    assert_eq!(events(), ledger, "no second attempt left a ledger row");
    assert_eq!(
        m.w.core
            .store
            .crew_merge(&id)
            .unwrap()
            .unwrap()
            .needs_person,
        Some(said)
    );

    // A later trigger (the lead's tool) after capacity frees starts it, and the waiting note goes.
    capacity_frees(&m.w);
    let (answer, error) =
        m.w.call(
            &m.w.lead_token,
            "crew_answer_review",
            json!({"task": id, "branch": m.target.branch}),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(tasks(), before + 1, "the fix started");
    let rows = m.w.core.store.crew_paused_starts(&project(&m.w)).unwrap();
    let row = one(&rows, "started comment fix row");
    assert_eq!(row.id, refused.id, "still one row, not a second");
    assert_eq!(row.key, id, "the same trigger's row");
    assert!(row.started, "the same trigger started, so the row says so");
    assert_eq!(
        m.w.core
            .store
            .crew_merge(&id)
            .unwrap()
            .unwrap()
            .needs_person,
        None,
        "the waiting note is gone once the fix started"
    );
}

#[tokio::test]
async fn the_lead_tool_for_a_paused_comment_fix_shows_the_refusal_once_and_does_not_retry() {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    let other = passed(&m.w, &["src/**"], "Add another feature.").await;
    let third = passed(&m.w, &["src/**"], "Add a third feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let outcomes = m.process(NOW + 1).await;
    let [MergeOutcome::Merged { merge_commit, .. }] = &outcomes[..] else {
        panic!("the merge did not happen: {outcomes:?}")
    };
    run_git(&m.w.repo, &["checkout", "--detach", "-q", merge_commit]);
    m.w.core
        .store
        .set_merge_review(&id, "commented", Some("Tidy the feature."))
        .unwrap();
    wait_for_a_person(&m.w, &other);
    wait_for_a_person(&m.w, &third);
    pause_over_the_limit(&m.w);
    let before =
        m.w.core
            .store
            .crew_work_tasks(&m.w.lead, 100)
            .unwrap()
            .len();
    let (answer, error) =
        m.w.call(
            &m.w.lead_token,
            "crew_answer_review",
            json!({"task": id, "branch": m.target.branch}),
        )
        .await;
    assert!(error, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains("review capacity"),
        "{answer}"
    );
    assert_eq!(
        m.w.core
            .store
            .crew_work_tasks(&m.w.lead, 100)
            .unwrap()
            .len(),
        before
    );
}

/// A reviewed task whose finding a repro-less second review confirms, with the project over its review limit.
async fn confirmed_while_paused() -> (World, String, String) {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1..=4 => reviewed(vec![finding("src/lib.txt", 1, "Wrong value.")]),
        5 => fixed(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(&w, &subject, "correctness").await;
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    wait_for_a_person(&w, &subject);
    wait_for_a_person(&w, &lane);
    pause_over_the_limit(&w);
    let (pending, error) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    assert!(!error, "{pending}");
    w.until_state(pending["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    (w, subject, id)
}

#[tokio::test]
async fn a_paused_review_fix_tells_the_lead_it_is_waiting_for_capacity_and_a_later_check_starts_it()
{
    let (w, subject, id) = confirmed_while_paused().await;
    w.until("the lead is told", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|p| p.contains("<crew-review") && p.contains("Review capacity is full"))
    })
    .await;
    let told = w
        .sent_to(&w.lead)
        .into_iter()
        .find(|p| p.contains("Review capacity is full"))
        .unwrap();
    assert!(told.contains("waiting"), "{told}");
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().is_empty());
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    let refused = one(&rows, "refused review fix row");
    assert_eq!(
        (
            refused.kind,
            refused.waiting,
            refused.limit,
            refused.started
        ),
        (PausedKind::ReviewFix, 2, 1, false),
        "the held row is the refused review fix, with the counts of the refusal"
    );
    let notices = || {
        w.sent_to(&w.lead)
            .iter()
            .filter(|p| p.contains("Review capacity is full"))
            .count()
    };
    assert_eq!(notices(), 1, "told once, not retried");

    // Capacity frees. A confirmed finding is decided once, so the new trigger is a new review that files it again.
    capacity_frees(&w);
    let (again, error) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    assert!(error, "a decided finding is not checked twice: {again}");
    let lane = one_lane(&w, &subject, "correctness").await;
    w.until_state(&lane, "passed").await;
    let finding = w.status(&subject)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap().to_owned())
        .find(|f| *f != id)
        .expect("the new review filed the finding again");
    let (pending, error) = w
        .lead("crew_check_finding", json!({ "finding": finding }))
        .await;
    assert!(!error, "{pending}");
    w.until("the fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
}

#[tokio::test]
async fn a_started_comment_fix_leaves_any_other_needs_person_note_alone() {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let outcomes = m.process(NOW + 1).await;
    let [MergeOutcome::Merged { merge_commit, .. }] = &outcomes[..] else {
        panic!("the merge did not happen: {outcomes:?}")
    };
    run_git(&m.w.repo, &["checkout", "--detach", "-q", merge_commit]);
    m.w.core
        .store
        .set_merge_review(&id, "commented", Some("Tidy the feature."))
        .unwrap();
    m.w.core
        .store
        .set_merge_needs_person(&id, Some("repository checks fail on the tip"))
        .unwrap();
    let before =
        m.w.core
            .store
            .crew_work_tasks(&m.w.lead, 100)
            .unwrap()
            .len();
    let (answer, error) =
        m.w.call(
            &m.w.lead_token,
            "crew_answer_review",
            json!({"task": id, "branch": m.target.branch}),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(
        m.w.core
            .store
            .crew_work_tasks(&m.w.lead, 100)
            .unwrap()
            .len(),
        before + 1
    );
    assert_eq!(
        m.w.core
            .store
            .crew_merge(&id)
            .unwrap()
            .unwrap()
            .needs_person
            .as_deref(),
        Some("repository checks fail on the tip")
    );
}
