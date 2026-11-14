//! Autonomy by area (slice 4j): the circuit breaker's rule, its writes and its reads, on a real `Core` with fake
//! engines. Merge rows are written beside real tasks as the queue would write them; the two revert paths use real git in a temporary
//! repository. Nothing is pushed. What the merge gate does with an area's mode is in `autopilot_enforce`.

use std::sync::Arc;

use serde_json::json;

use super::autopilot_support::events;
use super::autopilot_watch::{NOW, history};
use super::harness::{Turn, World, bind, brief, key, world};
use super::review_support::commit_all;
use crate::crew::{Mode, ModeScope, Page, ReviewAnswer, WatchOutcome};
use crate::store::{AreaMode, MergeRecord};
use crate::testing::run_git;
use orbyn_contracts as wire;

pub(super) const DAY: i64 = 86_400;

pub(super) struct Rig {
    pub(super) w: World,
    pub(super) project: String,
    next: usize,
}

pub(super) async fn rig() -> Rig {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    Rig {
        w,
        project,
        next: 0,
    }
}

fn record_of(w: &World, task: &str, areas: &[&str], at: i64, n: usize) -> MergeRecord {
    MergeRecord {
        id: 0,
        task_id: task.to_owned(),
        crew_id: w.task(task).crew_id,
        repo: w.repo.to_string_lossy().into_owned(),
        branch: "fake-main".into(),
        before_commit: format!("{:040x}", n * 2),
        merge_commit: format!("{:040x}", n * 2 + 1),
        candidate: format!("{:040x}", n * 2 + 1),
        risk: "medium".into(),
        reasons: json!(["ordinary feature work"]),
        areas: areas.iter().map(|a| (*a).to_owned()).collect(),
        engine: "alpha".into(),
        mode: "full".into(),
        files: 1,
        lines: 1,
        proof: json!({}),
        not_verified: json!([]),
        merged_unix: at,
        due_unix: None,
        review_state: "waiting".into(),
        comment: None,
        watch: "unknown".into(),
        reverted_commit: None,
        needs_person: None,
    }
}

impl Rig {
    /// A merge of the project's lead, in `areas`, made at `at`; returns its task.
    pub(super) async fn merge(&mut self, areas: &[&str], at: i64) -> String {
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
        self.w
            .core
            .store
            .add_crew_merge(&record_of(&self.w, &task, areas, at, n))
            .unwrap();
        task
    }

    /// A merge as `record_merge` records it: the row, then the breaker's look at its areas. Returns its task.
    pub(super) async fn merge_recorded(&mut self, areas: &[&str], at: i64) -> String {
        let task = self.merge(areas, at).await;
        let stored = self.w.core.store.crew_merge(&task).unwrap().unwrap();
        self.w.core.area_breaker_after_merge(&stored, at).unwrap();
        task
    }

    /// The merge is reverted at `at`, as `revert_merge` records it: the row first, then the breaker. Returns the revert commit.
    pub(super) fn revert(&self, task: &str, at: i64) -> String {
        let commit = format!(
            "{:040x}",
            0xfeed_0000_u64 + self.w.core.store.crew_merge(task).unwrap().unwrap().id as u64
        );
        self.w.core.store.set_merge_reverted(task, &commit).unwrap();
        let record = self.w.core.store.crew_merge(task).unwrap().unwrap();
        self.w
            .core
            .area_breaker_after_revert(&record, &commit, at)
            .unwrap();
        commit
    }

    pub(super) fn mode(&self, area: &str) -> Option<AreaMode> {
        self.w.core.store.area_mode(&self.project, area).unwrap()
    }

    pub(super) fn kinds(&self, area: &str) -> Vec<String> {
        let mut kinds: Vec<String> = self
            .w
            .core
            .store
            .area_events(&self.project, area, 50)
            .unwrap()
            .into_iter()
            .map(|e| e.kind)
            .collect();
        kinds.reverse();
        kinds
    }

    pub(super) fn view(&self, at: i64) -> wire::AutopilotAreas {
        self.w
            .core
            .autopilot_areas(&self.project, Page::new(Some(200), None).unwrap(), at)
            .unwrap()
    }

    pub(super) fn row(&self, area: &str, at: i64) -> wire::AutopilotAreaRow {
        self.view(at)
            .rows
            .into_iter()
            .find(|r| r.area == area)
            .unwrap_or_else(|| panic!("no row for {area}"))
    }

    /// Five merges in `src/a` a day apart before `NOW`, the last two reverted at `NOW`: the second revert trips the breaker.
    pub(super) async fn tripped(&mut self) -> (Vec<String>, String) {
        let mut tasks = Vec::new();
        for i in 0..5 {
            tasks.push(self.merge(&["src/a"], NOW - (5 - i) * DAY).await);
        }
        self.revert(&tasks[0], NOW);
        let commit = self.revert(&tasks[1], NOW);
        (tasks, commit)
    }
}

#[tokio::test]
async fn four_merges_with_two_reverts_do_not_trip_and_five_do() {
    let mut four = rig().await;
    let tasks = [
        four.merge(&["src/a"], NOW - 4 * DAY).await,
        four.merge(&["src/a"], NOW - 3 * DAY).await,
        four.merge(&["src/a"], NOW - 2 * DAY).await,
        four.merge(&["src/a"], NOW - DAY).await,
    ];
    four.revert(&tasks[0], NOW);
    four.revert(&tasks[1], NOW);
    assert!(four.mode("src/a").is_none(), "4 merges is not enough data");
    assert!(four.kinds("src/a").is_empty());
    let row = four.row("src/a", NOW);
    assert_eq!(row.state, wire::AutopilotAreaState::NotEnoughData);
    assert_eq!((row.record.merges, row.record.reverts), (4, 2));
    assert!(
        row.line.starts_with("Not enough data: 4 of 5"),
        "{}",
        row.line
    );

    let mut five = rig().await;
    let (tasks, commit) = five.tripped().await;
    let mode = five.mode("src/a").expect("the area is lowered");
    assert_eq!(mode.state, "demoted");
    assert_eq!(
        (mode.tripped_at, mode.recovery_since),
        (Some(NOW), Some(NOW))
    );
    assert_eq!(mode.trip_reason.as_ref().unwrap()["reverts"], 2);
    let trail = five
        .w
        .core
        .store
        .area_events(&five.project, "src/a", 10)
        .unwrap();
    assert_eq!(trail.len(), 1, "the first revert did not trip it");
    let event = &trail[0];
    assert_eq!(
        (
            event.kind.as_str(),
            event.decided_by.as_str(),
            event.cause.as_str()
        ),
        ("demoted", "breaker", commit.as_str())
    );
    assert_eq!(
        (event.from_mode.as_deref(), event.to_mode.as_deref()),
        (Some("full"), Some("on"))
    );
    assert_eq!(
        event.record,
        json!({ "merges": 5, "reverts": 2, "window_days": 14, "escapes": null })
    );
    // Flow shows it on the task whose revert tripped it, and only there.
    assert_eq!(events(&five.w, &tasks[1], "area_demoted").len(), 1);
    assert!(events(&five.w, &tasks[0], "area_demoted").is_empty());
    // The mode in force is still what the person chose (the ceiling); the merge gate lowers it per area, and the view says so.
    let crew = five.w.task(&tasks[1]).crew_id;
    assert_eq!(
        five.w.core.crew_autopilot_mode(&five.project, &crew),
        Mode::Full
    );
    let view = five.view(NOW);
    assert!(view.enforced);
    assert_eq!(view.ceiling, wire::AutopilotMode::Full);
    let row = five.row("src/a", NOW);
    assert_eq!(row.state, wire::AutopilotAreaState::Demoted);
    assert_eq!(row.now, wire::AutopilotMode::On);
    assert_eq!(row.since, Some(NOW));
    assert_eq!(row.record.escapes, None);
    assert_eq!(row.recovery.as_ref().unwrap().merges_needed, 5);
}

#[tokio::test]
async fn a_ceiling_of_on_or_off_never_trips_and_the_record_is_still_shown() {
    for ceiling in [Mode::On, Mode::Off] {
        let mut r = rig().await;
        r.w.core
            .set_crew_autopilot_mode(&ModeScope::Repo(r.project.clone()), ceiling)
            .unwrap();
        r.tripped().await;
        assert!(r.mode("src/a").is_none(), "{ceiling:?} is never changed");
        assert!(r.kinds("src/a").is_empty());
        let row = r.row("src/a", NOW);
        assert_eq!(row.state, wire::AutopilotAreaState::CeilingLower);
        assert_eq!((row.record.merges, row.record.reverts), (5, 2));
        assert_eq!(
            row.now.to_owned(),
            if ceiling == Mode::On {
                wire::AutopilotMode::On
            } else {
                wire::AutopilotMode::Off
            }
        );
    }
}

#[tokio::test]
async fn the_way_back_needs_both_counts_and_a_new_revert_starts_it_again() {
    let mut r = rig().await;
    let (tasks, _) = r.tripped().await;
    let any = tasks[0].clone();
    // 4 merges since, 14 days on: not enough merges. A 5th merge, 13 days on: not enough days.
    let mut since = Vec::new();
    for d in 1..=4 {
        since.push(r.merge(&["src/a"], NOW + d * DAY).await);
    }
    r.w.core.area_restore_check(&any, NOW + 14 * DAY).unwrap();
    assert_eq!(r.mode("src/a").unwrap().state, "demoted", "4 merges since");
    since.push(r.merge(&["src/a"], NOW + 5 * DAY).await);
    r.w.core
        .area_restore_check(&any, NOW + 14 * DAY - 1)
        .unwrap();
    assert_eq!(r.mode("src/a").unwrap().state, "demoted", "13 days");
    // Both met: the view says so, and no read starts the return.
    let row = r.row("src/a", NOW + 14 * DAY);
    assert_eq!(row.state, wire::AutopilotAreaState::CriteriaMet);
    assert_eq!(row.line, "Criteria met; returns at the next merge.");
    assert_eq!(
        r.mode("src/a").unwrap().state,
        "demoted",
        "a read never writes"
    );
    r.w.core.area_restore_check(&any, NOW + 14 * DAY).unwrap();
    let back = r.mode("src/a").unwrap();
    assert_eq!(
        (back.state.as_str(), back.recovery_since, back.tripped_at),
        ("follows", None, None)
    );
    assert_eq!(r.kinds("src/a"), ["demoted", "restored"]);
    let restored = r.w.core.store.area_events(&r.project, "src/a", 1).unwrap();
    assert_eq!(restored[0].cause, format!("restore:{NOW}"));
    r.w.core.area_restore_check(&any, NOW + 15 * DAY).unwrap();
    assert_eq!(r.kinds("src/a"), ["demoted", "restored"], "it returns once");

    // A revert while lowered starts the clock again.
    let mut r = rig().await;
    let (tasks, _) = r.tripped().await;
    let any = tasks[0].clone();
    let mut since = Vec::new();
    for d in 1..=5 {
        since.push(r.merge(&["src/a"], NOW + d * DAY).await);
    }
    r.revert(&since[4], NOW + 6 * DAY);
    assert_eq!(r.kinds("src/a"), ["demoted", "recovery_reset"]);
    assert_eq!(r.mode("src/a").unwrap().recovery_since, Some(NOW + 6 * DAY));
    r.w.core.area_restore_check(&any, NOW + 14 * DAY).unwrap();
    assert_eq!(
        r.mode("src/a").unwrap().state,
        "demoted",
        "the old counts are gone"
    );
    for d in 7..=11 {
        r.merge(&["src/a"], NOW + d * DAY).await;
    }
    r.w.core
        .area_restore_check(&any, NOW + 20 * DAY - 1)
        .unwrap();
    assert_eq!(
        r.mode("src/a").unwrap().state,
        "demoted",
        "14 days from the revert"
    );
    r.w.core.area_restore_check(&any, NOW + 20 * DAY).unwrap();
    assert_eq!(r.mode("src/a").unwrap().state, "follows");
    assert_eq!(r.kinds("src/a"), ["demoted", "recovery_reset", "restored"]);
}

#[tokio::test]
async fn the_same_revert_twice_records_one_event() {
    let mut r = rig().await;
    let (tasks, commit) = r.tripped().await;
    let again = r.w.core.store.crew_merge(&tasks[1]).unwrap().unwrap();
    let before = r.mode("src/a").unwrap();
    r.w.core
        .area_breaker_after_revert(&again, &commit, NOW + 100)
        .unwrap();
    assert_eq!(r.kinds("src/a"), ["demoted"]);
    assert_eq!(r.mode("src/a").unwrap(), before, "nothing moved");
    assert_eq!(events(&r.w, &tasks[1], "area_demoted").len(), 1);
    // A reset is recorded once too.
    let later = r.merge(&["src/a"], NOW + DAY).await;
    let reset = r.revert(&later, NOW + 2 * DAY);
    let record = r.w.core.store.crew_merge(&later).unwrap().unwrap();
    r.w.core
        .area_breaker_after_revert(&record, &reset, NOW + 3 * DAY)
        .unwrap();
    assert_eq!(r.kinds("src/a"), ["demoted", "recovery_reset"]);
    assert_eq!(r.mode("src/a").unwrap().recovery_since, Some(NOW + 2 * DAY));
}

#[tokio::test]
async fn the_window_keeps_13_and_14_days_and_drops_15() {
    let mut r = rig().await;
    for age in [13, 14, 14, 14, 15] {
        r.merge(&["src/a"], NOW - age * DAY).await;
    }
    for age in [13, 13, 14, 14, 14] {
        r.merge(&["src/b"], NOW - age * DAY).await;
    }
    let a = r.row("src/a", NOW);
    assert_eq!(a.state, wire::AutopilotAreaState::NotEnoughData);
    assert_eq!(a.record.merges, 4, "the 15-day-old merge is out");
    let b = r.row("src/b", NOW);
    assert_eq!(b.state, wire::AutopilotAreaState::Follows);
    assert_eq!(b.record.merges, 5, "14 days is still in");
    assert_eq!(
        r.row("src/b", NOW + 1).record.merges,
        2,
        "a second later the 14-day-old merges are out"
    );
}

#[tokio::test]
async fn areas_are_counted_per_project_and_by_repository() {
    let mut a = rig().await;
    // A second project with its own lead and the same area name.
    let other = a.w.core.new_project("other", false, None).await.unwrap();
    a.w.core.store.trust_project(&other.id, "test").unwrap();
    let repo = std::path::PathBuf::from(&other.path);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/base.txt"), "base\n").unwrap();
    run_git(&repo, &["add", "."]);
    commit_all(&repo, "base");
    let lead =
        a.w.core
            .create_thread(wire::CreateThread {
                project_id: Some(other.id.clone()),
                engine: "alpha".into(),
                model: None,
                title: Some("lead two".into()),
                worktree: None,
                skill_profile: None,
                base_branch: None,
                effort: None,
                options: Default::default(),
            })
            .await
            .unwrap();
    let token = bind(&a.w.core, &lead.id, "run-lead-two");
    let mut theirs = Vec::new();
    for n in 1..=5 {
        let (answer, error) = a
            .w
            .call(
                &token,
                "crew_start",
                json!({ "brief": brief(json!({ "files": [{ "path": format!("src/u{n}/**"), "mode": "write" }], "outcome": format!("Other {n}.") })), "answer_key": key() }),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap().to_owned();
        a.w.core
            .store
            .add_crew_merge(&record_of(
                &a.w,
                &task,
                &["src/a"],
                NOW - n * DAY,
                100 + usize::try_from(n).unwrap(),
            ))
            .unwrap();
        theirs.push(task);
    }
    let (_, _) = a.tripped().await;
    assert_eq!(a.mode("src/a").unwrap().state, "demoted");
    assert!(
        a.w.core
            .store
            .area_mode(&other.id, "src/a")
            .unwrap()
            .is_none(),
        "the other project is untouched"
    );
    let page = Page::new(Some(200), None).unwrap();
    let there = a.w.core.autopilot_areas(&other.id, page, NOW).unwrap();
    let row = &there.rows[0];
    assert_eq!(
        (row.area.as_str(), row.state),
        ("src/a", wire::AutopilotAreaState::Follows)
    );
    assert_eq!((row.record.merges, row.record.reverts), (5, 0));
    // The trust record reads one repository, or the whole device when none is named.
    let count = |trust: &wire::AutopilotTrust| {
        let area = trust.areas.iter().find(|r| r.name == "src/a").unwrap();
        (area.rate.merges, area.rate.reverts)
    };
    assert!(
        a.w.core
            .autopilot_trust(NOW, Some(&a.project))
            .map(|t| t.autonomy_by_area)
            .unwrap()
    );
    assert_eq!(
        count(&a.w.core.autopilot_trust(NOW, Some(&a.project)).unwrap()),
        (5, 2)
    );
    assert_eq!(
        count(&a.w.core.autopilot_trust(NOW, Some(&other.id)).unwrap()),
        (5, 0)
    );
    assert_eq!(
        count(&a.w.core.autopilot_trust(NOW, None).unwrap()),
        (10, 2)
    );
    assert!(
        a.w.core
            .autopilot_trust(NOW, Some("no-such-project"))
            .is_err()
    );
    // A revert in the other project counts there.
    let revert = |task: &str, at: i64| {
        let commit = format!("{:040x}", 0xbeef_u64);
        a.w.core.store.set_merge_reverted(task, &commit).unwrap();
        let record = a.w.core.store.crew_merge(task).unwrap().unwrap();
        a.w.core
            .area_breaker_after_revert(&record, &commit, at)
            .unwrap();
    };
    revert(&theirs[0], NOW);
    assert!(
        a.w.core
            .store
            .area_mode(&other.id, "src/a")
            .unwrap()
            .is_none(),
        "one revert there"
    );
}

#[tokio::test]
async fn the_reads_answer_not_found_with_the_flag_off() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let page = Page::new(None, None).unwrap();
    assert!(w.core.autopilot_areas("p", page, NOW).is_err());
    assert!(w.core.autopilot_area_history("p", "src/a").is_err());
    assert!(w.core.autopilot_thresholds().is_err());
}

#[tokio::test]
async fn the_thresholds_are_served_as_proposed() {
    let r = rig().await;
    let t = r.w.core.autopilot_thresholds().unwrap();
    assert_eq!(
        (
            t.window_days,
            t.min_merges,
            t.trip_reverts,
            t.recovery_days,
            t.recovery_merges,
            t.proposed
        ),
        (14, 5, 2, 14, 5, true)
    );
    let history =
        r.w.core
            .autopilot_area_history(&r.project, "src/a")
            .unwrap();
    assert!(history.events.is_empty() && !history.truncated);
    assert!(r.w.core.autopilot_area_history(&r.project, "").is_err());
    assert!(r.w.core.autopilot_area_history("../x", "src/a").is_err());
}

/// Two more merges in the project's area `src/x` on another branch, so the window holds 6: one already reverted.
async fn crowd(h: &super::autopilot_watch::Watched) {
    for (n, reverted) in [(5, false), (6, true)] {
        let (answer, error) = h
            .w
            .start(
                brief(json!({ "files": [{ "path": format!("src/t{n}/**"), "mode": "write" }], "outcome": format!("Task {n}.") })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap().to_owned();
        let mut record = record_of(
            &h.w,
            &task,
            &["src/x"],
            NOW - 10 - n,
            200 + usize::try_from(n).unwrap(),
        );
        record.branch = "other".into();
        h.w.core.store.add_crew_merge(&record).unwrap();
        if reverted {
            h.w.core
                .store
                .set_merge_reverted(&task, &format!("{:040x}", 0xaaaa_u64))
                .unwrap();
        }
    }
}

#[tokio::test]
async fn a_revert_found_by_the_watch_reaches_the_breaker() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/c.txt", "c\n")],
    ])
    .await;
    crowd(&h).await;
    let project = h.w.core.store.thread(&h.w.lead).unwrap().project_id;
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    let WatchOutcome::Reverted {
        task_id,
        revert_commit,
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert_eq!(task_id, h.tasks[2]);
    let mode =
        h.w.core
            .store
            .area_mode(&project, "src/x")
            .unwrap()
            .expect("the bisect revert tripped the area");
    assert_eq!(
        (mode.state.as_str(), mode.tripped_at),
        ("demoted", Some(NOW + 100))
    );
    let trail = h.w.core.store.area_events(&project, "src/x", 5).unwrap();
    assert_eq!(trail[0].cause, revert_commit);
    assert_eq!(events(&h.w, &h.tasks[2], "area_demoted").len(), 1);
}

#[tokio::test]
async fn a_revert_the_person_asks_for_reaches_the_breaker() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/c.txt", "c\n")],
        &[("src/d.txt", "d\n")],
    ])
    .await;
    crowd(&h).await;
    let project = h.w.core.store.thread(&h.w.lead).unwrap().project_id;
    // The branch is not checked out anywhere, as the queue leaves it.
    let commit =
        h.w.core
            .crew_answer_review(&h.tasks[1], ReviewAnswer::Revert, &h.target, NOW + 100)
            .await
            .unwrap()
            .expect("the revert commit");
    let mode =
        h.w.core
            .store
            .area_mode(&project, "src/x")
            .unwrap()
            .expect("the person's revert tripped the area");
    assert_eq!(mode.state, "demoted");
    let trail = h.w.core.store.area_events(&project, "src/x", 5).unwrap();
    assert_eq!(
        (trail[0].cause.as_str(), trail[0].decided_by.as_str()),
        (commit.as_str(), "breaker")
    );
    assert_eq!(
        h.w.core
            .store
            .area_mode(&project, "src/x")
            .unwrap()
            .unwrap()
            .state,
        "demoted"
    );
}

#[tokio::test]
async fn a_trip_is_evaluated_when_a_merge_is_recorded() {
    let mut r = rig().await;
    let mut tasks = Vec::new();
    for i in 0..3 {
        tasks.push(r.merge(&["src/a"], NOW - (6 - i) * DAY).await);
    }
    r.revert(&tasks[0], NOW - 4 * DAY);
    r.revert(&tasks[1], NOW - 4 * DAY);
    assert!(r.mode("src/a").is_none(), "3 merges are not enough data");
    r.merge_recorded(&["src/a"], NOW - 3 * DAY).await;
    assert!(r.mode("src/a").is_none(), "4 merges");
    let fifth = r.merge_recorded(&["src/a"], NOW - 2 * DAY).await;
    let mode = r.mode("src/a").expect("the fifth merge meets the rule");
    assert_eq!(mode.state, "demoted");
    let trail = r.w.core.store.area_events(&r.project, "src/a", 5).unwrap();
    assert_eq!(trail.len(), 1);
    assert!(trail[0].cause.starts_with("merge:"), "{}", trail[0].cause);
    assert_eq!(trail[0].from_mode.as_deref(), Some("full"));
    assert_eq!(events(&r.w, &fifth, "area_demoted").len(), 1);
    // Looking again records nothing more.
    let stored = r.w.core.store.crew_merge(&fifth).unwrap().unwrap();
    r.w.core
        .area_breaker_after_merge(&stored, NOW - DAY)
        .unwrap();
    assert_eq!(r.kinds("src/a"), ["demoted"]);
    // A ceiling of On is never lowered this way either.
    let mut on = rig().await;
    on.w.core
        .set_crew_autopilot_mode(&ModeScope::Repo(on.project.clone()), Mode::On)
        .unwrap();
    let t = [
        on.merge(&["src/a"], NOW - 6 * DAY).await,
        on.merge(&["src/a"], NOW - 5 * DAY).await,
    ];
    on.revert(&t[0], NOW - 4 * DAY);
    on.revert(&t[1], NOW - 4 * DAY);
    for d in [3, 2, 1] {
        on.merge_recorded(&["src/a"], NOW - d * DAY).await;
    }
    assert!(on.mode("src/a").is_none());
}

#[tokio::test]
async fn the_way_back_is_shown_in_progress_then_met_and_queuing_never_waits_for_the_lock() {
    let mut r = rig().await;
    let (tasks, _) = r.tripped().await;
    let any = tasks[0].clone();
    assert_eq!(r.row("src/a", NOW).state, wire::AutopilotAreaState::Demoted);
    r.merge(&["src/a"], NOW + DAY).await;
    r.merge(&["src/a"], NOW + 2 * DAY).await;
    let row = r.row("src/a", NOW + 3 * DAY);
    assert_eq!(row.state, wire::AutopilotAreaState::Recovering);
    let progress = row.recovery.unwrap();
    assert_eq!(
        (
            progress.clean_days,
            progress.days_needed,
            progress.merges,
            progress.merges_needed
        ),
        (3, 14, 2, 5)
    );
    for d in 3..=5 {
        r.merge(&["src/a"], NOW + d * DAY).await;
    }
    // The lock is held (a long check or a bisect): queuing skips the check instead of waiting.
    let guard = r.w.core.crew_merges.lock().await;
    assert!(!r.w.core.area_restore_if_free(&any, NOW + 14 * DAY).unwrap());
    assert_eq!(r.mode("src/a").unwrap().state, "demoted");
    drop(guard);
    assert!(r.w.core.area_restore_if_free(&any, NOW + 14 * DAY).unwrap());
    assert_eq!(r.mode("src/a").unwrap().state, "follows");
}

#[tokio::test]
async fn the_listed_key_finds_the_history() {
    let mut r = rig().await;
    r.tripped().await;
    let row = r.row("src/a", NOW);
    let history =
        r.w.core
            .autopilot_area_history(&r.project, &row.area_key)
            .unwrap();
    assert_eq!(history.events.len(), 1);
    assert_eq!(history.events[0].kind, "demoted");
}
