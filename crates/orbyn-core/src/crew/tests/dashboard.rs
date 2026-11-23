//! The Crew dashboard read (W12, row G18): one repository's figures for one window, from a real `Core` with fake engines. Merge
//! rows are written beside real tasks, as the queue would write them. What the read derives from one merge's own records (what it
//! needed, its checks, its cost, who reverted it) is unit-tested in `crew/dashboard.rs`; here it is the window, the project, the
//! weeks, the lists and the refusals.
//!
//! Every task is started on its own path (`src/t<n>/**`) so no worker's lease touches another's, as `autopilot_areas` does it; the
//! area the dashboard counts is the one the merge row carries, which is what the queue writes.

use std::collections::BTreeMap;
use std::sync::Arc;

use orbyn_contracts::{PatternKind, PatternScope, RedactionPatternInput};
use serde_json::json;

use super::autopilot_watch::{NOW, history};
use super::harness::{KEY_MARKER, Turn, World, brief, key, world};
use crate::crew::{ReviewAnswer, WatchOutcome};
use crate::error::CoreError;
use crate::store::MergeRecord;
use crate::testing::run_git;
use orbyn_contracts as wire;

const DAY: i64 = 86_400;

/// 2026-11-16T00:00:00Z, a Monday. 1970-01-01 was a Thursday, so this is not copied from the
/// read's own week arithmetic: it is a literal date's Unix stamp.
const MONDAY: i64 = 1_794_787_200;
/// 2026-11-22T00:00:00Z, the Sunday that ends [`MONDAY`]'s week.
const SUNDAY: i64 = 1_795_305_600;
/// [`MONDAY`] at 00:00 in a +10:00 zone, as Unix seconds.
const LOCAL_MONDAY: i64 = MONDAY - 10 * 3_600;

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
        self.merge_outcome(area, merged, engine, proof, None).await
    }

    /// A merge whose brief carried `outcome`, which is what the dashboard shows as its title.
    async fn merge_outcome(
        &mut self,
        area: &str,
        merged: i64,
        engine: &str,
        proof: serde_json::Value,
        outcome: Option<String>,
    ) -> String {
        self.next += 1;
        let n = self.next;
        let outcome = outcome.unwrap_or_else(|| format!("Task {n}."));
        let (answer, error) = self
            .w
            .start(
                brief(json!({ "files": [{ "path": format!("src/t{n}/**"), "mode": "write" }], "outcome": outcome })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap().to_owned();
        self.add_merge_record(&task, area, merged, engine, proof, n);
        task
    }

    /// The merge row of a task, as the queue writes it.
    fn add_merge_record(
        &self,
        task: &str,
        area: &str,
        merged: i64,
        engine: &str,
        proof: serde_json::Value,
        n: usize,
    ) {
        let record = MergeRecord {
            id: 0,
            task_id: task.to_owned(),
            crew_id: self.w.task(task).crew_id,
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
    }

    /// A stored task whose brief carried `outcome`, without the start path's answer-key refusal:
    /// the read is shown a record that predates or bypassed that check, and must still withhold.
    fn task_with_outcome(&self, outcome: &str, n: usize) -> String {
        let stamper = super::fake::stamper();
        let draft = brief(json!({
            "files": [{ "path": format!("src/key{n}/**"), "mode": "write" }],
            "outcome": outcome,
        }));
        let brief = stamper.brief(&draft).unwrap();
        let key = stamper.answer_key(&key(), &brief.id).unwrap();
        let task = self
            .w
            .core
            .store
            .create_crew_task(&brief, &key, &format!("run-key-{n}"))
            .unwrap();
        self.w
            .core
            .store
            .add_crew_run(&task.id, &self.w.lead, "worker-alpha")
            .unwrap();
        task.id
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

    /// The same read with every bounded list cut at `cap`, as the test hook does it.
    fn read_capped(
        &self,
        days: Option<u32>,
        offset: i32,
        now: i64,
        cap: usize,
    ) -> wire::CrewDashboard {
        self.w
            .core
            .crew_dashboard_with_caps(&self.project, days, offset, now, cap)
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
    // A literal Sunday, 2026-11-22T00:00:00Z. Its Monday is 2026-11-16, and the +10:00 Monday
    // the person sees starts ten hours before UTC's.
    let now = SUNDAY;
    // Two merges in the week three weeks back, one this week: the two weeks between are empty.
    r.merge(
        "src/a/**",
        LOCAL_MONDAY - 21 * DAY + 3_600,
        "alpha",
        json!({}),
    )
    .await;
    r.merge(
        "src/a/**",
        LOCAL_MONDAY - 21 * DAY + 7_200,
        "alpha",
        json!({}),
    )
    .await;
    r.merge("src/a/**", LOCAL_MONDAY + 3_600, "alpha", json!({}))
        .await;

    let answer = r.read(Some(30), 600, now);
    assert_eq!(answer.weeks.len(), 4);
    assert_eq!(answer.weeks[0].start, LOCAL_MONDAY - 21 * DAY);
    assert_eq!(answer.weeks[0].merges, 2);
    assert_eq!(answer.weeks[1].merges, 0);
    assert_eq!(answer.weeks[2].merges, 0);
    assert_eq!(answer.weeks[3].start, LOCAL_MONDAY);
    assert_eq!(answer.weeks[3].merges, 1);
    // Every week starts at midnight in the person's own zone, never at a UTC midnight the person
    // would see mid-afternoon.
    for week in &answer.weeks {
        assert_eq!((week.start + 600 * 60).rem_euclid(DAY), 0, "{week:?}");
    }
}

/// The widest UTC offsets a person can name are accepted, and one minute past them is not.
#[tokio::test]
async fn the_widest_offsets_are_accepted_and_one_minute_more_is_refused() {
    let r = rig().await;
    let now = crate::util::unix_now();
    for offset in [-840, 840] {
        assert!(
            r.w.core
                .crew_dashboard(&r.project, Some(30), offset, now)
                .is_ok(),
            "offset {offset} was refused"
        );
    }
    for offset in [-841, 841] {
        assert!(
            matches!(
                r.w.core.crew_dashboard(&r.project, Some(30), offset, now),
                Err(CoreError::Invalid(_))
            ),
            "offset {offset} was accepted"
        );
    }
}

/// A merge stamped after `now` is still in `merges`, so its week must hold it too: the weekly
/// bars add up to the merges figure.
#[tokio::test]
async fn a_merge_stamped_after_now_is_counted_in_its_own_week() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    r.merge("src/a/**", now + DAY, "alpha", json!({})).await;
    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.merges, 1);
    assert_eq!(
        answer.weeks.iter().map(|week| week.merges).sum::<u32>(),
        answer.merges,
        "the weeks add up to merges"
    );
    assert_eq!(answer.weeks.len(), 1);
    assert_eq!(answer.weeks[0].merges, 1);
    assert_eq!(answer.weeks[0].start.rem_euclid(DAY), 0, "a UTC Monday");
}

/// Two raw engine and area names that the scrub turns into the same text are one row, not two
/// rows the page would draw twice.
#[tokio::test]
async fn two_raw_names_that_scrub_to_the_same_text_are_one_row() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let first = format!("ghp_{}", "a1".repeat(20));
    let second = format!("ghp_{}", "b2".repeat(20));
    r.merge(&format!("src/{first}/**"), now - 2 * DAY, &first, json!({}))
        .await;
    r.merge(&format!("src/{second}/**"), now - DAY, &second, json!({}))
        .await;
    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.engines.len(), 1, "{:?}", answer.engines);
    assert_eq!(answer.engines[0].merges, 2);
    assert_eq!(answer.areas.len(), 1, "{:?}", answer.areas);
    assert_eq!(answer.areas[0].merges, 2);
}

/// A ledger row of another kind that waited on the person is `other`, never `nothing`.
#[tokio::test]
async fn a_ledger_row_that_waited_on_the_person_is_needed_other() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let task = r.merge("src/a/**", now - DAY, "alpha", json!({})).await;
    r.w.core
        .merge_event(&task, "person", &json!({ "why": "a person must decide" }));
    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.recent[0].needed, wire::CrewNeeded::Other);
    assert_eq!(answer.unaided, 0);
}

/// A list over its cap is cut, and `truncated` says so. The test hook lowers every cap so a
/// handful of rows proves the bound the production sizes would need a hundred for.
#[tokio::test]
async fn a_list_over_its_cap_is_cut_and_says_so() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let mut tasks = Vec::new();
    for i in 0..3 {
        tasks.push(
            r.merge(
                &format!("src/t{i}/**"),
                now - (3 - i) * DAY,
                &format!("engine-{i}"),
                json!({}),
            )
            .await,
        );
    }
    for task in &tasks {
        r.revert(task, serde_json::Value::Null);
    }
    let answer = r.read_capped(Some(30), 0, now, 2);
    assert!(answer.truncated);
    assert_eq!(answer.engines.len(), 2);
    assert_eq!(answer.areas.len(), 2);
    assert_eq!(answer.reverts.len(), 2);
    assert_eq!(answer.recent.len(), 2);
}

/// One answered question on the shared lead thread counts for the task it was asked in, and not
/// for every task that thread led afterwards.
#[tokio::test]
async fn one_answered_question_counts_only_for_the_task_it_was_asked_in() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let first = r
        .merge("src/a/**", now - 2 * 3_600, "alpha", json!({}))
        .await;
    let second = r.merge("src/b/**", now - 3_600, "alpha", json!({})).await;
    let first_brief = now - 3 * DAY;
    let second_brief = now - 2 * DAY;
    r.brief_at(&first, first_brief);
    r.brief_at(&second, second_brief);
    // Asked and answered after the first brief, before the second one.
    answered_question(
        &r,
        "q-shared",
        first_brief + 12 * 3_600,
        first_brief + 12 * 3_600,
    );
    let answer = r.read(Some(30), 0, now);
    let needed = |title: &str| {
        answer
            .recent
            .iter()
            .find(|recent| recent.title == title)
            .unwrap()
            .needed
    };
    assert_eq!(needed("Task 1."), wire::CrewNeeded::Answer);
    assert_eq!(needed("Task 2."), wire::CrewNeeded::Nothing);
    assert_eq!(answer.unaided, 1);
}

/// A question answered before the task's brief belongs to whatever came before, never to it.
#[tokio::test]
async fn a_question_answered_before_the_brief_does_not_count() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let task = r.merge("src/a/**", now - 3_600, "alpha", json!({})).await;
    let brief_at = now - DAY;
    r.brief_at(&task, brief_at);
    answered_question(&r, "q-before", brief_at - 2 * 3_600, brief_at - 3_600);
    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.recent[0].needed, wire::CrewNeeded::Nothing);
    assert_eq!(answer.unaided, 1);
}

/// The read must not stop at the store's 500-row question list: an answer past it still counts.
#[tokio::test]
async fn an_answered_question_past_the_five_hundred_row_limit_still_counts() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let task = r.merge("src/a/**", now - 3_600, "alpha", json!({})).await;
    let brief_at = now - 10 * DAY;
    r.brief_at(&task, brief_at);
    {
        let conn = r.w.core.store.reader();
        question_run(&conn, "run-bulk", &r.w.lead, brief_at);
        let mut statement = conn
            .prepare(
                "INSERT INTO question_requests
                     (id, thread_id, run_id, questions, state, answers, decided_by, decided_at, created_at)
                 VALUES (?1, ?2, ?3, '[]', ?4, '{}', 'person', ?5, ?6)",
            )
            .unwrap();
        for i in 0..500 {
            let at = stamp(brief_at + 1 + i);
            statement
                .execute(rusqlite::params![
                    format!("q-dismissed-{i}"),
                    &r.w.lead,
                    "run-bulk",
                    "dismissed",
                    at,
                    at
                ])
                .unwrap();
        }
        let at = stamp(brief_at + 1_000);
        statement
            .execute(rusqlite::params![
                "q-answered",
                &r.w.lead,
                "run-bulk",
                "answered",
                at,
                at
            ])
            .unwrap();
    }
    let answer = r.read(Some(30), 0, now);
    assert_eq!(answer.recent[0].needed, wire::CrewNeeded::Answer);
}

/// A title that quotes the task's answer key comes out withheld, not on the screen.
#[tokio::test]
async fn a_title_withholds_the_tasks_answer_key_text() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    let task = r.task_with_outcome(
        &format!("Ran echo {KEY_MARKER} on the tree and finished."),
        1,
    );
    r.next += 1;
    let n = r.next;
    r.add_merge_record(&task, "src/a/**", now - DAY, "alpha", json!({}), n);
    let answer = r.read(Some(30), 0, now);
    let title = &answer.recent[0].title;
    assert!(!title.contains(KEY_MARKER), "{title}");
    assert!(title.contains("[check]"), "{title}");
}

/// A title that matches the person's own Settings › Secrets pattern comes out masked.
#[tokio::test]
async fn a_title_masks_the_persons_own_secret_pattern() {
    let mut r = rig().await;
    let now = crate::util::unix_now();
    r.w.core
        .save_redaction_pattern(
            "local",
            None,
            &RedactionPatternInput {
                name: "Corp id".into(),
                pattern: "corp_[a-z]{6}".into(),
                kind: PatternKind::Regex,
                applies_to: PatternScope::Anywhere,
                accept_broad: Some(true),
            },
        )
        .unwrap();
    r.merge_outcome(
        "src/a/**",
        now - DAY,
        "alpha",
        json!({}),
        Some("The corp_abcdef build finished.".into()),
    )
    .await;
    let answer = r.read(Some(30), 0, now);
    let title = &answer.recent[0].title;
    assert!(!title.contains("corp_abcdef"), "{title}");
    assert!(title.contains("[hidden: Corp id]"), "{title}");
}

/// A question answered after `created`, for the shared lead thread. The row is written directly:
/// a real question needs an engine run, and the read only sees the stored answer.
fn answered_question(r: &Rig, id: &str, created: i64, decided: i64) {
    let conn = r.w.core.store.reader();
    question_run(&conn, &format!("run-{id}"), &r.w.lead, created);
    conn.execute(
        "INSERT INTO question_requests
             (id, thread_id, run_id, questions, state, answers, decided_by, decided_at, created_at)
         VALUES (?1, ?2, ?3, '[]', 'answered', '{}', 'person', ?4, ?5)",
        rusqlite::params![
            id,
            &r.w.lead,
            format!("run-{id}"),
            stamp(decided),
            stamp(created)
        ],
    )
    .unwrap();
}

/// A finished engine run the question can name: `question_requests.run_id` has a foreign key, and
/// a test question does not need a live engine to have asked it.
fn question_run(conn: &rusqlite::Connection, id: &str, thread: &str, at: i64) {
    conn.execute(
        "INSERT INTO runs (id, thread_id, engine, state, owner_epoch, error, started_at, ended_at)
         VALUES (?1, ?2, 'alpha', 'completed', 'test', NULL, ?3, ?3)",
        rusqlite::params![id, thread, stamp(at)],
    )
    .unwrap();
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

/// The watch recorded a break it was unsure of, then the person reverted the merge: the record
/// says the cause was not certain, so the dashboard says `false` and not `null`.
#[tokio::test]
async fn a_persons_revert_after_an_unsure_break_is_not_certain() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/c.txt", "c\n")],
    ])
    .await;
    let project = h.w.core.store.thread(&h.w.lead).unwrap().project_id;
    // The watch never moves a checked-out branch, so it records a break without certainty.
    run_git(&h.w.repo, &["checkout", "-q", &h.main]);
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    assert!(
        matches!(outcome, WatchOutcome::NeedsPerson { .. }),
        "{outcome:?}"
    );
    run_git(&h.w.repo, &["checkout", "-q", "parking"]);
    // The person reverts through the same `revert_merge` the watch uses, once the branch is free.
    let commit =
        h.w.core
            .crew_answer_review(&h.tasks[2], ReviewAnswer::Revert, &h.target, NOW + 200)
            .await
            .unwrap();
    assert!(commit.is_some());
    let answer =
        h.w.core
            .crew_dashboard(&project, Some(30), 0, NOW + 300)
            .unwrap();
    let revert = answer
        .reverts
        .iter()
        .find(|revert| revert.by == wire::CrewRevertBy::Person)
        .expect("the person reverted the merge");
    assert_eq!(revert.certain, Some(false));
}

/// The watch reverted a break it proved certain, through `revert_found` and `revert_merge`.
#[tokio::test]
async fn the_watchs_own_certain_revert_is_certain() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/c.txt", "c\n")],
    ])
    .await;
    let project = h.w.core.store.thread(&h.w.lead).unwrap().project_id;
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    assert!(
        matches!(outcome, WatchOutcome::Reverted { .. }),
        "{outcome:?}"
    );
    let answer =
        h.w.core
            .crew_dashboard(&project, Some(30), 0, NOW + 200)
            .unwrap();
    let revert = answer
        .reverts
        .iter()
        .find(|revert| revert.by == wire::CrewRevertBy::Auto)
        .expect("the watch reverted the merge");
    assert_eq!(revert.certain, Some(true));
}

/// A person's revert where the watch recorded no break at all leaves the cause unknown.
#[tokio::test]
async fn a_revert_with_nothing_recorded_is_null_certain() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/c.txt", "c\n")],
        &[("src/d.txt", "d\n")],
    ])
    .await;
    let project = h.w.core.store.thread(&h.w.lead).unwrap().project_id;
    let commit =
        h.w.core
            .crew_answer_review(&h.tasks[1], ReviewAnswer::Revert, &h.target, NOW + 100)
            .await
            .unwrap();
    assert!(commit.is_some());
    let answer =
        h.w.core
            .crew_dashboard(&project, Some(30), 0, NOW + 200)
            .unwrap();
    let revert = answer
        .reverts
        .iter()
        .find(|revert| revert.by == wire::CrewRevertBy::Person)
        .expect("the person reverted the merge");
    assert_eq!(revert.certain, None);
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
    let before = dump(&r);
    let answer = r.read(Some(30), 0, now);
    assert_eq!(dump(&r), before, "the read changed no row of any table");
    // The held command is what that merge needed, so it is not one that needed nothing.
    assert_eq!(answer.merges, 1);
    assert_eq!(answer.unaided, 0);
    assert_eq!(answer.recent[0].needed, wire::CrewNeeded::Approval);
}

/// Every row of every table the store holds, before and after the read. Not three counts: a read
/// that wrote to a table it never counted would still be caught.
fn dump(r: &Rig) -> BTreeMap<String, Vec<Vec<String>>> {
    let conn = r.w.core.store.reader();
    let mut tables = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = tables
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut out = BTreeMap::new();
    for table in tables {
        let mut statement = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                let mut cells = Vec::with_capacity(columns);
                for column in 0..columns {
                    cells.push(format!("{:?}", row.get_ref(column)?));
                }
                Ok(cells)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        out.insert(table, rows);
    }
    out
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

/// KC3: the "Right now" row is not a figure over the window. Its waits are the one needs-you field (a dismissed stop
/// leaves it and the window's merges stay put), and its Working count comes from the one working function.
#[tokio::test]
async fn the_live_row_reads_the_one_needs_you_field_and_the_one_working_function() {
    use orbyn_contracts::NeedsYouKind;

    let mut r = rig().await;
    // One merge in the window, whose task is still running (the rig hangs every worker).
    let merged = r
        .merge("src/live", NOW, "alpha", with_usage(None, None, None))
        .await;
    r.w.core
        .store
        .set_merge_needs_person(&merged, Some("The break needs a person."))
        .unwrap();
    // A worker that never ran waits too, and this one can be dismissed.
    let (answer, error) =
        r.w.start(
            brief(json!({
                "engine": { "id": "ghost", "assigned_by": "lead" },
                "files": [{ "path": "src/ghost/**", "mode": "write" }]
            })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let ghost = answer["task"].as_str().unwrap().to_owned();

    let read = |days: Option<u32>| r.read(days, 0, NOW);
    let live = read(Some(30)).live;
    assert_eq!(
        live.needs_you
            .iter()
            .map(|w| (w.kind, w.count))
            .collect::<Vec<_>>(),
        [
            (NeedsYouKind::CouldNotStart, 1),
            (NeedsYouKind::MergeWaiting, 1)
        ],
        "both waits are the field's own items"
    );
    assert_eq!(
        live.working
            .iter()
            .map(|w| (w.why, w.count))
            .collect::<Vec<_>>(),
        [(wire::FlowWorking::Running, 1)],
        "the running worker is Working"
    );
    assert!(!live.truncated);

    // The live row ignores the 30, 60 and 90 day range at the same instant.
    assert_eq!(read(Some(30)).live, read(Some(60)).live);
    assert_eq!(read(Some(60)).live, read(Some(90)).live);

    // A dismissal hides the stop from the live row and moves no window figure.
    let window_merges = read(Some(30)).merges;
    r.w.core
        .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &ghost, "user")
        .unwrap();
    let after = read(Some(30));
    assert_eq!(
        after.live.needs_you,
        vec![wire::CrewDashboardWaiting {
            kind: NeedsYouKind::MergeWaiting,
            count: 1
        }]
    );
    assert_eq!(
        after.merges, window_merges,
        "the window is not the live row"
    );
}
/// KC3 F5: the live row is one repository's. Another project's working task is not in this answer's Working count, and this
/// project's waits are not in the other project's.
#[tokio::test]
async fn the_live_row_counts_only_this_projects_workers_and_waits() {
    use orbyn_contracts::NeedsYouKind;

    let mut r = rig().await;
    // This project: one running worker (the rig hangs every worker) and one could-not-start wait.
    r.merge("src/live", NOW, "alpha", with_usage(None, None, None))
        .await;
    let (answer, error) =
        r.w.start(
            brief(json!({
                "engine": { "id": "ghost", "assigned_by": "lead" },
                "files": [{ "path": "src/ghost/**", "mode": "write" }]
            })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");

    // Another project with a crew task of its own, running.
    let folder = r.w.repo.parent().unwrap().join("live-elsewhere");
    std::fs::create_dir_all(&folder).unwrap();
    let other =
        r.w.core
            .store
            .add_project(crate::store::NewProject {
                path: folder.to_str().unwrap(),
                name: "live-elsewhere",
                is_git: false,
                managed: false,
                scratch: false,
            })
            .unwrap();
    let thread =
        r.w.core
            .store
            .create_thread(crate::store::NewThread {
                project_id: other.id.clone(),
                title: "Elsewhere".into(),
                engine: "alpha".into(),
                model: None,
                cwd: folder.to_str().unwrap().to_owned(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
    let mut stamper = super::fake::stamper();
    stamper.crew_id = orbyn_contracts::CrewId::new(thread.id.clone()).unwrap();
    let other_brief = stamper
        .brief(&super::fake::draft(json!({ "outcome": "Elsewhere." })))
        .unwrap();
    let other_key = stamper.answer_key(&key(), &other_brief.id).unwrap();
    let other_task =
        r.w.core
            .store
            .create_crew_task(&other_brief, &other_key, "run-elsewhere")
            .unwrap();
    r.w.core
        .store
        .add_crew_run(other_task.id.as_str(), &thread.id, "worker-elsewhere")
        .unwrap();
    r.w.core
        .store
        .move_crew_task(other_task.id.as_str(), "routed", None)
        .unwrap();
    r.w.core
        .store
        .move_crew_task(other_task.id.as_str(), "running", None)
        .unwrap();

    let live = r.read(Some(30), 0, NOW).live;
    assert_eq!(
        live.working,
        vec![wire::CrewDashboardWorking {
            why: wire::FlowWorking::Running,
            count: 1
        }],
        "only this project's running worker"
    );
    assert_eq!(
        live.needs_you,
        vec![wire::CrewDashboardWaiting {
            kind: NeedsYouKind::CouldNotStart,
            count: 1
        }]
    );
    // The other project sees its own worker and none of this project's waits.
    let other_live =
        r.w.core
            .crew_dashboard(&other.id, Some(30), 0, NOW)
            .unwrap()
            .live;
    assert_eq!(
        other_live.working,
        vec![wire::CrewDashboardWorking {
            why: wire::FlowWorking::Running,
            count: 1
        }]
    );
    assert!(other_live.needs_you.is_empty(), "{other_live:?}");
}
/// KC3 F5: a passed task whose latest fix has not finished is Working (Fix) in the live row, from the one function, and stops
/// reading Working when the fix ends. The truth table answers for the surface too, not only for the pure function.
#[tokio::test]
async fn a_fix_in_flight_reads_working_in_the_live_row() {
    let r = rig().await;
    // A task of this project that already passed, and a fix of a review finding that has not started.
    let parent = r.task_with_outcome("A fix waits.", 1);
    let stamper = super::fake::stamper();
    let fix_brief = stamper
        .brief(&super::fake::draft(
            json!({ "outcome": "Fix the finding." }),
        ))
        .unwrap();
    let fix_key = stamper.answer_key(&key(), &fix_brief.id).unwrap();
    r.w.core
        .store
        .create_crew_task(&fix_brief, &fix_key, "run-fix-1")
        .unwrap();
    let finding = format!("sha256:{}", "f".repeat(64));
    {
        let conn = r.w.core.store.reader();
        conn.execute(
            "UPDATE crew_tasks SET state='passed' WHERE id = ?1",
            [parent.as_str()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO crew_findings (id, task_id, subject_id, lane, round, fingerprint, body)
             VALUES (?1, ?2, ?2, 'correctness', 1, 'fp', '{}')",
            rusqlite::params![finding, parent],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO crew_fixes (finding_id, subject_id, fix_task_id, fingerprint, created_at)
             VALUES (?1, ?2, ?3, 'fp', '2026-11-12T09:00:00Z')",
            rusqlite::params![finding, parent, fix_brief.id.as_str()],
        )
        .unwrap();
    }
    assert_eq!(
        r.read(Some(30), 0, NOW).live.working,
        vec![wire::CrewDashboardWorking {
            why: wire::FlowWorking::Fix,
            count: 1
        }]
    );

    // The fix ends: the same read counts no worker.
    r.w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_tasks SET state='passed' WHERE id = ?1",
            [fix_brief.id.as_str()],
        )
        .unwrap();
    assert!(r.read(Some(30), 0, NOW).live.working.is_empty());
}
