//! The list of starts that review capacity refused: one bounded row per Crew-started trigger, updated by a repeat, marked
//! when the same trigger starts, and never written with pausing off or for a start a person made. Real `Core`, fake engines.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::{CrewId, CrewMember, CrewToken, ErrorCode};
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::capacity::{CapacitySettings, PausedKind, PausedTrigger, Sampling};
use crate::crew::fixtures::merge_record;
use crate::crew::leases::timestamp;
use crate::crew::stamp::Stamper;
use crate::crew::start::{Launch, Links};

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

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

fn save(w: &World, limit: u32, pause: bool) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit,
                sampling: Sampling::Off,
                pause_enabled: pause,
            },
        )
        .unwrap();
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

/// A launch the way a fix makes it. The brief's time is fixed, so the same `path` is the same trigger (the same brief hash).
async fn launch(
    w: &World,
    path: &str,
    crew_started: bool,
    trigger: Option<PausedTrigger>,
) -> Result<serde_json::Value, wire::CrewError> {
    launch_at(w, path, crew_started, trigger, 1_800_000_000).await
}

fn trig(kind: PausedKind, key: &str) -> Option<PausedTrigger> {
    Some(PausedTrigger {
        kind,
        key: key.into(),
    })
}

/// The same, with the brief stamped at `at`, so a retry changes the brief's hash the way a real one does.
async fn launch_at(
    w: &World,
    path: &str,
    crew_started: bool,
    trigger: Option<PausedTrigger>,
    at: i64,
) -> Result<serde_json::Value, wire::CrewError> {
    let lead = w.core.store.thread(&w.lead).unwrap();
    let stamper = Stamper {
        crew_id: CrewId::new(lead.id.clone()).unwrap(),
        engine: CrewToken::new("alpha").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: timestamp(at).unwrap(),
    };
    let brief = stamper
        .brief(&brief(
            json!({ "files": [{ "path": path, "mode": "write" }], "outcome": "A cleanup." }),
        ))
        .unwrap();
    let key = stamper.answer_key(&key(), &brief.id).unwrap();
    w.core
        .launch_crew_task(Launch {
            lead: &lead,
            now: stamper.now,
            brief,
            key,
            role: None,
            lane: None,
            lane_keeps_lead: false,
            author_engine: None,
            prefix: "worker",
            prompt: None,
            links: Links {
                crew_started,
                trigger,
                ..Links::default()
            },
        })
        .await
}

#[tokio::test]
async fn a_refused_crew_start_stores_one_row_with_counts_and_no_task() {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 2, true);
    let tasks = w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len();
    let refused = launch(
        &w,
        "src/clean/**",
        true,
        trig(PausedKind::CommentFix, "merge-1"),
    )
    .await
    .unwrap_err();
    assert_eq!(
        refused.code,
        ErrorCode::NotAuthorized,
        "still refused as before"
    );
    assert_eq!(
        w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len(),
        tasks
    );
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(
        (row.kind, row.waiting, row.limit, row.started),
        (PausedKind::CommentFix, 3, 2, false)
    );
    assert_eq!(row.key, "merge-1");
    assert_eq!(row.first_seen_unix, row.last_seen_unix);
}

#[tokio::test]
async fn a_repeat_of_the_same_trigger_updates_the_one_row_and_a_different_one_adds_another() {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 2, true);
    launch(
        &w,
        "src/clean/**",
        true,
        trig(PausedKind::CommentFix, "merge-1"),
    )
    .await
    .unwrap_err();
    waiting_one_more(&w).await;
    launch(
        &w,
        "src/clean/**",
        true,
        trig(PausedKind::CommentFix, "merge-1"),
    )
    .await
    .unwrap_err();
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate: {rows:?}");
    assert_eq!(rows[0].waiting, 4, "the counts of the last refusal");
    launch(
        &w,
        "src/other/**",
        true,
        trig(PausedKind::EscapeCheck, "esc-1"),
    )
    .await
    .unwrap_err();
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.kind == PausedKind::EscapeCheck));
}

async fn waiting_one_more(w: &World) {
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/extra/**", "mode": "write" }], "outcome": "Extra." })),
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

#[tokio::test]
async fn when_the_same_trigger_later_starts_its_row_is_marked_started_not_duplicated() {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 2, true);
    launch(
        &w,
        "src/clean/**",
        true,
        trig(PausedKind::CommentFix, "merge-1"),
    )
    .await
    .unwrap_err();
    capacity_frees(&w);
    let started = launch(
        &w,
        "src/clean/**",
        true,
        trig(PausedKind::CommentFix, "merge-1"),
    )
    .await
    .unwrap();
    assert!(started["task"].is_string(), "{started}");
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].started && rows[0].started_unix.is_some());
    // A trigger that never was refused leaves no row.
    launch(
        &w,
        "src/fresh/**",
        true,
        trig(PausedKind::CommentFix, "merge-9"),
    )
    .await
    .unwrap();
    assert_eq!(
        w.core.store.crew_paused_starts(&project(&w)).unwrap().len(),
        1
    );
}

#[tokio::test]
async fn with_pausing_off_or_for_a_person_start_nothing_is_written() {
    let w = quiet().await;
    waiting(&w, 3).await;
    // Pause never turned on, then explicitly off with the queue over the limit.
    launch(&w, "src/a/**", true, trig(PausedKind::CommentFix, "k"))
        .await
        .unwrap();
    save(&w, 1, false);
    launch(&w, "src/b/**", true, trig(PausedKind::CommentFix, "k"))
        .await
        .unwrap();
    // Pause on: a person's start goes and stores nothing.
    save(&w, 1, true);
    launch(&w, "src/c/**", false, None).await.unwrap();
    assert!(
        w.core
            .store
            .crew_paused_starts(&project(&w))
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_capacity_state_lists_the_held_starts_and_an_old_client_reading_none_sees_an_empty_list()
 {
    let w = quiet().await;
    waiting(&w, 3).await;
    assert!(
        w.core
            .crew_capacity_state(&project(&w))
            .unwrap()
            .held
            .is_empty()
    );
    save(&w, 2, true);
    launch(
        &w,
        "src/clean/**",
        true,
        trig(PausedKind::CommentFix, "merge-1"),
    )
    .await
    .unwrap_err();
    let state = w.core.crew_capacity_state(&project(&w)).unwrap();
    assert_eq!(state.held.len(), 1);
    let held = &state.held[0];
    assert_eq!(
        (held.kind, held.state, held.waiting, held.limit),
        (
            wire::HeldStartKind::CommentFix,
            wire::HeldStartState::Held,
            3,
            2
        )
    );
    assert_eq!(held.started_unix, None);
}

#[tokio::test]
async fn with_the_preview_flag_off_nothing_is_written() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    save(&w, 1, true);
    let trigger = crate::crew::capacity::PausedTrigger {
        kind: PausedKind::ReviewFix,
        key: "k".into(),
    };
    w.core
        .capacity_start_check(
            &project(&w),
            Some((crate::crew::capacity::Priority::Low, true)),
            Some(&trigger),
        )
        .unwrap();
    assert!(
        w.core
            .store
            .crew_paused_starts(&project(&w))
            .unwrap()
            .is_empty()
    );
}

/// A refused start, then a retry stamped later (so a different brief hash) that now goes: one row, marked started.
async fn retried_ends_with_one_started_row(kind: PausedKind, key: &str) {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 2, true);
    launch_at(&w, "src/clean/**", true, trig(kind, key), 1_800_000_000)
        .await
        .unwrap_err();
    capacity_frees(&w);
    launch_at(&w, "src/clean/**", true, trig(kind, key), 1_800_000_500)
        .await
        .unwrap();
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].started && rows[0].kind == kind && rows[0].key == key,
        "{rows:?}"
    );
}

#[tokio::test]
async fn a_comment_fix_retried_later_ends_with_one_started_row() {
    retried_ends_with_one_started_row(PausedKind::CommentFix, "merge-task-1").await;
}

#[tokio::test]
async fn a_review_fix_retried_later_ends_with_one_started_row() {
    retried_ends_with_one_started_row(PausedKind::ReviewFix, "task-and-fingerprint-hash").await;
}

#[tokio::test]
async fn an_escape_check_retried_later_ends_with_one_started_row() {
    retried_ends_with_one_started_row(PausedKind::EscapeCheck, "escape-1").await;
}

#[tokio::test]
async fn a_crew_started_launch_without_a_trigger_is_unknown_and_never_a_comment_fix() {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 2, true);
    launch(&w, "src/clean/**", true, None).await.unwrap_err();
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, PausedKind::Unknown);
}
