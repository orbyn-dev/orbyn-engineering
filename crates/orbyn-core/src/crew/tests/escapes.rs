//! The escape log (slice 4k, PR 1) on a real `Core` with fake engines: a revert made the way the product makes one logs exactly one
//! possible escape, the log and detail read it with the counts apart, the Preview flag hides it, and a revert changes nothing else.

use orbyn_contracts as wire;

use super::autopilot_areas::{DAY, rig};
use super::autopilot_watch::NOW;
use crate::crew::Page;
use crate::error::CoreError;

fn page() -> Page {
    Page::new(Some(200), None).unwrap()
}

#[tokio::test]
async fn a_revert_is_one_possible_escape_counted_apart_and_idempotent() {
    let mut rig = rig().await;
    let a = rig.merge(&["src/a"], NOW - 3 * DAY).await;
    let b = rig.merge(&["src/a"], NOW - 2 * DAY).await;
    let core = &rig.w.core;
    let log = core.crew_escape_log(&rig.project, None, page()).unwrap();
    assert_eq!(
        (
            log.merges,
            log.possible,
            log.confirmed,
            log.dismissed,
            log.total
        ),
        (2, 0, 0, 0, 0)
    );
    assert!(log.low_sample, "two merges are under the sample of ten");

    rig.revert(&a, NOW);
    rig.revert(&a, NOW + 5);
    let log = core.crew_escape_log(&rig.project, None, page()).unwrap();
    assert_eq!(
        (log.possible, log.confirmed, log.dismissed, log.total),
        (1, 0, 0, 1)
    );
    let engine = &log.engines[0];
    assert_eq!(
        (
            engine.engine.as_str(),
            engine.merges,
            engine.possible,
            engine.confirmed
        ),
        ("alpha", 2, 1, 0)
    );
    let row = &log.rows[0];
    assert_eq!(
        (
            row.merge_task_id.as_str(),
            row.source,
            row.state,
            row.decided_by
        ),
        (
            a.as_str(),
            wire::EscapeSource::Revert,
            wire::EscapeState::Possible,
            None
        )
    );
    assert!(row.line.contains("has not confirmed"), "{}", row.line);

    let detail = core.crew_escape_detail(&row.id).unwrap();
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.events[0].kind, wire::EscapeEventKind::Detected);
    assert!(detail.events[0].cause.starts_with("revert:"));

    // A merge nobody reverted is not an escape, and no read wrote anything.
    assert!(core.store.crew_escape_of_merge(&b).unwrap().is_none());
    assert_eq!(
        core.crew_escape_log(&rig.project, Some("possible"), page())
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        core.crew_escape_log(&rig.project, Some("confirmed"), page())
            .unwrap()
            .total,
        0
    );
    assert!(matches!(
        core.crew_escape_log(&rig.project, Some("likely"), page()),
        Err(CoreError::Invalid(_))
    ));
}

#[tokio::test]
async fn the_log_is_display_only_a_possible_escape_changes_no_area_mode() {
    let mut rig = rig().await;
    let tasks = [
        rig.merge_recorded(&["src/a"], NOW - 5 * DAY).await,
        rig.merge_recorded(&["src/a"], NOW - 4 * DAY).await,
    ];
    let before = rig.mode("src/a");
    rig.w
        .core
        .store
        .set_merge_reverted(&tasks[0], "feed01")
        .unwrap();
    assert_eq!(
        rig.w
            .core
            .store
            .crew_escapes_of_project(&rig.project, 0)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        rig.mode("src/a"),
        before,
        "the breaker reads reverts, never the escape log"
    );
}

#[tokio::test]
async fn the_flag_off_and_unknown_ids_answer_not_found_and_a_stored_document_is_read_closed() {
    let rig = rig().await;
    let core = &rig.w.core;
    assert!(matches!(
        core.crew_escape_detail("esc_none"),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.crew_escape_log("nobody", None, page()),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.crew_escape_detail("../x"),
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(
        core.crew_escape_thresholds().unwrap(),
        super::super::escapes::DEFAULTS
    );
    core.store
        .set_setting(super::super::escapes::THRESHOLDS_KEY, "{")
        .unwrap();
    assert!(matches!(
        core.crew_escape_thresholds(),
        Err(CoreError::Unreadable(_))
    ));
}

fn seen(state: wire::EscapeState) -> wire::EscapeSeen {
    wire::EscapeSeen { state, gap: None }
}

fn decide(project: &str, state: wire::EscapeState) -> wire::EscapeDecide {
    wire::EscapeDecide {
        project: project.to_owned(),
        seen: seen(state),
        note: None,
    }
}

#[tokio::test]
async fn nothing_automatic_confirms_and_a_persons_confirm_moves_only_the_escape_counts() {
    let mut rig = rig().await;
    let tasks = [
        rig.merge_recorded(&["src/a"], NOW - 5 * DAY).await,
        rig.merge_recorded(&["src/a"], NOW - 4 * DAY).await,
    ];
    // A revert, a second revert write, a watch that reads broken: every automatic path writes `possible` and no more.
    rig.revert(&tasks[0], NOW);
    rig.revert(&tasks[0], NOW + 5);
    rig.w
        .core
        .store
        .set_merge_reverted(&tasks[0], "feed99")
        .unwrap();
    let core = &rig.w.core;
    let log = core.crew_escape_log(&rig.project, None, page()).unwrap();
    assert_eq!((log.possible, log.confirmed, log.dismissed), (1, 0, 0));
    let id = log.rows[0].id.clone();
    let mode = rig.mode("src/a");

    let row = core
        .crew_escape_confirm(&id, &decide(&rig.project, wire::EscapeState::Possible))
        .unwrap();
    assert_eq!(
        (row.state, row.decided_by),
        (
            wire::EscapeState::Confirmed,
            Some(wire::EscapeActor::Person)
        )
    );
    assert!(row.line.contains("Confirmed by a person"), "{}", row.line);
    let log = core.crew_escape_log(&rig.project, None, page()).unwrap();
    assert_eq!((log.possible, log.confirmed, log.dismissed), (0, 1, 0));
    assert_eq!((log.engines[0].possible, log.engines[0].confirmed), (0, 1));
    // The detail trail names the person's act, oldest first.
    let detail = core.crew_escape_detail(&id).unwrap();
    let kinds: Vec<_> = detail.events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            wire::EscapeEventKind::Detected,
            wire::EscapeEventKind::Confirmed
        ]
    );
    assert_eq!(detail.events[1].decided_by, wire::EscapeActor::Person);
    // Display only: no area mode moved, and a later revert of the same merge keeps it confirmed.
    assert_eq!(rig.mode("src/a"), mode, "a confirm feeds no breaker");
    rig.w
        .core
        .store
        .set_merge_reverted(&tasks[0], "feed98")
        .unwrap();
    let again = core
        .crew_escape_confirm(&id, &decide(&rig.project, wire::EscapeState::Possible))
        .unwrap();
    assert_eq!(again, row, "the same confirm twice is the same answer");
    assert_eq!(
        core.crew_escape_log(&rig.project, Some("possible"), page())
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn the_acts_check_the_project_the_id_and_the_note_before_they_write() {
    let mut rig = rig().await;
    let task = rig.merge_recorded(&["src/a"], NOW - 5 * DAY).await;
    let core = &rig.w.core;
    let none = |r: Result<wire::EscapeRow, CoreError>, what: &str| {
        assert!(matches!(r, Err(CoreError::Invalid(_))), "{what}: {r:?}");
    };
    let mark = |project: &str, merge: &str, note: Option<String>| wire::EscapeMark {
        project: project.to_owned(),
        merge_task_id: merge.to_owned(),
        seen: None,
        note,
    };
    none(
        core.crew_escape_mark(&mark("../x", &task, None)),
        "project id",
    );
    none(
        core.crew_escape_mark(&mark(&rig.project, "../x", None)),
        "merge id",
    );
    none(
        core.crew_escape_mark(&mark(&rig.project, &task, Some("x".repeat(201)))),
        "a long note",
    );
    assert!(matches!(
        core.crew_escape_mark(&mark("nobody", &task, None)),
        Err(CoreError::NotFound(_))
    ));
    assert!(
        core.crew_escape_log(&rig.project, None, page())
            .unwrap()
            .rows
            .is_empty()
    );

    // A blank note is no note; a mark makes one confirmed escape that says a person marked it.
    let row = core
        .crew_escape_mark(&mark(&rig.project, &task, Some("   ".into())))
        .unwrap();
    assert_eq!(
        (row.source, row.state, row.reason.as_deref()),
        (
            wire::EscapeSource::PersonMarked,
            wire::EscapeState::Confirmed,
            Some("A person marked this merge as an escape.")
        )
    );
    let gap = core
        .crew_escape_set_gap(
            &row.id,
            &wire::EscapeGapSet {
                project: rig.project.clone(),
                seen: seen(wire::EscapeState::Confirmed),
                gap: wire::EscapeGap::ToolingGap,
            },
        )
        .unwrap();
    assert_eq!(
        (gap.gap, gap.gap_by),
        (
            Some(wire::EscapeGap::ToolingGap),
            Some(wire::EscapeActor::Person)
        )
    );
    assert!(matches!(
        core.crew_escape_dismiss(&row.id, &decide(&rig.project, wire::EscapeState::Confirmed)),
        Err(CoreError::Stale(_))
    ));
}
