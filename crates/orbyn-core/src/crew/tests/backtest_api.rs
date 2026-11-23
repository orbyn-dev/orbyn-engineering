//! The reviewer backtest's core entry points (slice 4n PR 3) on a real `Core` with a scripted replay: the estimate, a
//! person's start and stop, the report, the one-run-at-a-time rule, Not measured, the startup recovery and that the router
//! never reads any of it. No engine runs and nothing live is connected.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;

use orbyn_contracts as wire;
use serde_json::json;

use super::autopilot_areas::{DAY, Rig, rig};
use super::harness::{Turn, world};
use crate::app::{Core, CoreConfig};
use crate::crew::backtest::{
    Escape, FixtureEscapes, ReplayedFinding, STOPPED_BY_PERSON, ScriptedRunner,
};
use crate::error::CoreError;
use crate::store::{BacktestLaneRow, BacktestState, LaneState, NewLane};
use crate::testing::TempDir;
use crate::util::unix_now;

fn lane(lane: &str, engine: &str) -> wire::BacktestLaneInput {
    wire::BacktestLaneInput {
        lane: lane.into(),
        engine: engine.into(),
        model: format!("{engine}-model"),
        kind: wire::BacktestLaneKind::Review,
    }
}

fn start_input(
    project: &str,
    lanes: Vec<wire::BacktestLaneInput>,
    budgets: &[(&str, u64)],
) -> wire::BacktestStartInput {
    wire::BacktestStartInput {
        project: project.into(),
        lanes,
        budgets: budgets
            .iter()
            .map(|(engine, tokens)| wire::BacktestEngineBudget {
                engine: (*engine).into(),
                tokens: *tokens,
            })
            .collect(),
    }
}

/// `n` merges 40 days old, one 5 days old (too young) and one 200 days old (too old). Their record ids are 1..=n+2.
async fn seed(rig: &mut Rig, n: usize) {
    let now = unix_now();
    for _ in 0..n {
        rig.merge(&["src"], now - 40 * DAY).await;
    }
    rig.merge(&["src"], now - 5 * DAY).await;
    rig.merge(&["src"], now - 200 * DAY).await;
}

fn runs_in_store(core: &Core) -> i64 {
    core.store
        .reader()
        .query_row("SELECT COUNT(*) FROM crew_backtest_runs", [], |r| r.get(0))
        .unwrap()
}

fn found(fingerprint: &str, file: &str) -> ReplayedFinding {
    ReplayedFinding {
        fingerprint: fingerprint.into(),
        files: vec![file.into()],
    }
}

#[tokio::test]
async fn with_the_flag_off_nothing_exists() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let missing = |error: CoreError| assert!(matches!(error, CoreError::NotFound(_)), "{error}");
    missing(w.core.crew_backtest_estimate(&project, &[]).unwrap_err());
    missing(w.core.crew_backtest_report("run-1").unwrap_err());
    missing(w.core.crew_backtest_start_view(&project).unwrap_err());
    missing(w.core.crew_backtest_stop("run-1").unwrap_err());
    missing(
        w.core
            .crew_backtest_start(&start_input(&project, vec![lane("c", "a")], &[("a", 5)]))
            .await
            .unwrap_err(),
    );
    assert_eq!(runs_in_store(&w.core), 0);
}

#[tokio::test]
async fn an_estimate_counts_merges_30_to_180_days_old_and_writes_nothing() {
    let mut rig = rig().await;
    seed(&mut rig, 3).await;
    let core = &rig.w.core;
    let estimate = core
        .crew_backtest_estimate(
            &rig.project,
            &[lane("correctness", "a"), lane("security", "b")],
        )
        .unwrap();
    assert_eq!(
        (
            estimate.history_days,
            estimate.outcome_days,
            estimate.eligible_prs
        ),
        (180, 30, 3)
    );
    assert_eq!(
        estimate.lanes.iter().map(|l| l.replays).collect::<Vec<_>>(),
        [3, 3]
    );
    // The escape log does not exist yet: outcomes are not measured, and nothing is running.
    assert!(!estimate.outcomes_measured);
    assert_eq!(estimate.running, None);
    assert_eq!(runs_in_store(core), 0);
    // No lanes is an estimate of the history alone.
    assert_eq!(
        core.crew_backtest_estimate(&rig.project, &[])
            .unwrap()
            .lanes
            .len(),
        0
    );
    // The names a start would refuse are refused here too, and an unknown project is not found.
    assert!(
        core.crew_backtest_estimate(&rig.project, &[lane("builder", "a")])
            .is_err()
    );
    assert!(
        core.crew_backtest_estimate(&rig.project, &[lane("cost", "a")])
            .is_err()
    );
    assert!(matches!(
        core.crew_backtest_estimate("nope", &[]).unwrap_err(),
        CoreError::NotFound(_)
    ));
}

#[tokio::test]
async fn a_start_with_no_replay_engine_connected_is_refused_and_writes_nothing() {
    let mut rig = rig().await;
    seed(&mut rig, 1).await;
    let error = rig
        .w
        .core
        .crew_backtest_start(&start_input(
            &rig.project,
            vec![lane("c", "a")],
            &[("a", 100)],
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Unsupported(_)), "{error}");
    assert_eq!(runs_in_store(&rig.w.core), 0);
}

#[tokio::test]
async fn a_run_reports_not_measured_and_never_a_zero_and_keeps_each_engines_budget_apart() {
    let mut rig = rig().await;
    seed(&mut rig, 2).await;
    let core = rig.w.core.clone();
    let mut runner = ScriptedRunner::default();
    runner.cost.insert("a".into(), 7);
    runner.cost.insert("b".into(), 3);
    runner.findings.insert(
        ("security".into(), "a".into(), 1),
        vec![found("sha256:f1", "x.rs")],
    );
    core.crew_backtest_use(Arc::new(runner), None);

    let input = start_input(
        &rig.project,
        vec![lane("security", "a"), lane("correctness", "b")],
        &[("a", 100), ("b", 50)],
    );
    let started = core.crew_backtest_start(&input).await.unwrap();
    // The replay goes on in the background and a fake finishes at once, so the answer may already read finished.
    assert!(matches!(
        started.state,
        wire::BacktestState::Running | wire::BacktestState::Finished
    ));
    core.crew_backtest_join().await;

    let run = core.crew_backtest_report(&started.id).unwrap();
    assert_eq!(run.state, wire::BacktestState::Finished);
    assert_eq!(run.stop_reason, None);
    // One budget per engine, as given; there is no total anywhere.
    let budgets: Vec<(&str, u64)> = run
        .budgets
        .iter()
        .map(|b| (b.engine.as_str(), b.tokens))
        .collect();
    assert_eq!(budgets, [("a", 100), ("b", 50)]);
    let security = run.lanes.iter().find(|l| l.lane == "security").unwrap();
    let correctness = run.lanes.iter().find(|l| l.lane == "correctness").unwrap();
    assert_eq!((security.tokens_used, correctness.tokens_used), (14, 6));
    // Not measured: the finding count shows, no score and no escape count of zero.
    for lane in [security, correctness] {
        assert_eq!(lane.state, wire::BacktestLaneState::Done);
        assert_eq!(lane.outcome, wire::BacktestLaneOutcome::NotMeasured);
        assert_eq!(lane.score, None);
    }
    assert_eq!((security.findings_n, correctness.findings_n), (1, 0));
    // No field names a total or money.
    let text = serde_json::to_string(&run).unwrap().to_lowercase();
    for word in ["total", "cost", "usd", "price", "money"] {
        assert!(!text.contains(word), "the view holds {word}: {text}");
    }
    // Reading again gives the same.
    assert_eq!(core.crew_backtest_report(&started.id).unwrap(), run);
}

#[tokio::test]
async fn a_start_view_of_a_project_that_never_ran_a_backtest_has_no_lane_and_no_budget() {
    let rig = rig().await;
    let view = rig.w.core.crew_backtest_start_view(&rig.project).unwrap();
    assert_eq!(view.project, rig.project);
    // Nothing is defaulted for a first run: no lane, no engine and no budget is invented.
    assert_eq!(view.run, None);
    assert!(matches!(
        rig.w.core.crew_backtest_start_view("nope"),
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_start_view_reads_back_the_last_runs_lanes_their_kinds_and_its_budgets() {
    let mut rig = rig().await;
    seed(&mut rig, 1).await;
    let core = rig.w.core.clone();
    core.crew_backtest_use(Arc::new(ScriptedRunner::default()), None);
    let mut check = lane("chk-tests", "b");
    check.kind = wire::BacktestLaneKind::Check;
    let started = core
        .crew_backtest_start(&start_input(
            &rig.project,
            vec![lane("security", "a"), check],
            &[("a", 100), ("b", 40)],
        ))
        .await
        .unwrap();
    core.crew_backtest_join().await;

    let view = core.crew_backtest_start_view(&rig.project).unwrap();
    let run = view.run.as_ref().expect("the run it just had");
    assert_eq!(run.id, started.id);
    assert_eq!(run.state, wire::BacktestState::Finished);
    // Lane then engine order, each with the kind it was started as.
    let lanes: Vec<(&str, &str, &str, Option<wire::BacktestLaneKind>)> = run
        .lanes
        .iter()
        .map(|l| (l.lane.as_str(), l.engine.as_str(), l.model.as_str(), l.kind))
        .collect();
    assert_eq!(
        lanes,
        [
            (
                "chk-tests",
                "b",
                "b-model",
                Some(wire::BacktestLaneKind::Check)
            ),
            (
                "security",
                "a",
                "a-model",
                Some(wire::BacktestLaneKind::Review)
            ),
        ]
    );
    // One budget per engine, each its own and never summed, and no total anywhere.
    let budgets: Vec<(&str, u64)> = run
        .budgets
        .iter()
        .map(|b| (b.engine.as_str(), b.tokens))
        .collect();
    assert_eq!(budgets, [("a", 100), ("b", 40)]);
    let text = serde_json::to_string(&view).unwrap().to_lowercase();
    for word in ["total", "cost", "usd", "price", "money"] {
        assert!(!text.contains(word), "the view holds {word}: {text}");
    }
    // The kind it serves is the one the report serves, so both agree.
    for served in &core.crew_backtest_report(&started.id).unwrap().lanes {
        let from_view = run
            .lanes
            .iter()
            .find(|l| l.lane == served.lane && l.engine == served.engine)
            .unwrap();
        assert_eq!(from_view.kind, served.kind, "{}", served.lane);
    }
}

#[tokio::test]
async fn a_start_view_serves_the_projects_newest_run() {
    let mut rig = rig().await;
    seed(&mut rig, 1).await;
    let core = rig.w.core.clone();
    core.crew_backtest_use(Arc::new(ScriptedRunner::default()), None);
    core.crew_backtest_start(&start_input(
        &rig.project,
        vec![lane("security", "a")],
        &[("a", 10)],
    ))
    .await
    .unwrap();
    core.crew_backtest_join().await;
    let second = core
        .crew_backtest_start(&start_input(
            &rig.project,
            vec![lane("correctness", "b")],
            &[("b", 20)],
        ))
        .await
        .unwrap();
    core.crew_backtest_join().await;
    let view = core.crew_backtest_start_view(&rig.project).unwrap();
    let run = view.run.expect("a run");
    assert_eq!(run.id, second.id, "the project's newest run");
    assert_eq!(run.lanes.len(), 1);
    assert_eq!(run.lanes[0].lane, "correctness");
    assert_eq!(run.lanes[0].kind, Some(wire::BacktestLaneKind::Review));
}

#[tokio::test]
async fn a_measured_log_scores_the_lane() {
    let mut rig = rig().await;
    seed(&mut rig, 1).await;
    let core = rig.w.core.clone();
    let mut runner = ScriptedRunner::default();
    runner.findings.insert(
        ("security".into(), "a".into(), 1),
        vec![found("sha256:f1", "x.rs")],
    );
    let escapes = FixtureEscapes::default().with(
        &rig.project,
        vec![Escape {
            pr: 1,
            changed_files: vec!["./x.rs".into()],
        }],
    );
    core.crew_backtest_use(Arc::new(runner), Some(Arc::new(escapes)));
    let started = core
        .crew_backtest_start(&start_input(
            &rig.project,
            vec![lane("security", "a")],
            &[("a", 10)],
        ))
        .await
        .unwrap();
    core.crew_backtest_join().await;
    let run = core.crew_backtest_report(&started.id).unwrap();
    let lane = &run.lanes[0];
    assert_eq!(lane.outcome, wire::BacktestLaneOutcome::Scored);
    let score = lane.score.as_ref().unwrap();
    // One escape, caught: below 5 escapes and 10 findings only the counts show.
    assert_eq!(
        (score.escapes_n, score.findings_n, score.unmatched_n),
        (1, 1, 0)
    );
    assert!(score.recall.is_none() && score.precision.is_none());
    assert!(
        core.crew_backtest_estimate(&rig.project, &[])
            .unwrap()
            .outcomes_measured
    );
}

#[tokio::test]
async fn a_second_start_while_one_runs_is_refused_and_a_stop_ends_the_run_as_stopped() {
    let mut rig = rig().await;
    seed(&mut rig, 3).await;
    let core = rig.w.core.clone();
    let (release, hold) = mpsc::channel();
    let (began, saw) = mpsc::channel();
    let runner = Arc::new(ScriptedRunner {
        hold: Some(Mutex::new(hold)),
        began: Some(Mutex::new(began)),
        ..ScriptedRunner::default()
    });
    core.crew_backtest_use(runner.clone(), None);
    let input = start_input(&rig.project, vec![lane("security", "a")], &[("a", 100)]);
    let started = core.crew_backtest_start(&input).await.unwrap();
    // The first replay has begun and is held: the run is in progress.
    assert_eq!(saw.recv().unwrap(), 1);
    assert_eq!(
        core.crew_backtest_estimate(&rig.project, &[])
            .unwrap()
            .running
            .as_deref(),
        Some(started.id.as_str())
    );

    let busy = core.crew_backtest_start(&input).await.unwrap_err();
    assert!(matches!(busy, CoreError::CrewTaskBusy(_)), "{busy}");
    assert!(busy.to_string().contains("already running"));
    assert_eq!(runs_in_store(&core), 1, "the refused start wrote nothing");

    // A stop only asks: the run is still running until its replay is over, so no other run can start meanwhile.
    let asked = core.crew_backtest_stop(&started.id).unwrap();
    assert_eq!(asked.state, wire::BacktestState::Running);
    assert_eq!(asked.stop_reason.as_deref(), Some(STOPPED_BY_PERSON));
    assert!(matches!(
        core.crew_backtest_start(&input).await.unwrap_err(),
        CoreError::CrewTaskBusy(_)
    ));
    release.send(()).unwrap();
    core.crew_backtest_join().await;

    let run = core.crew_backtest_report(&started.id).unwrap();
    assert_eq!(run.state, wire::BacktestState::Stopped);
    assert_eq!(run.stop_reason.as_deref(), Some(STOPPED_BY_PERSON));
    assert_eq!(run.lanes[0].state, wire::BacktestLaneState::Stopped);
    // It replayed the one pull request it was on and no other.
    assert_eq!(runner.calls.lock().unwrap().len(), 1);
    // A stop on a run that ended changes nothing, and another run may start now.
    assert_eq!(core.crew_backtest_stop(&started.id).unwrap(), run);
    drop(release);
    core.crew_backtest_start(&input).await.unwrap();
    core.crew_backtest_join().await;
}

#[tokio::test]
async fn a_request_that_is_not_a_review_or_that_hides_money_or_a_budget_is_refused_before_anything_is_written()
 {
    let mut rig = rig().await;
    seed(&mut rig, 1).await;
    let core = rig.w.core.clone();
    core.crew_backtest_use(Arc::new(ScriptedRunner::default()), None);
    let p = rig.project.clone();
    let refused = [
        start_input(&p, vec![lane("builder", "a")], &[("a", 10)]),
        start_input(&p, vec![lane("implementer", "a")], &[("a", 10)]),
        start_input(&p, vec![lane("c", "usd")], &[("usd", 10)]),
        start_input(&p, vec![lane("c", "a")], &[("a", 10), ("price", 3)]),
        start_input(&p, vec![lane("c", "a")], &[("a", 0)]),
        start_input(&p, vec![lane("c", "a")], &[]),
        start_input(&p, vec![lane("c", "a")], &[("b", 10)]),
        start_input(&p, vec![], &[("a", 10)]),
        // One engine named twice would become one budget: refused, never merged.
        start_input(&p, vec![lane("c", "a")], &[("a", 10), ("a", 20)]),
    ];
    for input in &refused {
        assert!(core.crew_backtest_start(input).await.is_err(), "{input:?}");
    }
    assert!(matches!(
        core.crew_backtest_start(&start_input("nope", vec![lane("c", "a")], &[("a", 10)]))
            .await
            .unwrap_err(),
        CoreError::NotFound(_)
    ));
    assert_eq!(runs_in_store(&core), 0);
}

fn config(dir: &TempDir) -> CoreConfig {
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.claude_program = "orbyn-test-no-such-claude".into();
    config.engines.codex_program = "orbyn-test-no-such-codex".into();
    config.engines.known_engine_ids = Some(Vec::new());
    config
}

#[tokio::test]
async fn a_run_left_running_by_a_crash_becomes_interrupted_when_the_core_opens() {
    let dir = TempDir::new();
    let budgets: BTreeMap<String, u64> = [("a".to_owned(), 100)].into();
    let lanes = vec![NewLane {
        lane: "security".to_owned(),
        engine: "a".to_owned(),
        model: "m".to_owned(),
        kind: crate::crew::backtest::LaneKind::Review,
    }];
    {
        let core = Core::open(config(&dir)).unwrap();
        let project = crate::crew::fixtures::add_project(&core, "/project");
        let store = &core.store;
        // One run the crash left running, with a lane mid-replay, one that finished, and one a person had stopped.
        store
            .crew_backtest_create(
                "crashed",
                &project,
                BacktestState::Running,
                &budgets,
                &lanes,
                10,
            )
            .unwrap();
        let mut row = BacktestLaneRow {
            lane: "security".into(),
            engine: "a".into(),
            model: "m".into(),
            kind: Some(crate::crew::backtest::LaneKind::Review),
            tokens_used: 40,
            state: LaneState::Running,
            escapes_n: None,
            caught_n: None,
            unresolved_n: None,
        };
        store.crew_backtest_save_lane("crashed", &row, &[]).unwrap();
        store
            .crew_backtest_create(
                "done",
                &project,
                BacktestState::Finished,
                &budgets,
                &lanes,
                5,
            )
            .unwrap();
        store
            .crew_backtest_create(
                "stopped",
                &project,
                BacktestState::Running,
                &budgets,
                &lanes,
                6,
            )
            .unwrap();
        store
            .crew_backtest_set_state(
                "stopped",
                BacktestState::Stopped,
                Some("token budget spent: a"),
                Some(7),
            )
            .unwrap();
        row.state = LaneState::Done;
        store.crew_backtest_save_lane("done", &row, &[]).unwrap();
        assert_eq!(
            store.crew_backtest_running().unwrap().as_deref(),
            Some("crashed")
        );
    }
    // The process is gone (the owner lock is released with the core). A new one opens the same data folder.
    let core = Core::open(config(&dir)).unwrap();
    let crashed = core.store.crew_backtest_run("crashed").unwrap();
    assert_eq!(crashed.state, BacktestState::Failed);
    assert_eq!(crashed.stop_reason.as_deref(), Some("interrupted"));
    assert!(crashed.finished_unix.is_some());
    let lanes = core.store.crew_backtest_lanes("crashed").unwrap();
    assert_eq!(lanes[0].state, LaneState::Failed);
    assert_eq!(lanes[0].tokens_used, 40);
    // Runs that had ended are left as they were, and nothing is running, so a new run may start.
    let done = core.store.crew_backtest_run("done").unwrap();
    assert_eq!(
        (done.state, done.stop_reason),
        (BacktestState::Finished, None)
    );
    let stopped = core.store.crew_backtest_run("stopped").unwrap();
    assert_eq!(stopped.state, BacktestState::Stopped);
    assert_eq!(
        stopped.stop_reason.as_deref(),
        Some("token budget spent: a")
    );
    assert_eq!(core.store.crew_backtest_running().unwrap(), None);
}

#[tokio::test]
async fn a_finished_backtest_does_not_change_the_routers_choice() {
    // The real dry run (`crew_route`, the path a start takes) before and after a run whose scores would be strongly in one
    // engine's favour. The lane picker may show the counts; Auto never reads them.
    let mut rig = rig().await;
    seed(&mut rig, 2).await;
    let core = rig.w.core.clone();
    let ask = || async {
        let (answer, error) = rig
            .w
            .call(
                &rig.w.lead_token,
                "crew_route",
                json!({ "brief": brief_json() }),
            )
            .await;
        assert!(!error, "{answer}");
        (
            answer["decision"]["engine"].clone(),
            answer["decision"]["scores"].clone(),
        )
    };
    let before = ask().await;

    let mut runner = ScriptedRunner::default();
    runner.findings.insert(
        ("security".into(), "alpha".into(), 1),
        vec![found("sha256:f1", "x.rs")],
    );
    let escapes = FixtureEscapes::default().with(
        &rig.project,
        vec![Escape {
            pr: 1,
            changed_files: vec!["x.rs".into()],
        }],
    );
    core.crew_backtest_use(Arc::new(runner), Some(Arc::new(escapes)));
    let started = core
        .crew_backtest_start(&start_input(
            &rig.project,
            vec![lane("security", "alpha")],
            &[("alpha", 100)],
        ))
        .await
        .unwrap();
    core.crew_backtest_join().await;
    let run = core.crew_backtest_report(&started.id).unwrap();
    assert_eq!(run.lanes[0].outcome, wire::BacktestLaneOutcome::Scored);

    assert_eq!(ask().await, before);
}

fn brief_json() -> serde_json::Value {
    super::harness::brief(json!({}))
}

#[test]
fn no_source_the_routing_reads_names_the_backtest() {
    // A source guard: the files that decide a route never mention the backtest.
    let word = ["back", "test"].concat();
    for file in [
        "router.rs",
        "score.rs",
        "tiers.rs",
        "filters.rs",
        "view.rs",
        "start.rs",
        "tool.rs",
        "tier_settings.rs",
    ] {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/crew")
                .join(file),
        )
        .unwrap();
        assert!(
            !text.to_lowercase().contains(&word),
            "{file} reads the backtest: a backtest must not change the router's choice"
        );
    }
}
