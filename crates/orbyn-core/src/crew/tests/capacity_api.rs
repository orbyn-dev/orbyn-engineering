//! Review capacity as a person sees and sets it (slice 4l, PR 4): the pause switch (off by default), Crew-started tasks
//! stored as Low, and the state, coverage and write calls the routes use. Real `Core`, fake engines.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::{CrewId, CrewMember, CrewToken, ErrorCode};
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::capacity::{CapacitySettings, Priority, Sampling};
use crate::crew::fixtures::merge_record;
use crate::crew::stamp::Stamper;
use crate::crew::start::{Launch, Links};
use crate::crew::tool::stamp_now;

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

/// A launch the way a review fix or a merge-comment fix makes it: written by the core, Crew-started.
async fn crew_launch(
    w: &World,
    path: &str,
    crew_started: bool,
) -> Result<serde_json::Value, wire::CrewError> {
    let lead = w.core.store.thread(&w.lead).unwrap();
    let stamper = Stamper {
        crew_id: CrewId::new(lead.id.clone()).unwrap(),
        engine: CrewToken::new("alpha").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: stamp_now().unwrap(),
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
                trigger: crew_started.then(|| crate::crew::capacity::PausedTrigger {
                    kind: crate::crew::capacity::PausedKind::CommentFix,
                    key: path.to_owned(),
                }),
                ..Links::default()
            },
        })
        .await
}

#[tokio::test]
async fn pausing_is_off_by_default_so_a_crew_started_start_goes_over_any_queue() {
    let w = quiet().await;
    waiting(&w, 3).await;
    let state = w.core.crew_capacity_state(&project(&w)).unwrap();
    assert!(!state.pause_enabled && !state.settings_saved);
    assert_eq!((state.waiting, state.limit, state.paused), (3, 8, None));
    // Limit 1, but pause_enabled stays off: nothing pauses.
    save(&w, 1, false);
    assert!(
        w.core
            .crew_capacity_state(&project(&w))
            .unwrap()
            .paused
            .is_none()
    );
    let started = crew_launch(&w, "src/clean/**", true).await.unwrap();
    let task = started["task"].as_str().unwrap();
    assert_eq!(
        w.core.store.crew_task_capacity(task).unwrap(),
        (Priority::Low, true)
    );
}

#[tokio::test]
async fn with_pausing_on_a_crew_started_start_is_refused_and_stores_nothing_while_a_person_start_goes()
 {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 2, true);
    let state = w.core.crew_capacity_state(&project(&w)).unwrap();
    assert_eq!(
        state.paused,
        Some(wire::PausedStart {
            waiting: 3,
            limit: 2
        })
    );
    let tasks = w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len();
    let refused = crew_launch(&w, "src/clean/**", true).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::NotAuthorized);
    assert_eq!(
        w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len(),
        tasks
    );
    // A person's own start: Medium and person-started, never paused.
    let person = crew_launch(&w, "src/clean/**", false).await.unwrap();
    let task = person["task"].as_str().unwrap();
    assert_eq!(
        w.core.store.crew_task_capacity(task).unwrap(),
        (Priority::Medium, false)
    );
}

#[tokio::test]
async fn a_person_sets_settings_and_a_task_and_a_bad_write_changes_nothing() {
    let w = quiet().await;
    let p = project(&w);
    let saved = w
        .core
        .crew_set_capacity_settings(&wire::CapacitySettingsInput {
            project: p.clone(),
            limit: 5,
            sampling: wire::CapacitySampling::OneInFour,
            pause_enabled: true,
        })
        .unwrap();
    assert!(saved.settings_saved && saved.pause_enabled && saved.limit == 5);
    assert_eq!(
        saved.plan.iter().map(|r| r.sampled).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    let zero = w
        .core
        .crew_set_capacity_settings(&wire::CapacitySettingsInput {
            project: p.clone(),
            limit: 0,
            sampling: wire::CapacitySampling::Off,
            pause_enabled: false,
        });
    assert!(zero.is_err());
    assert_eq!(w.core.crew_capacity_state(&p).unwrap().limit, 5);

    waiting(&w, 1).await;
    let task = w.core.store.crew_merges_of_project(&p).unwrap()[0]
        .task_id
        .clone();
    let answer = w
        .core
        .crew_set_task_capacity(
            &task,
            &wire::CapacityTaskInput {
                priority: Some(wire::CapacityPriority::Low),
                crew_started: None,
            },
        )
        .unwrap();
    assert_eq!(
        (answer.priority, answer.crew_started),
        (wire::CapacityPriority::Low, false)
    );
    let empty = wire::CapacityTaskInput {
        priority: None,
        crew_started: None,
    };
    assert!(w.core.crew_set_task_capacity(&task, &empty).is_err());
    let unknown = wire::CapacityTaskInput {
        priority: Some(wire::CapacityPriority::High),
        crew_started: None,
    };
    assert!(w.core.crew_set_task_capacity("nope", &unknown).is_err());
}

#[tokio::test]
async fn coverage_of_a_project_with_no_provenance_is_not_measured_and_never_invented() {
    let w = quiet().await;
    waiting(&w, 2).await;
    let c = w.core.crew_review_coverage(&project(&w), 0).unwrap();
    assert_eq!(c.began_unix, None);
    assert_eq!((c.merges, c.not_measured_merges, c.agent_lines), (0, 2, 0));
    assert!(c.areas.iter().all(|a| a.agent_lines == 0));
}

#[tokio::test]
async fn with_the_preview_flag_off_every_capacity_call_is_not_found() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let p = project(&w);
    assert!(w.core.crew_capacity_state(&p).is_err());
    assert!(w.core.crew_review_coverage(&p, 0).is_err());
}

#[tokio::test]
async fn a_high_task_is_never_paused_even_marked_crew_started_over_the_limit() {
    let w = quiet().await;
    waiting(&w, 3).await;
    save(&w, 1, true);
    // A task stored High and Crew-started: the check reads its stored pair and lets it go.
    assert!(
        w.core
            .capacity_start_check(&project(&w), Some((Priority::High, true)), None)
            .is_ok()
    );
    // And through the start path: a person's start (the only kind that is not Low) goes over the limit.
    let started = crew_launch(&w, "src/high/**", false).await.unwrap();
    let task = started["task"].as_str().unwrap();
    w.core
        .crew_set_task_capacity(
            task,
            &wire::CapacityTaskInput {
                priority: Some(wire::CapacityPriority::High),
                crew_started: Some(true),
            },
        )
        .unwrap();
    assert_eq!(
        w.core.store.crew_task_capacity(task).unwrap(),
        (Priority::High, true)
    );
    assert_eq!(
        w.core
            .crew_capacity_state(&project(&w))
            .unwrap()
            .paused
            .map(|p| p.limit),
        Some(1),
        "the queue is over, so a Low Crew start would pause"
    );
    w.core
        .capacity_start_check(&project(&w), Some((Priority::High, true)), None)
        .unwrap();
}
