//! Autonomy by area, enforced (slice 4j): the merge gate reads the mode of every area a diff touches, a person can keep the ceiling or
//! set an area to Off, and the failures fail closed. The gate tests use real git in a temporary repository and fake engines; the
//! override tests use merge rows written beside real tasks. Nothing is pushed.

use orbyn_contracts as wire;
use serde_json::json;

use super::autopilot_areas::{DAY, Rig, rig};
use super::autopilot_merge::{Merging, merging, passed};
use super::autopilot_support::events;
use super::autopilot_watch::NOW;
use super::harness::World;
use crate::crew::{MergeOutcome, Mode, Page, ReviewAnswer};
use crate::error::CoreError;

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// The breaker's record that `area` was lowered, as it writes it.
fn lower(w: &World, area: &str) {
    crate::crew::fixtures::lower_area(&w.core, &project_of(w), area, NOW);
}

fn seen(merges: u32, reverts: u32, tripped_at: Option<i64>) -> wire::AutopilotAreaSeen {
    wire::AutopilotAreaSeen {
        merges,
        reverts,
        tripped_at,
    }
}

fn choose(
    project: &str,
    area: &str,
    choice: &str,
    seen: wire::AutopilotAreaSeen,
) -> wire::AutopilotAreaOverrideBody {
    wire::AutopilotAreaOverrideBody {
        project: project.to_owned(),
        area: area.to_owned(),
        choice: choice.to_owned(),
        acknowledged: true,
        seen,
        reason: None,
    }
}

/// A Full crew whose tasks write into `src/db`: turn 0 a migration (high), turn 1 a query file (medium), turn 2 a note (low).
async fn db(mode: Mode) -> Merging {
    merging(mode, |n| match n {
        0 => vec![("src/db/migrations/001.sql", "create table t (id int);\n")],
        1 => vec![("src/db/query.txt", "select 1;\n")],
        _ => vec![("src/db/notes.md", "A note.\n")],
    })
    .await
}

const HIGH: &str = "src/db/migrations/**";
const MEDIUM: &str = "src/db/query.txt";
const LOW: &str = "src/db/notes.md";

#[tokio::test]
async fn a_lowered_area_makes_a_high_risk_diff_wait_while_the_others_still_merge() {
    let m = db(Mode::Full).await;
    lower(&m.w, "src/db");
    let high = passed(&m.w, &[HIGH], "Add a migration.").await;
    let medium = passed(&m.w, &[MEDIUM], "Tune a query.").await;
    let waiting =
        m.w.core
            .crew_queue_merge(&high, &m.target, NOW)
            .await
            .unwrap();
    // The tier does not change: it is still high, and the mode the crew chose is still Full. Only the gate reads the area.
    assert_eq!(
        (
            waiting.state.as_str(),
            waiting.risk.as_str(),
            waiting.mode.as_str()
        ),
        ("waiting", "high", "full")
    );
    let note = waiting.note.clone().unwrap();
    assert!(note.contains("waits for a person"), "{note}");
    assert!(
        note.contains("Area src/db is on On: 0 reverts in 14 days · 0 merges"),
        "{note}"
    );
    let queued =
        m.w.core
            .crew_queue_merge(&medium, &m.target, NOW + 1)
            .await
            .unwrap();
    assert_eq!(
        (queued.state.as_str(), queued.risk.as_str()),
        ("queued", "medium")
    );
    let outcomes = m.process(NOW + 2).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Merged { task_id, .. }] if *task_id == medium),
        "{outcomes:?}"
    );
    // The ledger names the lowered mode, and the risk report and the Review queue carry the marker.
    let queued_event = &events(&m.w, &high, "merge_queued")[0].body;
    assert_eq!(queued_event["effective_mode"], "on");
    let report = m.w.core.autopilot_risk_at(&high, NOW).unwrap();
    assert_eq!(report.risk, wire::AutopilotRisk::High);
    assert_eq!(report.area_modes.len(), 1);
    assert_eq!(report.area_modes[0].mode, wire::AutopilotMode::On);
    assert!(
        report.area_modes[0]
            .line
            .starts_with("Area src/db is on On:")
    );
    let review =
        m.w.core
            .autopilot_review_queue(NOW + 2, Page::new(None, None).unwrap())
            .unwrap();
    assert_eq!(review.items.len(), 1);
    assert_eq!(
        review.items[0].area_modes.len(),
        1,
        "a lowered area is marked"
    );
    // Released by a person, the high diff goes in.
    m.w.core.crew_release_merge(&high).unwrap();
    assert!(matches!(
        &m.process(NOW + 3).await[..],
        [MergeOutcome::Merged { .. }]
    ));
}

#[tokio::test]
async fn a_ceiling_of_on_or_off_is_never_changed_by_a_lowering() {
    let on = db(Mode::On).await;
    lower(&on.w, "src/db");
    let high = passed(&on.w, &[HIGH], "Add a migration.").await;
    let medium = passed(&on.w, &[MEDIUM], "Tune a query.").await;
    let entry =
        on.w.core
            .crew_queue_merge(&medium, &on.target, NOW)
            .await
            .unwrap();
    assert_eq!(
        entry.state, "queued",
        "On lets medium work through, lowered area or not"
    );
    let entry =
        on.w.core
            .crew_queue_merge(&high, &on.target, NOW)
            .await
            .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.note.as_deref()),
        (
            "waiting",
            Some("A high-risk diff waits for a person under On.")
        )
    );
    assert_eq!(
        events(&on.w, &high, "merge_queued")[0]
            .body
            .get("effective_mode"),
        None,
        "nothing was lowered"
    );

    let off = db(Mode::Off).await;
    lower(&off.w, "src/db");
    let low = passed(&off.w, &[HIGH], "Add a migration.").await;
    let entry = off
        .w
        .core
        .crew_queue_merge(&low, &off.target, NOW)
        .await
        .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.note.as_deref()),
        (
            "waiting",
            Some("Autopilot is off, so a person releases every merge.")
        )
    );
}

#[tokio::test]
async fn keep_ceiling_lets_a_high_diff_through_the_full_path_and_off_makes_everything_wait() {
    let m = db(Mode::Full).await;
    lower(&m.w, "src/db");
    let project = project_of(&m.w);
    let row =
        m.w.core
            .autopilot_area_override(
                &choose(&project, "src/db", "keep_ceiling", seen(0, 0, Some(NOW))),
                NOW,
            )
            .await
            .unwrap();
    assert_eq!(row.state, wire::AutopilotAreaState::Overridden);
    let high = passed(&m.w, &[HIGH], "Add a migration.").await;
    let entry =
        m.w.core
            .crew_queue_merge(&high, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.risk.as_str()),
        ("queued", "high"),
        "the area keeps the ceiling, so a high diff goes the Full path"
    );
    assert!(matches!(
        &m.process(NOW + 1).await[..],
        [MergeOutcome::Merged { .. }]
    ));
    let review =
        m.w.core
            .autopilot_review_queue(NOW + 1, Page::new(None, None).unwrap())
            .unwrap();
    assert!(
        review.items[0].area_modes.is_empty(),
        "a kept ceiling is not a lowered area"
    );

    // Off: the same area, every tier waits, and the sentence says why.
    let off = db(Mode::Full).await;
    lower(&off.w, "src/db");
    let project = project_of(&off.w);
    off.w
        .core
        .autopilot_area_override(
            &choose(&project, "src/db", "off", seen(0, 0, Some(NOW))),
            NOW,
        )
        .await
        .unwrap();
    for (paths, what) in [(HIGH, "high"), (MEDIUM, "medium"), (LOW, "low")] {
        let id = passed(&off.w, &[paths], what).await;
        let entry = off
            .w
            .core
            .crew_queue_merge(&id, &off.target, NOW)
            .await
            .unwrap();
        assert_eq!(entry.state, "waiting", "{what}");
        assert!(
            entry.note.unwrap().contains("An area is set to Off"),
            "{what}"
        );
    }
    assert!(off.process(NOW).await.is_empty());
}

#[tokio::test]
async fn the_lowest_area_of_the_diff_decides() {
    let m = merging(Mode::Full, |n| match n {
        0 => vec![("src/db/a.txt", "a\n"), ("src/web/a.txt", "a\n")],
        _ => vec![("src/db/b.txt", "b\n"), ("src/web/b.txt", "b\n")],
    })
    .await;
    lower(&m.w, "src/db");
    let first = passed(&m.w, &["src/db/a.txt", "src/web/a.txt"], "Touch both.").await;
    let entry =
        m.w.core
            .crew_queue_merge(&first, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(entry.areas, ["src/db", "src/web"]);
    assert_eq!(
        entry.state, "queued",
        "On for one area lets a medium diff through"
    );
    // A person sets the other area to Off: now the lowest of the two is Off.
    let project = project_of(&m.w);
    m.w.core
        .autopilot_area_override(&choose(&project, "src/web", "off", seen(0, 0, None)), NOW)
        .await
        .unwrap();
    let second = passed(
        &m.w,
        &["src/db/b.txt", "src/web/b.txt"],
        "Touch both again.",
    )
    .await;
    let entry =
        m.w.core
            .crew_queue_merge(&second, &m.target, NOW + 1)
            .await
            .unwrap();
    assert_eq!(entry.state, "waiting");
    let report = m.w.core.autopilot_risk_at(&second, NOW + 1).unwrap();
    let modes: Vec<(String, wire::AutopilotMode)> = report
        .area_modes
        .iter()
        .map(|a| (a.area.clone(), a.mode))
        .collect();
    assert_eq!(
        modes,
        [
            ("src/db".to_owned(), wire::AutopilotMode::On),
            ("src/web".to_owned(), wire::AutopilotMode::Off)
        ]
    );
}

#[tokio::test]
async fn an_area_lowered_after_queuing_holds_the_merge_when_it_is_processed() {
    let m = db(Mode::Full).await;
    let high = passed(&m.w, &[HIGH], "Add a migration.").await;
    let entry =
        m.w.core
            .crew_queue_merge(&high, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(entry.state, "queued");
    lower(&m.w, "src/db");
    let before = m.tip().await;
    let outcomes = m.process(NOW + 1).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::NeedsPerson { why, .. }] if why.contains("an area is lowered")),
        "{outcomes:?}"
    );
    assert_eq!(m.tip().await, before);
}

#[tokio::test]
async fn an_unreadable_area_fails_closed_as_off_with_a_ledger_event() {
    let m = db(Mode::Full).await;
    let high = passed(&m.w, &[HIGH], "Add a migration.").await;
    let medium = passed(&m.w, &[MEDIUM], "Tune a query.").await;
    let low = passed(&m.w, &[LOW], "Write a note.").await;
    m.w.core.store.break_area_modes_for_test();
    let entry =
        m.w.core
            .crew_queue_merge(&high, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(entry.state, "waiting");
    assert!(entry.note.unwrap().contains("could not be read"));
    let rows = events(&m.w, &high, "area_read_failed");
    assert_eq!(rows.len(), 1, "visible in the ledger, not only the log");
    assert_eq!(rows[0].body["treated_as"], "off");
    // Off, not On: a person may have set the area to Off, so a failure to read never lets even a low diff through.
    for task in [&medium, &low] {
        let entry =
            m.w.core
                .crew_queue_merge(task, &m.target, NOW)
                .await
                .unwrap();
        assert_eq!(entry.state, "waiting", "{task}");
    }
}

#[tokio::test]
async fn a_breaker_that_fails_after_a_revert_is_in_the_ledger_and_tried_again() {
    let m = db(Mode::Full).await;
    let id = passed(&m.w, &[MEDIUM], "Tune a query.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    assert!(matches!(
        &m.process(NOW).await[..],
        [MergeOutcome::Merged { .. }]
    ));
    m.w.core.store.break_area_modes_for_test();
    let reverted =
        m.w.core
            .crew_answer_review(&id, ReviewAnswer::Revert, &m.target, NOW + 5)
            .await
            .unwrap();
    assert!(reverted.is_some(), "the revert itself stands");
    let failed = events(&m.w, &id, "area_breaker_failed");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].body["stage"], "revert");
    assert_eq!(failed[0].body["resolved"], false);
    assert_eq!(failed[0].body["revert"], json!(reverted.clone().unwrap()));
    // Repaired, the next look at the same revert records it once and marks the failure resolved.
    m.w.core.store.fix_area_modes_for_test();
    m.w.core.area_breaker_retry(NOW + 6);
    m.w.core.area_breaker_retry(NOW + 7);
    let failed = events(&m.w, &id, "area_breaker_failed");
    assert_eq!(failed.len(), 2, "one resolved row, and not again");
    assert_eq!(failed[1].body["resolved"], true);
}

#[tokio::test]
async fn a_failed_breaker_trips_the_area_at_the_next_merge_without_a_second_revert() {
    let mut r = rig().await;
    let mut tasks = Vec::new();
    for i in 0..5 {
        tasks.push(r.merge(&["src/a"], NOW - (5 - i) * DAY).await);
    }
    r.revert(&tasks[0], NOW);
    // The second revert reaches the breaker while the table is damaged.
    let commit = format!("{:040x}", 0xfeed_u64);
    r.w.core
        .store
        .set_merge_reverted(&tasks[1], &commit)
        .unwrap();
    let record = r.w.core.store.crew_merge(&tasks[1]).unwrap().unwrap();
    r.w.core.store.break_area_modes_for_test();
    let error =
        r.w.core
            .area_breaker_after_revert(&record, &commit, NOW)
            .unwrap_err();
    r.w.core
        .area_breaker_failed(&tasks[1], "revert", Some(&commit), &error);
    r.w.core.store.fix_area_modes_for_test();
    assert!(r.mode("src/a").is_none(), "not recorded yet");
    // The next merge looks at the revert again.
    r.merge_recorded(&["src/a"], NOW + 1).await;
    assert_eq!(r.mode("src/a").unwrap().state, "demoted");
    assert_eq!(r.kinds("src/a"), ["demoted"]);
    assert_eq!(events(&r.w, &tasks[1], "area_breaker_failed").len(), 2);
}

async fn lowered_rig() -> (Rig, Vec<String>) {
    let mut r = rig().await;
    let (tasks, _) = r.tripped().await;
    (r, tasks)
}

#[tokio::test]
async fn a_person_keeps_the_ceiling_and_the_trail_names_the_audit_row() {
    let (r, _) = lowered_rig().await;
    let before = r.row("src/a", NOW);
    assert_eq!(before.now, wire::AutopilotMode::On);
    let seen = seen(5, 2, Some(NOW));
    let mut body = choose(&r.project, "src/a", "keep_ceiling", seen.clone());
    body.reason = Some("The two reverts were one bad deploy.".into());
    let row =
        r.w.core
            .autopilot_area_override(&body, NOW + 60)
            .await
            .unwrap();
    assert_eq!(row.state, wire::AutopilotAreaState::Overridden);
    assert_eq!(row.now, wire::AutopilotMode::Full);
    assert_eq!(row.person_override.as_ref().unwrap().choice, "keep_ceiling");
    let trail = r.w.core.store.area_events(&r.project, "src/a", 10).unwrap();
    assert_eq!(trail.len(), 2);
    let event = &trail[0];
    assert_eq!(
        (event.kind.as_str(), event.decided_by.as_str()),
        ("override_set", "person")
    );
    assert_eq!(
        (event.from_mode.as_deref(), event.to_mode.as_deref()),
        (Some("on"), Some("full"))
    );
    // The cause is the audit row, and that row says what the person did.
    let audit_id: i64 = event
        .cause
        .strip_prefix("person:")
        .unwrap()
        .parse()
        .unwrap();
    let (actor, action, detail): (String, String, String) =
        r.w.core
            .store
            .reader()
            .query_row(
                "SELECT actor, action, detail FROM audit_events WHERE id = ?1",
                [audit_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
    assert_eq!(
        (actor.as_str(), action.as_str()),
        ("user", "crew.area.override")
    );
    assert!(detail.contains("one bad deploy"), "{detail}");
    // The same choice twice records nothing the second time.
    r.w.core
        .autopilot_area_override(&body, NOW + 120)
        .await
        .unwrap();
    assert_eq!(r.kinds("src/a"), ["demoted", "override_set"]);
}

#[tokio::test]
async fn a_choice_is_refused_above_the_ceiling_without_an_acknowledgement_or_on_an_old_record() {
    let (r, _) = lowered_rig().await;
    let good = || choose(&r.project, "src/a", "keep_ceiling", seen(5, 2, Some(NOW)));
    for choice in ["full", "on", "", "keep ceiling"] {
        let mut body = good();
        body.choice = choice.to_owned();
        let error =
            r.w.core
                .autopilot_area_override(&body, NOW)
                .await
                .unwrap_err();
        assert!(matches!(error, CoreError::Invalid(_)), "{choice}: {error}");
    }
    let mut body = good();
    body.acknowledged = false;
    assert!(matches!(
        r.w.core
            .autopilot_area_override(&body, NOW)
            .await
            .unwrap_err(),
        CoreError::Invalid(_)
    ));
    for old in [
        seen(5, 1, Some(NOW)),
        seen(4, 2, Some(NOW)),
        seen(5, 2, Some(NOW - 1)),
        seen(5, 2, None),
    ] {
        let mut body = good();
        body.seen = old;
        assert!(
            matches!(
                r.w.core
                    .autopilot_area_override(&body, NOW)
                    .await
                    .unwrap_err(),
                CoreError::Stale(_)
            ),
            "a record the person did not see"
        );
    }
    // An area the breaker never touched has nothing to keep.
    let other = choose(&r.project, "src/z", "keep_ceiling", seen(0, 0, None));
    assert!(matches!(
        r.w.core
            .autopilot_area_override(&other, NOW)
            .await
            .unwrap_err(),
        CoreError::Invalid(_)
    ));
    assert_eq!(
        r.kinds("src/a"),
        ["demoted"],
        "every refusal left the trail alone"
    );
    assert_eq!(CoreError::Stale("x".into()).status(), 409);
    assert_eq!(CoreError::Stale("x".into()).code(), "stale");
}

#[tokio::test]
async fn a_new_trip_ends_keep_ceiling_and_follow_returns_the_area_to_the_record() {
    let (mut r, _) = lowered_rig().await;
    r.w.core
        .autopilot_area_override(
            &choose(&r.project, "src/a", "keep_ceiling", seen(5, 2, Some(NOW))),
            NOW + 10,
        )
        .await
        .unwrap();
    // Another merge in the area is reverted: a new trip.
    let later = r.merge(&["src/a"], NOW + DAY).await;
    r.revert(&later, NOW + DAY);
    assert_eq!(
        r.kinds("src/a"),
        [
            "demoted",
            "override_set",
            "override_cleared",
            "recovery_reset"
        ]
    );
    let row = r.row("src/a", NOW + DAY);
    assert!(
        matches!(
            row.state,
            wire::AutopilotAreaState::Demoted | wire::AutopilotAreaState::Recovering
        ),
        "it shows lowered again: {:?}",
        row.state
    );
    assert_eq!(row.now, wire::AutopilotMode::On);
    assert!(row.person_override.is_none());
    let cleared = &r.w.core.store.area_events(&r.project, "src/a", 10).unwrap()[1];
    assert_eq!(
        (cleared.kind.as_str(), cleared.decided_by.as_str()),
        ("override_cleared", "breaker")
    );

    // Off stays until the person follows the record again; the breaker never loosens it.
    let record = r
        .view(NOW + DAY)
        .rows
        .into_iter()
        .find(|x| x.area == "src/a")
        .unwrap()
        .record;
    let current = seen(record.merges, record.reverts, Some(NOW));
    r.w.core
        .autopilot_area_override(
            &choose(&r.project, "src/a", "off", current.clone()),
            NOW + DAY,
        )
        .await
        .unwrap();
    let again = r.merge(&["src/a"], NOW + 2 * DAY).await;
    r.revert(&again, NOW + 2 * DAY);
    assert_eq!(r.row("src/a", NOW + 2 * DAY).now, wire::AutopilotMode::Off);
    let follow = wire::AutopilotAreaFollowBody {
        project: r.project.clone(),
        area: "src/a".into(),
        seen: None,
    };
    let row =
        r.w.core
            .autopilot_area_follow(&follow, NOW + 3 * DAY)
            .await
            .unwrap();
    assert!(row.person_override.is_none());
    assert_eq!(
        row.now,
        wire::AutopilotMode::On,
        "back to the breaker's record, still lowered"
    );
    let count = r.kinds("src/a").len();
    r.w.core
        .autopilot_area_follow(&follow, NOW + 4 * DAY)
        .await
        .unwrap();
    assert_eq!(
        r.kinds("src/a").len(),
        count,
        "following twice records nothing"
    );
    // A follow that carries an old record is refused.
    let old = wire::AutopilotAreaFollowBody {
        seen: Some(seen(1, 1, None)),
        ..follow
    };
    assert!(matches!(
        r.w.core
            .autopilot_area_follow(&old, NOW + 5 * DAY)
            .await
            .unwrap_err(),
        CoreError::Stale(_)
    ));
}

#[tokio::test]
async fn the_risk_report_the_review_queue_and_the_digest_carry_the_areas() {
    let (r, tasks) = lowered_rig().await;
    let report = r.w.core.autopilot_risk_at(&tasks[4], NOW).unwrap();
    assert_eq!(report.area_modes.len(), 1);
    assert_eq!(
        report.area_modes[0].line,
        "Area src/a is on On: 2 reverts in 14 days · 5 merges"
    );
    let review =
        r.w.core
            .autopilot_review_queue(NOW, Page::new(None, None).unwrap())
            .unwrap();
    assert!(!review.items.is_empty());
    assert!(
        review
            .items
            .iter()
            .all(|i| i.area_modes.len() == 1 && i.area_modes[0].mode == wire::AutopilotMode::On)
    );
    let digest = r.w.core.autopilot_digest(NOW + 1, Some(24)).unwrap();
    assert_eq!(digest.areas_changed.len(), 1);
    assert_eq!(digest.areas_changed[0].kind, "demoted");
    assert_eq!(digest.areas_changed[0].area, "src/a");
    assert!(!digest.quiet);
    let quiet = rig().await;
    let digest = quiet.w.core.autopilot_digest(NOW, Some(24)).unwrap();
    assert!(digest.areas_changed.is_empty());
}

#[tokio::test]
async fn a_revert_that_lands_while_the_person_waits_makes_the_choice_stale() {
    let (r, tasks) = lowered_rig().await;
    let core = r.w.core.clone();
    let holder = r.w.core.clone();
    let body = choose(&r.project, "src/a", "keep_ceiling", seen(5, 2, Some(NOW)));
    // A merge is being checked: it holds the merge lock while the person's choice waits for it.
    let guard = holder.crew_merges.lock().await;
    let waiting = tokio::spawn(async move { core.autopilot_area_override(&body, NOW + 1).await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    r.revert(&tasks[2], NOW + 2);
    drop(guard);
    let error = waiting.await.unwrap().unwrap_err();
    assert!(matches!(error, CoreError::Stale(_)), "{error}");
    assert_eq!(
        r.kinds("src/a"),
        ["demoted", "recovery_reset"],
        "no override was written"
    );
}

#[tokio::test]
async fn a_return_ends_keep_ceiling_in_the_trail_and_an_area_name_is_checked() {
    let (r, tasks) = lowered_rig().await;
    r.w.core
        .autopilot_area_override(
            &choose(&r.project, "src/a", "keep_ceiling", seen(5, 2, Some(NOW))),
            NOW + 1,
        )
        .await
        .unwrap();
    let mut r = r;
    for d in 1..=5 {
        r.merge(&["src/a"], NOW + d * DAY).await;
    }
    r.w.core
        .area_restore_check(&tasks[0], NOW + 14 * DAY)
        .unwrap();
    assert_eq!(
        r.kinds("src/a"),
        ["demoted", "override_set", "override_cleared", "restored"]
    );
    assert!(r.mode("src/a").unwrap().person_override.is_none());
    for bad in [
        "", "/src/a", "src/a/", "src//a", "src/a/b", "../a", "src/..", "a\u{7}b",
    ] {
        let body = choose(&r.project, bad, "off", seen(0, 0, None));
        assert!(
            matches!(
                r.w.core
                    .autopilot_area_override(&body, NOW)
                    .await
                    .unwrap_err(),
                CoreError::Invalid(_)
            ),
            "{bad:?}"
        );
    }
}
