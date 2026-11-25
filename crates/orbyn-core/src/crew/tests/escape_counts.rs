//! One area's escape counts by engine (docs/crew/escapes.md, "Screens and the reads they need") on a real `Core` with fake engines: the
//! counts are the area's and the engine's alone, possible and confirmed stay apart, an unknown project and an area that is not a name
//! are refused rather than served as nothing, a cut list says it was cut, and a read writes nothing.

use serde_json::json;

use super::autopilot_areas::{DAY, Rig, rig};
use super::autopilot_watch::NOW;
use super::harness::{Turn, brief, key, world};
use crate::crew::Page;
use crate::error::CoreError;
use orbyn_contracts as wire;

fn page() -> Page {
    Page::new(Some(200), None).unwrap()
}

/// One engine's counts, found by the engine it names rather than by a place in the list: the read orders engines
/// busiest first, which is its contract, not a fixture order a test can read a position from.
fn engine_counts<'a>(counts: &'a wire::EscapeCounts, name: &str) -> &'a wire::EscapeEngineCounts {
    counts
        .engines
        .iter()
        .find(|engine| engine.engine == name)
        .unwrap_or_else(|| panic!("no {name} engine: {:?}", counts.engines))
}

fn decide(project: &str, state: wire::EscapeState) -> wire::EscapeDecide {
    wire::EscapeDecide {
        project: project.to_owned(),
        seen: wire::EscapeSeen { state, gap: None },
        note: None,
    }
}

/// A merge recorded with `engine` in `areas` at `at`, for the cases where the engine has to differ: the rig's own merges are all
/// `alpha`. The task is started the way the product starts one, so the merge joins its project's reads. `n` keeps each start's leased
/// path its own, as the rig's do, so a second start is not a lease conflict with the first.
async fn merge_as(rig: &mut Rig, n: usize, engine: &str, areas: &[&str], at: i64) -> String {
    let (answer, error) = rig
        .w
        .start(
            brief(
                json!({ "files": [{ "path": format!("src/t{n}/**"), "mode": "write" }], "outcome": "Task." }),
            ),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap().to_owned();
    let mut record =
        crate::crew::fixtures::merge_record(&task, &rig.w.repo.to_string_lossy(), "fake-main", "x");
    record.crew_id = rig.w.task(&task).crew_id;
    record.engine = engine.to_owned();
    record.areas = areas.iter().map(|a| (*a).to_owned()).collect();
    record.merged_unix = at;
    rig.w.core.store.add_crew_merge(&record).unwrap();
    task
}

#[tokio::test]
async fn counts_are_one_areas_by_engine_and_possible_stays_apart() {
    let mut rig = rig().await;
    let a = rig.merge(&["web/auth"], NOW - 3 * DAY).await;
    let _b = rig.merge(&["web/auth"], NOW - 2 * DAY).await;
    let _elsewhere = rig.merge(&["docs/guides"], NOW - DAY).await;
    let core = &rig.w.core;

    let counts = core.crew_escape_counts(&rig.project, "web/auth").unwrap();
    assert_eq!(
        (
            counts.project.as_str(),
            counts.area.as_str(),
            counts.merges,
            counts.truncated
        ),
        (rig.project.as_str(), "web/auth", 2, false)
    );
    assert_eq!(
        (counts.window_days, counts.min_merges),
        (60, 10),
        "the log's own stored window and sample, not a guess"
    );
    let engine = engine_counts(&counts, "alpha");
    assert_eq!(
        (
            engine.engine.as_str(),
            engine.merges,
            engine.possible,
            engine.confirmed,
            engine.dismissed
        ),
        ("alpha", 2, 0, 0, 0),
        "two merges in the area, no escape logged yet"
    );
    assert!(engine.low_sample, "two merges are under the sample of ten");
    assert_eq!(counts.engines.len(), 1, "one engine so far");
    assert!(counts.as_of > 0);

    // A revert logs a possible escape against that merge: counted possible, never confirmed, never dismissed.
    rig.revert(&a, NOW);
    let counts = core.crew_escape_counts(&rig.project, "web/auth").unwrap();
    assert_eq!((counts.merges, counts.engines.len()), (2, 1));
    let engine = engine_counts(&counts, "alpha");
    assert_eq!(
        (engine.possible, engine.confirmed, engine.dismissed),
        (1, 0, 0)
    );

    // The other area's merge is its own read, and the escape is not counted there.
    let docs = core
        .crew_escape_counts(&rig.project, "docs/guides")
        .unwrap();
    assert_eq!((docs.merges, docs.engines.len()), (1, 1));
    let engine = engine_counts(&docs, "alpha");
    assert_eq!(
        (engine.merges, engine.possible, engine.confirmed),
        (1, 0, 0)
    );
}

#[tokio::test]
async fn a_confirmed_escape_is_counted_confirmed_and_never_as_possible() {
    let mut rig = rig().await;
    let a = rig.merge(&["web/auth"], NOW - 2 * DAY).await;
    let _b = rig.merge(&["web/auth"], NOW - 3 * DAY).await;
    let core = &rig.w.core;
    rig.revert(&a, NOW);
    let log = core
        .crew_escape_log(&rig.project, Some("possible"), page())
        .unwrap();
    let id = log
        .rows
        .iter()
        .find(|row| row.merge_task_id == a)
        .expect("the reverted merge's possible escape")
        .id
        .clone();
    core.crew_escape_confirm(&id, &decide(&rig.project, wire::EscapeState::Possible))
        .unwrap();

    let counts = core.crew_escape_counts(&rig.project, "web/auth").unwrap();
    let engine = engine_counts(&counts, "alpha");
    assert_eq!(
        (
            engine.merges,
            engine.possible,
            engine.confirmed,
            engine.dismissed
        ),
        (2, 0, 1, 0),
        "a person's confirm moves the escape, it does not add one"
    );
}

#[tokio::test]
async fn an_unknown_project_or_an_area_that_is_not_a_name_is_refused() {
    let rig = rig().await;
    let core = &rig.w.core;
    assert!(matches!(
        core.crew_escape_counts("no-such-project", "web/auth"),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.crew_escape_counts(&rig.project, ""),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        core.crew_escape_counts(&rig.project, &"a".repeat(301)),
        Err(CoreError::Invalid(_))
    ));
    assert!(
        matches!(
            core.crew_escape_counts(&rig.project, "web/\u{1}auth"),
            Err(CoreError::Invalid(_))
        ),
        "a control character is not an area name"
    );
    assert!(
        matches!(
            core.crew_escape_counts("not a project", "web/auth"),
            Err(CoreError::Invalid(_))
        ),
        "a project id that is not a name is refused before the store is asked"
    );
}

#[tokio::test]
async fn a_cut_list_says_it_was_cut_and_keeps_the_busiest_engine() {
    let mut rig = rig().await;
    let engine = "alpha";
    merge_as(&mut rig, 1, engine, &["web/auth"], NOW - 4 * DAY).await;
    merge_as(&mut rig, 2, engine, &["web/auth"], NOW - 3 * DAY).await;
    merge_as(&mut rig, 3, engine, &["web/auth"], NOW - 2 * DAY).await;
    merge_as(&mut rig, 4, "beta", &["web/auth"], NOW - DAY).await;
    let core = &rig.w.core;

    let counts = core.crew_escape_counts(&rig.project, "web/auth").unwrap();
    assert_eq!(
        (counts.engines.len(), counts.truncated),
        (2, false),
        "two engines fit under the bound"
    );
    // The order is the contract ("busiest first"): the engine with three merges is listed before the one with one.
    let order: Vec<(&str, u32)> = counts
        .engines
        .iter()
        .map(|row| (row.engine.as_str(), row.merges))
        .collect();
    assert_eq!(order, [("alpha", 3), ("beta", 1)], "busiest engine first");

    // One row more than the cap: the busiest is kept, the rest is cut and said to be cut, never dropped silently.
    let cut = core
        .crew_escape_counts_capped(&rig.project, "web/auth", 1)
        .unwrap();
    assert_eq!(
        (cut.engines.len(), cut.truncated),
        (1, true),
        "a cut list carries one row and says so"
    );
    let kept = engine_counts(&cut, "alpha");
    assert_eq!(
        (kept.engine.as_str(), kept.merges),
        ("alpha", 3),
        "the engine the area was mostly merged with is kept"
    );
    assert_eq!(
        cut.merges, 4,
        "the area's own total still counts every engine's merge, cut list or not"
    );
}

#[tokio::test]
async fn the_read_writes_nothing_and_reads_the_same_twice() {
    let mut rig = rig().await;
    let a = rig.merge(&["web/auth"], NOW - 2 * DAY).await;
    rig.revert(&a, NOW);
    let core = &rig.w.core;
    let before = core.crew_escape_log(&rig.project, None, page()).unwrap();
    let first = core.crew_escape_counts(&rig.project, "web/auth").unwrap();
    let second = core.crew_escape_counts(&rig.project, "web/auth").unwrap();
    assert_eq!(first, second, "a read answers the same twice");
    let after = core.crew_escape_log(&rig.project, None, page()).unwrap();
    assert_eq!(
        (before.total, before.possible, after.total, after.possible),
        (after.total, after.possible, after.total, after.possible),
        "the log is exactly as it was: a GET never writes"
    );
}

#[tokio::test]
async fn the_preview_flag_hides_the_read() {
    let w = world(false, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    assert!(matches!(
        w.core.crew_escape_counts(&project, "web/auth"),
        Err(CoreError::NotFound(_))
    ));
}
