//! The Crew dashboard read (W12, row G18): one repository's figures for one window, from a real `Core` with fake engines. Merge
//! rows are written beside real tasks, as the queue would write them. What the read derives from one merge's own records (what it
//! needed, its checks, its cost, who reverted it) is unit-tested in `crew/dashboard.rs`; here it is the window, the project, the
//! weeks, the lists and the refusals.
//!
//! Every task is started on its own path (`src/t<n>/**`) so no worker's lease touches another's, as `autopilot_areas` does it; the
//! area the dashboard counts is the one the merge row carries, which is what the queue writes.

use std::sync::Arc;

use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::error::CoreError;
use crate::store::MergeRecord;
use orbyn_contracts as wire;

const DAY: i64 = 86_400;

struct Rig {
    w: World,
    project: String,
    next: usize,
}

async fn rig() -> Rig {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    Rig {
        w,
        project,
        next: 0,
    }
}

impl Rig {
    /// A merge in `area` at `merged`, made by `engine`. `proof` is the merge's stored proof.
    async fn merge(
        &mut self,
        area: &str,
        merged: i64,
        engine: &str,
        proof: serde_json::Value,
    ) -> String {
        self.next += 1;
        let n = self.next;
        let (answer, error) = self
            .w
            .start(
                brief(json!({ "files": [{ "path": format!("src/t{n}/**"), "mode": "write" }], "outcome": format!("Task {n}.") })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap().to_owned();
        let record = MergeRecord {
            id: 0,
            task_id: task.clone(),
            crew_id: self.w.task(&task).crew_id,
            repo: self.w.repo.to_string_lossy().into_owned(),
            branch: "fake-main".into(),
            before_commit: format!("{:040x}", n * 2),
            merge_commit: format!("{:040x}", n * 2 + 1),
            candidate: format!("{:040x}", n * 2 + 1),
            risk: "medium".into(),
            reasons: json!(["ordinary feature work"]),
            areas: vec![area.to_owned()],
            engine: engine.to_owned(),
            mode: "full".into(),
            files: 1,
            lines: 1,
            proof,
            not_verified: json!([]),
            merged_unix: merged,
            due_unix: None,
            review_state: "waiting".into(),
            comment: None,
            watch: "unknown".into(),
            reverted_commit: None,
            needs_person: None,
        };
        self.w.core.store.add_crew_merge(&record).unwrap();
        task
    }

    /// The task's brief was accepted at `created`, so its cycle time is known.
    fn brief_at(&self, task: &str, created: i64) {
        let conn = self.w.core.store.reader();
        conn.execute(
            "UPDATE crew_tasks SET created_at = ?2 WHERE id = ?1",
            rusqlite::params![task, stamp(created)],
        )
        .unwrap();
    }

    /// The merge is reverted, as `revert_merge` records it: the row, then its ledger row.
    fn revert(&self, task: &str, body: serde_json::Value) {
        let conn = self.w.core.store.reader();
        conn.execute(
            "UPDATE crew_merges SET review_state='reverted', reverted_commit=?2 WHERE task_id=?1",
            rusqlite::params![task, format!("{:040x}", 0xfeed_u64)],
        )
        .unwrap();
        drop(conn);
        self.w.core.merge_event(
            task,
            "reverted",
            &json!({ "revert": format!("{:040x}", 0xfeed_u64), "merge": format!("{:040x}", 0xbeef_u64), "by": "request", "verified": true }),
        );
        if !body.is_null() {
            self.w.core.merge_event(task, "person_told", &body);
        }
    }

    fn read(&self, days: Option<u32>, offset: i32, now: i64) -> wire::CrewDashboard {
        self.w
            .core
            .crew_dashboard(&self.project, days, offset, now)
            .unwrap()
    }
}

/// RFC 3339 for a Unix time, which is how the store keeps a task's stamps.
fn stamp(unix: i64) -> String {
    let at = time::OffsetDateTime::from_unix_timestamp(unix).unwrap();
    at.format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

#[tokio::test]
async fn the_read_counts_one_projects_merges_inside_the_window_only() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    r.merge("src/a/**", now - 2 * DAY, "alpha", json!({})).await;
    r.merge("src/a/**", now - 40 * DAY, "alpha", json!({}))
        .await; // inside 60, outside 30
    r.merge("src/a/**", now - 100 * DAY, "alpha", json!({}))
        .await; // outside every window

    assert_eq!(r.read(Some(30), 0, now).merges, 1);
    assert_eq!(r.read(Some(60), 0, now).merges, 2);
    assert_eq!(r.read(Some(90), 0, now).merges, 2);
    assert_eq!(r.read(None, 0, now).days, 60, "60 days when none is asked");
    assert_eq!(r.read(None, 0, now).merges, 2);
}

#[tokio::test]
async fn another_projects_merges_are_not_in_this_ones_answer() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    r.merge("src/a/**", now - DAY, "alpha", json!({})).await;
    let folder = r.w.repo.parent().unwrap().join("elsewhere");
    std::fs::create_dir_all(&folder).unwrap();
    let other =
        r.w.core
            .store
            .add_project(crate::store::NewProject {
                path: folder.to_str().unwrap(),
                name: "elsewhere",
                is_git: false,
                managed: false,
                scratch: false,
            })
            .unwrap();
    let empty = r.w.core.crew_dashboard(&other.id, None, 0, now).unwrap();
    assert_eq!(empty.merges, 0, "a project with no Crew use answers zero");
    assert_eq!(empty.unaided, 0);
    assert!(empty.cycle.is_none(), "nothing measured the cycle");
    assert!(empty.weeks.is_empty());
    assert!(empty.engines.is_empty() && empty.areas.is_empty());
    assert!(empty.reverts.is_empty() && empty.recent.is_empty());
    assert!(!empty.truncated);
    assert_eq!(empty.project, other.id);
    // The first project still has its own merge: one answer never mixes two repositories.
    assert_eq!(r.read(None, 0, now).merges, 1);
}

#[tokio::test]
async fn a_rate_and_a_median_state_the_sample_they_need() {
    let r = rig().await;
    let answer = r.read(Some(30), 0, crate::util::unix_now());
    assert_eq!(answer.min_rate_sample, 10);
    assert_eq!(answer.min_median_sample, 5);
    assert!(answer.as_of > 0);
}

#[tokio::test]
async fn the_weeks_are_the_persons_local_mondays_and_a_quiet_week_is_a_zero_bar() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let monday = local_monday(now, 600);
    // Two merges in the week three weeks back, one this week: the two weeks between are empty.
    r.merge("src/a/**", monday - 21 * DAY + 3_600, "alpha", json!({}))
        .await;
    r.merge("src/a/**", monday - 21 * DAY + 7_200, "alpha", json!({}))
        .await;
    r.merge("src/a/**", monday + 3_600, "alpha", json!({}))
        .await;

    let answer = r.read(Some(30), 600, now);
    assert_eq!(answer.weeks.len(), 4);
    assert_eq!(answer.weeks[0].start, monday - 21 * DAY);
    assert_eq!(answer.weeks[0].merges, 2);
    assert_eq!(answer.weeks[1].merges, 0);
    assert_eq!(answer.weeks[2].merges, 0);
    assert_eq!(answer.weeks[3].start, monday);
    assert_eq!(answer.weeks[3].merges, 1);
    // Every week starts at midnight in the person's own zone, never at a UTC midnight the person
    // would see mid-afternoon.
    for week in &answer.weeks {
        assert_eq!((week.start + 600 * 60).rem_euclid(DAY), 0, "{week:?}");
    }
}

#[tokio::test]
async fn an_engine_and_an_area_carry_their_own_record_and_a_cost_in_one_unit() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    r.merge(
        "src/a/**",
        now - 3 * DAY,
        "alpha",
        with_usage(Some(100), Some(50), None),
    )
    .await;
    r.merge(
        "src/a/**",
        now - 2 * DAY,
        "alpha",
        with_usage(Some(10), Some(5), None),
    )
    .await;
    r.merge(
        "src/b/**",
        now - DAY,
        "bravo",
        with_usage(None, None, Some(2_500)),
    )
    .await;
    // One engine that reported two units is never summed across them.
    r.merge(
        "src/b/**",
        now,
        "charlie",
        with_usage(Some(5), Some(0), None),
    )
    .await;
    r.merge("src/b/**", now, "charlie", with_usage(None, None, Some(9)))
        .await;

    let answer = r.read(Some(30), 0, now);
    let alpha = answer.engines.iter().find(|e| e.name == "alpha").unwrap();
    assert_eq!(alpha.merges, 2);
    assert_eq!(
        alpha.cost.as_ref().map(|c| (c.unit.as_str(), c.amount)),
        Some(("tokens", 165))
    );
    let bravo = answer.engines.iter().find(|e| e.name == "bravo").unwrap();
    assert_eq!(
        bravo.cost.as_ref().map(|c| (c.unit.as_str(), c.amount)),
        Some(("micros", 2_500))
    );
    let charlie = answer.engines.iter().find(|e| e.name == "charlie").unwrap();
    assert_eq!(charlie.merges, 2);
    assert!(charlie.cost.is_none(), "two units are never added");
    // Most merges first.
    assert_eq!(answer.engines[0].name, "alpha");

    let a = answer.areas.iter().find(|a| a.name == "src/a/**").unwrap();
    assert_eq!(a.merges, 2);
    let b = answer.areas.iter().find(|b| b.name == "src/b/**").unwrap();
    assert_eq!(b.merges, 3);
    assert_eq!(answer.areas[0].name, "src/b/**");
}

#[tokio::test]
async fn an_engine_with_no_reported_cost_says_nothing_rather_than_zero() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    r.merge("src/a/**", now - DAY, "alpha", json!({})).await;
    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.engines[0].merges, 1);
    assert!(answer.engines[0].cost.is_none());
}

#[tokio::test]
async fn a_revert_says_who_reverted_it_and_the_list_is_newest_first() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let by_person = r.merge("src/a/**", now - 3 * DAY, "alpha", json!({})).await;
    let by_auto = r.merge("src/a/**", now - 2 * DAY, "alpha", json!({})).await;
    r.revert(&by_person, serde_json::Value::Null);
    r.revert(&by_auto, json!({ "about": "reverted" }));

    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.reverted, 2);
    assert_eq!(answer.reverted_auto, 1);
    assert_eq!(answer.reverted_by_person, 1);
    assert_eq!(
        answer.reverted_auto + answer.reverted_by_person,
        answer.reverted
    );
    assert_eq!(answer.reverts.len(), 2);
    assert!(
        answer
            .reverts
            .windows(2)
            .all(|pair| pair[0].at >= pair[1].at),
        "newest first"
    );
    let auto_row = answer
        .reverts
        .iter()
        .find(|revert| revert.by == wire::CrewRevertBy::Auto)
        .expect("the watch's own revert is in the list");
    assert_eq!(auto_row.certain, Some(true), "a certain break was proved");
    let person_row = answer
        .reverts
        .iter()
        .find(|revert| revert.by == wire::CrewRevertBy::Person)
        .expect("the person's revert is in the list");
    assert_eq!(person_row.certain, None, "nothing recorded its cause");
    assert!(answer.reverts.iter().all(|revert| !revert.title.is_empty()));
}

#[tokio::test]
async fn recent_lists_the_newest_merges_with_their_checks() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let passed = json!({ "task_checks": [{ "check": "chk-a", "outcome": "pass" }] });
    let failed = json!({ "task_checks": [{ "check": "chk-a", "outcome": "fail" }] });
    r.merge("src/a/**", now - 3 * DAY, "alpha", passed).await;
    r.merge("src/a/**", now - 2 * DAY, "alpha", failed).await;
    r.merge("src/a/**", now - DAY, "alpha", json!({})).await;

    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.recent.len(), 3);
    assert_eq!(
        answer.recent[0].checks,
        wire::CrewChecks::None_,
        "the newest"
    );
    assert_eq!(answer.recent[1].checks, wire::CrewChecks::Failed);
    assert_eq!(answer.recent[2].checks, wire::CrewChecks::Passed);
    assert!(
        answer
            .recent
            .iter()
            .all(|recent| recent.needed == wire::CrewNeeded::Nothing),
        "nothing waited on the person in these tasks"
    );
    assert!(!answer.truncated);
}

#[tokio::test]
async fn the_cycle_is_the_time_from_the_brief_to_the_merge() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let first = r.merge("src/a/**", now - 4 * DAY, "alpha", json!({})).await;
    let second = r.merge("src/a/**", now - 3 * DAY, "alpha", json!({})).await;
    let third = r.merge("src/a/**", now - 2 * DAY, "alpha", json!({})).await;
    r.brief_at(&first, now - 4 * DAY - 60);
    r.brief_at(&second, now - 3 * DAY - 120);
    r.brief_at(&third, now - 2 * DAY - 180);

    let cycle = r.read(Some(30), 0, now).cycle.expect("three merges met");
    assert_eq!(cycle.fastest_s, Some(60));
    assert_eq!(cycle.slowest_s, Some(180));
    assert_eq!(cycle.median_s, Some(120));
    assert_eq!(cycle.p90_s, Some(180));
}

#[tokio::test]
async fn a_merge_with_no_readable_brief_moment_leaves_its_cycle_out() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let task = r.merge("src/a/**", now - DAY, "alpha", json!({})).await;
    // The brief was accepted after the merge: nothing measured it, and it is never a negative.
    r.brief_at(&task, now);
    assert!(r.read(Some(30), 0, now).cycle.is_none());
}

#[tokio::test]
async fn the_read_writes_nothing() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let task = r.merge("src/a/**", now - DAY, "alpha", json!({})).await;
    r.w.core
        .merge_event(&task, "held", &json!({ "command": "cargo test" }));
    let before = rows(&r);
    let answer = r.read(Some(30), 0, now);
    assert_eq!(rows(&r), before, "the read changed no row");
    // The held command is what that merge needed, so it is not one that needed nothing.
    assert_eq!(answer.merges, 1);
    assert_eq!(answer.unaided, 0);
    assert_eq!(answer.recent[0].needed, wire::CrewNeeded::Approval);
}

/// The counts the read could write if it were not a read.
fn rows(r: &Rig) -> (i64, i64, i64) {
    let conn = r.w.core.store.reader();
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    };
    (
        count("crew_merges"),
        count("crew_autopilot_events"),
        count("crew_tasks"),
    )
}

#[tokio::test]
async fn a_window_that_is_not_thirty_sixty_or_ninety_is_refused() {
    let r = rig().await;
    let now = crate::util::unix_now();
    for days in [Some(0), Some(45), Some(120)] {
        let refused = r.w.core.crew_dashboard(&r.project, days, 0, now);
        assert!(
            matches!(refused, Err(CoreError::Invalid(_))),
            "days {days:?} was accepted"
        );
    }
    // A UTC offset wider than any zone is refused too.
    assert!(matches!(
        r.w.core.crew_dashboard(&r.project, None, 900, now),
        Err(CoreError::Invalid(_))
    ));
}

#[tokio::test]
async fn an_unknown_project_is_not_found() {
    let r = rig().await;
    let refused =
        r.w.core
            .crew_dashboard("project-that-is-not-here", None, 0, crate::util::unix_now());
    assert!(
        matches!(refused, Err(CoreError::NotFound(_))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn the_read_is_not_found_while_crew_preview_is_off() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let refused = w
        .core
        .crew_dashboard(&project, None, 0, crate::util::unix_now());
    assert!(
        matches!(refused, Err(CoreError::NotFound(_))),
        "{refused:?}"
    );
}

/// The Monday of the week `unix` falls in, in the person's own zone.
fn local_monday(unix: i64, offset_minutes: i32) -> i64 {
    let local = unix + i64::from(offset_minutes) * 60;
    let weekday = (local.div_euclid(DAY) + 3).rem_euclid(7);
    (local.div_euclid(DAY) - weekday) * DAY - i64::from(offset_minutes) * 60
}

/// A proof as `record_merge` writes it, with the cost one engine reported.
fn with_usage(
    tokens_in: Option<u64>,
    tokens_out: Option<u64>,
    cost_micros: Option<u64>,
) -> serde_json::Value {
    let mut cost = serde_json::Map::new();
    if let Some(tokens_in) = tokens_in {
        cost.insert("tokens_in".into(), json!(tokens_in));
    }
    if let Some(tokens_out) = tokens_out {
        cost.insert("tokens_out".into(), json!(tokens_out));
    }
    if let Some(micros) = cost_micros {
        cost.insert("cost_micros".into(), json!(micros));
    }
    json!({
        "task_checks": [{ "check": "chk-a", "outcome": "pass" }],
        "cost": serde_json::Value::Object(cost),
    })
}
