//! The backtest replay on a fake runner: per-engine budgets, lane isolation, stored results, Not measured, a
//! re-read, the request's refusals and the guard that only a person's start can reach `run`. No engine runs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::replay::FakeLaneRunner;
use super::*;
use crate::store::{
    BacktestState, LaneState, MOST_BUDGET_TOKENS, NewProject, ResultOutcome, Store,
};
use crate::testing::TempDir;

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

fn open() -> (TempDir, Store, String) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    let project = store
        .add_project(NewProject {
            path: "/repo/a",
            name: "a",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id;
    (dir, store, project)
}

/// Pull requests 1..=n merged 40 days ago, plus one too young (900) and one too old (901).
fn prs(n: u64) -> Vec<PastPr> {
    let mut all: Vec<PastPr> = (1..=n)
        .map(|number| PastPr {
            number,
            merged_unix: NOW - 40 * DAY,
        })
        .collect();
    all.push(PastPr {
        number: 900,
        merged_unix: NOW - 5 * DAY,
    });
    all.push(PastPr {
        number: 901,
        merged_unix: NOW - 400 * DAY,
    });
    all
}

fn spec(lane: &str, engine: &str) -> LaneSpec {
    LaneSpec {
        lane: lane.into(),
        engine: engine.into(),
        model: format!("{engine}-model"),
        kind: LaneKind::Review,
    }
}

fn request(project: &str, lanes: Vec<LaneSpec>, budgets: &[(&str, u64)]) -> BacktestRequest {
    BacktestRequest {
        project: project.into(),
        lanes,
        budgets: budgets.iter().map(|(e, t)| ((*e).into(), *t)).collect(),
    }
}

fn found(fp: &str, file: &str) -> ReplayedFinding {
    ReplayedFinding {
        fingerprint: fp.into(),
        files: vec![file.into()],
    }
}

fn runner(cost: &[(&str, u64)]) -> FakeLaneRunner {
    FakeLaneRunner {
        cost: cost.iter().map(|(e, t)| ((*e).into(), *t)).collect(),
        ..FakeLaneRunner::default()
    }
}

fn lane_of<'a>(report: &'a BacktestReport, lane: &str, engine: &str) -> &'a LaneReport {
    report
        .lanes
        .iter()
        .find(|l| l.lane == lane && l.engine == engine)
        .unwrap()
}

fn calls_of(runner: &FakeLaneRunner, lane: &str, engine: &str) -> Vec<u64> {
    runner
        .calls
        .borrow()
        .iter()
        .filter(|(l, e, _)| l == lane && e == engine)
        .map(|(_, _, pr)| *pr)
        .collect()
}

/// The fixture is keyed by the project id; tests that need a measured log build it with the real id.
fn measured_for(project: &str, escapes: Vec<Escape>) -> FixtureEscapes {
    FixtureEscapes::default().with(project, escapes)
}

#[test]
fn an_engine_stops_when_its_own_budget_is_spent_and_the_other_goes_on() {
    let (_d, store, project) = open();
    // A pays 100 a replay with a budget of 250; B pays 10 a replay with a budget of 250. Five pull requests.
    let fake = runner(&[("engine-a", 100), ("engine-b", 10)]);
    let req = request(
        &project,
        vec![
            spec("correctness", "engine-a"),
            spec("security", "engine-b"),
        ],
        &[("engine-a", 250), ("engine-b", 250)],
    );
    let report = run(&store, "run-1", &req, &prs(5), NOW, &fake, &NoEscapeLog).unwrap();

    let a = lane_of(&report, "correctness", "engine-a");
    let b = lane_of(&report, "security", "engine-b");
    assert_eq!((a.state, a.tokens_used), (LaneState::Stopped, 300));
    assert_eq!((b.state, b.tokens_used), (LaneState::Done, 50));
    assert_eq!(calls_of(&fake, "correctness", "engine-a"), vec![1, 2, 3]);
    assert_eq!(calls_of(&fake, "security", "engine-b"), vec![1, 2, 3, 4, 5]);
    assert_eq!(report.run.state, BacktestState::Stopped);
    assert_eq!(
        report.run.stop_reason.as_deref(),
        Some("token budget spent: engine-a")
    );
    assert_eq!(report.run.finished_unix, Some(NOW));
}

#[test]
fn budgets_are_never_summed_or_pooled_across_engines() {
    let (_d, store, project) = open();
    // A has a tiny budget and B a huge one. Pooled, B's room would keep A running; apart, A stops after one replay.
    let fake = runner(&[("engine-a", 100), ("engine-b", 100)]);
    let req = request(
        &project,
        vec![
            spec("correctness", "engine-a"),
            spec("correctness", "engine-b"),
        ],
        &[("engine-a", 100), ("engine-b", 1_000_000)],
    );
    let report = run(&store, "run-1", &req, &prs(4), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(calls_of(&fake, "correctness", "engine-a"), vec![1]);
    assert_eq!(calls_of(&fake, "correctness", "engine-b"), vec![1, 2, 3, 4]);
    assert_eq!(
        lane_of(&report, "correctness", "engine-a").state,
        LaneState::Stopped
    );
    assert_eq!(
        lane_of(&report, "correctness", "engine-b").state,
        LaneState::Done
    );
    // The stored budgets are exactly what was asked, one per engine.
    assert_eq!(
        store.crew_backtest_run("run-1").unwrap().budgets,
        BTreeMap::from([
            ("engine-a".to_owned(), 100),
            ("engine-b".to_owned(), 1_000_000)
        ])
    );
}

#[test]
fn lanes_of_one_engine_share_that_engines_budget() {
    let (_d, store, project) = open();
    let fake = runner(&[("engine-a", 100)]);
    let req = request(
        &project,
        vec![
            spec("correctness", "engine-a"),
            spec("security", "engine-a"),
        ],
        &[("engine-a", 200)],
    );
    let report = run(&store, "run-1", &req, &prs(5), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(calls_of(&fake, "correctness", "engine-a"), vec![1, 2]);
    assert!(calls_of(&fake, "security", "engine-a").is_empty());
    assert_eq!(
        lane_of(&report, "security", "engine-a").state,
        LaneState::Stopped
    );
}

#[test]
fn a_lane_stopped_before_its_first_replay_is_not_scored_as_zero_escapes() {
    let (_d, store, project) = open();
    let fake = runner(&[("engine-a", 100)]);
    let req = request(
        &project,
        vec![
            spec("correctness", "engine-a"),
            spec("security", "engine-a"),
        ],
        &[("engine-a", 100)],
    );
    let escapes = measured_for(
        &project,
        vec![Escape {
            pr: 1,
            changed_files: vec!["a.rs".into()],
        }],
    );
    let report = run(&store, "run-1", &req, &prs(3), NOW, &fake, &escapes).unwrap();
    let never = lane_of(&report, "security", "engine-a");
    assert_eq!(never.state, LaneState::Stopped);
    assert_eq!(never.result, LaneResult::NotMeasured { findings_n: 0 });
    let stored = store
        .crew_backtest_lanes("run-1")
        .unwrap()
        .into_iter()
        .find(|l| l.lane == "security")
        .unwrap();
    assert_eq!((stored.escapes_n, stored.caught_n), (None, None));
    // The lane that did replay is scored.
    assert!(matches!(
        lane_of(&report, "correctness", "engine-a").result,
        LaneResult::Scored(_)
    ));
}

#[test]
fn a_budget_spent_exactly_on_the_last_pull_request_is_done_not_stopped() {
    let (_d, store, project) = open();
    let fake = runner(&[("engine-a", 100)]);
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 300)],
    );
    let report = run(&store, "run-1", &req, &prs(3), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(report.lanes[0].state, LaneState::Done);
    assert_eq!(report.run.state, BacktestState::Finished);
    assert_eq!(report.run.stop_reason, None);
}

#[test]
fn only_eligible_pull_requests_are_replayed() {
    let (_d, store, project) = open();
    let fake = runner(&[]);
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10)],
    );
    run(&store, "run-1", &req, &prs(2), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(calls_of(&fake, "correctness", "engine-a"), vec![1, 2]);
}

#[test]
fn a_failed_lane_is_recorded_and_the_others_carry_on() {
    let (_d, store, project) = open();
    let mut fake = runner(&[("engine-a", 5), ("engine-b", 5)]);
    fake.broken
        .insert(("correctness".to_owned(), "engine-a".to_owned()));
    fake.findings.insert(
        ("security".to_owned(), "engine-b".to_owned(), 2),
        vec![found("sha256:s1", "x.rs")],
    );
    let req = request(
        &project,
        vec![
            spec("correctness", "engine-a"),
            spec("security", "engine-b"),
        ],
        &[("engine-a", 1000), ("engine-b", 1000)],
    );
    let report = run(&store, "run-1", &req, &prs(3), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(
        lane_of(&report, "correctness", "engine-a").state,
        LaneState::Failed
    );
    assert_eq!(
        lane_of(&report, "correctness", "engine-a").result,
        LaneResult::Unscored { findings_n: 0 }
    );
    let b = lane_of(&report, "security", "engine-b");
    assert_eq!((b.state, b.tokens_used), (LaneState::Done, 15));
    assert_eq!(calls_of(&fake, "security", "engine-b"), vec![1, 2, 3]);
    assert_eq!(report.run.state, BacktestState::Finished);
}

#[test]
fn a_run_where_every_lane_failed_is_failed() {
    let (_d, store, project) = open();
    let mut fake = runner(&[]);
    fake.broken
        .insert(("correctness".to_owned(), "engine-a".to_owned()));
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10)],
    );
    let report = run(&store, "run-1", &req, &prs(2), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(report.run.state, BacktestState::Failed);
}

#[test]
fn findings_are_stored_with_their_outcome_and_scored_with_the_pure_score() {
    let (_d, store, project) = open();
    let mut fake = runner(&[("engine-a", 1)]);
    for (pr, fp, file) in [
        (1, "sha256:f1", "a.rs"),
        (2, "sha256:f2", "x.rs"),
        (3, "sha256:f3", "c.rs"),
    ] {
        fake.findings.insert(
            ("correctness".to_owned(), "engine-a".to_owned(), pr),
            vec![found(fp, file), found(fp, file)],
        );
    }
    // PR1: fix changed a.rs (caught). PR2: fix changed b.rs (the finding was about x.rs: unmatched). PR3: no escape.
    let escapes = vec![
        Escape {
            pr: 1,
            changed_files: vec!["a.rs".into()],
        },
        Escape {
            pr: 2,
            changed_files: vec!["b.rs".into()],
        },
    ];
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 100)],
    );
    let report = run(
        &store,
        "run-1",
        &req,
        &prs(3),
        NOW,
        &fake,
        &measured_for(&project, escapes.clone()),
    )
    .unwrap();

    let outcomes: Vec<(u64, ResultOutcome)> = store
        .crew_backtest_results("run-1")
        .unwrap()
        .into_iter()
        .map(|r| (r.pr, r.outcome))
        .collect();
    // A fingerprint raised twice on one pull request is one finding.
    assert_eq!(
        outcomes,
        vec![
            (1, ResultOutcome::Caught),
            (2, ResultOutcome::Unmatched),
            (3, ResultOutcome::NoEscape)
        ]
    );
    let past: Vec<PastFinding> = [(1, "a.rs"), (2, "x.rs"), (3, "c.rs")]
        .iter()
        .map(|(pr, file)| PastFinding {
            pr: *pr,
            lane: "correctness".into(),
            engine: "engine-a".into(),
            model: "engine-a-model".into(),
            files: vec![(*file).into()],
        })
        .collect();
    assert_eq!(
        lane_of(&report, "correctness", "engine-a").result,
        LaneResult::Scored(rates(&past, &escapes))
    );
    let LaneResult::Scored(score) = &lane_of(&report, "correctness", "engine-a").result else {
        unreachable!()
    };
    // Two escapes and three findings are below the minimums: counts only.
    assert_eq!(
        (score.escapes_n, score.findings_n, score.unmatched_n),
        (2, 3, 2)
    );
    assert!(score.recall.is_none() && score.precision.is_none());
}

#[test]
fn a_stopped_lane_is_scored_only_on_the_pull_requests_it_replayed() {
    let (_d, store, project) = open();
    let mut fake = runner(&[("engine-a", 100)]);
    fake.findings.insert(
        ("correctness".to_owned(), "engine-a".to_owned(), 1),
        vec![found("sha256:f1", "a.rs")],
    );
    let escapes = vec![
        Escape {
            pr: 1,
            changed_files: vec!["a.rs".into()],
        },
        Escape {
            pr: 5,
            changed_files: vec!["e.rs".into()],
        },
    ];
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 100)],
    );
    let report = run(
        &store,
        "run-1",
        &req,
        &prs(5),
        NOW,
        &fake,
        &measured_for(&project, escapes),
    )
    .unwrap();
    let LaneResult::Scored(score) = &lane_of(&report, "correctness", "engine-a").result else {
        panic!("scored")
    };
    // Only PR 1 was replayed, so only its escape counts against the lane.
    assert_eq!((score.escapes_n, score.findings_n), (1, 1));
}

#[test]
fn when_the_escape_log_cannot_say_the_run_finishes_with_counts_only() {
    let (_d, store, project) = open();
    let mut fake = runner(&[("engine-a", 1)]);
    for pr in 1..=12 {
        fake.findings.insert(
            ("correctness".to_owned(), "engine-a".to_owned(), pr),
            vec![found(&format!("sha256:f{pr}"), "a.rs")],
        );
    }
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 1000)],
    );
    let report = run(&store, "run-1", &req, &prs(12), NOW, &fake, &NoEscapeLog).unwrap();
    assert_eq!(report.run.state, BacktestState::Finished);
    // Twelve findings would show a precision if zero escapes were assumed; Not measured shows the count only.
    assert_eq!(
        report.lanes[0].result,
        LaneResult::NotMeasured { findings_n: 12 }
    );
    let stored = &store.crew_backtest_lanes("run-1").unwrap()[0];
    assert_eq!((stored.escapes_n, stored.caught_n), (None, None));
    // No finding is stored as a miss: the log could not judge it.
    let outcomes: BTreeSet<ResultOutcome> = store
        .crew_backtest_results("run-1")
        .unwrap()
        .into_iter()
        .map(|r| r.outcome)
        .collect();
    assert_eq!(outcomes, BTreeSet::from([ResultOutcome::NotMeasured]));
    // Reading again turns nothing into an unmatched finding or a precision.
    assert_eq!(
        super::replay::report(&store, "run-1").unwrap().lanes[0].result,
        LaneResult::NotMeasured { findings_n: 12 }
    );
}

/// A runner that panics, as a broken live runner might.
struct PanickingRunner;

impl LaneRunner for PanickingRunner {
    fn replay(&self, _: &LaneSpec, _: &PastPr) -> Result<LaneReplay, ReplayFailed> {
        panic!("runner broke");
    }
}

#[test]
fn a_runner_panic_ends_the_run_as_failed_and_never_leaves_it_running() {
    let (_d, store, project) = open();
    let req = request(
        &project,
        vec![
            spec("correctness", "engine-a"),
            spec("security", "engine-b"),
        ],
        &[("engine-a", 10), ("engine-b", 10)],
    );
    assert!(
        run(
            &store,
            "run-1",
            &req,
            &prs(2),
            NOW,
            &PanickingRunner,
            &NoEscapeLog
        )
        .is_err()
    );
    let stored = store.crew_backtest_run("run-1").unwrap();
    assert_eq!(stored.state, BacktestState::Failed);
    assert!(stored.stop_reason.is_some() && stored.finished_unix == Some(NOW));
    assert!(
        store
            .crew_backtest_lanes("run-1")
            .unwrap()
            .iter()
            .all(|l| l.state == LaneState::Failed)
    );
}

#[test]
fn a_run_that_cannot_be_written_ends_failed() {
    let (_d, store, project) = open();
    let fake = runner(&[]);
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10)],
    );
    // An escape log that fails the store write: pull request numbers beyond what the database holds.
    let mut fake = fake;
    fake.findings.insert(
        ("correctness".to_owned(), "engine-a".to_owned(), u64::MAX),
        vec![found("sha256:f1", "a.rs")],
    );
    let huge = [PastPr {
        number: u64::MAX,
        merged_unix: NOW - 40 * DAY,
    }];
    assert!(run(&store, "run-1", &req, &huge, NOW, &fake, &NoEscapeLog).is_err());
    let stored = store.crew_backtest_run("run-1").unwrap();
    assert_eq!(stored.state, BacktestState::Failed);
    assert!(
        store
            .crew_backtest_lanes("run-1")
            .unwrap()
            .iter()
            .all(|l| l.state == LaneState::Failed)
    );
}

#[test]
fn budgets_outside_one_to_the_maximum_are_refused_up_front() {
    for bad in [0, MOST_BUDGET_TOKENS + 1, u64::MAX, i64::MAX as u64] {
        let req = request(
            "p",
            vec![spec("correctness", "engine-a")],
            &[("engine-a", bad)],
        );
        assert!(req.validate().is_err(), "{bad}");
        // An engine no lane uses is held to the same range.
        let req = request(
            "p",
            vec![spec("correctness", "engine-a")],
            &[("engine-a", 10), ("engine-b", bad)],
        );
        assert!(req.validate().is_err(), "unused {bad}");
    }
    let ok = request(
        "p",
        vec![spec("correctness", "engine-a")],
        &[("engine-a", MOST_BUDGET_TOKENS)],
    );
    assert!(ok.validate().is_ok());
}

#[test]
fn a_measured_log_with_no_escapes_is_not_the_same_as_not_measured() {
    let (_d, store, project) = open();
    let fake = runner(&[]);
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10)],
    );
    let report = run(
        &store,
        "run-1",
        &req,
        &prs(2),
        NOW,
        &fake,
        &measured_for(&project, vec![]),
    )
    .unwrap();
    assert!(matches!(
        report.lanes[0].result,
        LaneResult::Scored(LaneScore { escapes_n: 0, .. })
    ));
}

#[test]
fn reading_a_run_again_gives_the_same_report() {
    let (_d, store, project) = open();
    let mut fake = runner(&[("engine-a", 7)]);
    fake.findings.insert(
        ("correctness".to_owned(), "engine-a".to_owned(), 1),
        vec![found("sha256:f1", "a.rs")],
    );
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 100)],
    );
    let escapes = measured_for(
        &project,
        vec![Escape {
            pr: 1,
            changed_files: vec!["a.rs".into()],
        }],
    );
    let first = run(&store, "run-1", &req, &prs(2), NOW, &fake, &escapes).unwrap();
    assert_eq!(report(&store, "run-1").unwrap(), first);
    assert_eq!(report(&store, "run-1").unwrap(), first);
    assert_eq!(store.crew_backtest_results("run-1").unwrap().len(), 1);
}

#[test]
fn a_run_id_is_used_once_and_a_second_start_changes_nothing() {
    let (_d, store, project) = open();
    let fake = runner(&[]);
    let req = request(
        &project,
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10)],
    );
    let first = run(&store, "run-1", &req, &prs(2), NOW, &fake, &NoEscapeLog).unwrap();
    assert!(run(&store, "run-1", &req, &prs(2), NOW + 1, &fake, &NoEscapeLog).is_err());
    assert_eq!(report(&store, "run-1").unwrap(), first);
}

#[test]
fn an_unknown_project_is_refused() {
    let (_d, store, _project) = open();
    let fake = runner(&[]);
    let req = request(
        "nope",
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10)],
    );
    assert!(run(&store, "run-1", &req, &prs(1), NOW, &fake, &NoEscapeLog).is_err());
    assert!(fake.calls.borrow().is_empty());
}

fn valid_json() -> String {
    r#"{"project":"p","lanes":[{"lane":"correctness","engine":"engine-a","model":"m","kind":"review"}],"budgets":{"engine-a":1000}}"#.into()
}

#[test]
fn a_valid_request_reads_from_json() {
    let req = BacktestRequest::from_json(&valid_json()).unwrap();
    assert_eq!(req.lanes[0].kind, LaneKind::Review);
    assert_eq!(req.budgets["engine-a"], 1000);
}

#[test]
fn a_builder_lane_is_refused_by_type_and_by_name() {
    let by_kind = valid_json().replace("\"review\"", "\"builder\"");
    assert!(BacktestRequest::from_json(&by_kind).is_err());
    for name in [
        "builder",
        "Builder-2",
        "implementer",
        "code_coder",
        "author",
    ] {
        let req = request("p", vec![spec(name, "engine-a")], &[("engine-a", 10)]);
        assert!(req.validate().is_err(), "{name}");
    }
    // A check named build, and a lane that only contains the letters, are fine.
    for name in ["build", "authorization", "decoder-safety", "correctness"] {
        let req = request("p", vec![spec(name, "engine-a")], &[("engine-a", 10)]);
        assert!(req.validate().is_ok(), "{name}");
    }
}

#[test]
fn a_money_field_is_refused() {
    for field in ["max_cost_usd", "price", "budget_usd", "currency"] {
        let text = valid_json().replacen('{', &format!("{{\"{field}\":5,"), 1);
        assert!(BacktestRequest::from_json(&text).is_err(), "{field}");
    }
    let in_lane = valid_json().replace("\"kind\"", "\"cost\":1,\"kind\"");
    assert!(BacktestRequest::from_json(&in_lane).is_err());
    // A budget keyed by a currency is money in disguise.
    let req = request(
        "p",
        vec![spec("correctness", "engine-a")],
        &[("engine-a", 10), ("usd", 5)],
    );
    assert!(req.validate().is_err());
    let req = request("p", vec![spec("correctness", "usd")], &[("usd", 10)]);
    assert!(req.validate().is_err());
}

#[test]
fn a_request_without_lanes_or_a_budget_for_each_engine_is_refused() {
    assert!(
        request("p", vec![], &[("engine-a", 10)])
            .validate()
            .is_err()
    );
    assert!(
        request("p", vec![spec("correctness", "engine-a")], &[])
            .validate()
            .is_err()
    );
    assert!(
        request(
            "p",
            vec![spec("correctness", "engine-a")],
            &[("engine-a", 0)]
        )
        .validate()
        .is_err()
    );
    assert!(
        request(
            "p",
            vec![
                spec("correctness", "engine-a"),
                spec("correctness", "engine-a")
            ],
            &[("engine-a", 10)]
        )
        .validate()
        .is_err()
    );
    assert!(
        request(
            "p",
            vec![spec("correctness", "engine-a")],
            &[("engine-b", 10)]
        )
        .validate()
        .is_err()
    );
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "target" {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

/// The lines of a source that are code: no `//` comment lines.
fn code_of(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn only_a_persons_start_can_reach_the_replay_there_is_no_timer_and_no_other_caller() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "the scan found no sources");

    // Any mention of the backtest outside its own module and store, however it is imported (`backtest::run`,
    // `use crate::crew::{backtest}`, a glob, a re-export), is a possible caller. Each mention must be one of the places
    // below, line by line: the wire types, the route table and the one route file (transport only), the marker lists,
    // the module declarations, the store's registration, and the core's own slot and startup recovery (`app.rs`).
    // Built in parts so this file never matches itself.
    let word = ["back", "test"].concat();
    for file in &files {
        let name = file.to_string_lossy().replace('\\', "/");
        let is_test = name.ends_with("_tests.rs") || name.contains("/tests/");
        let inside = name.contains("/crew/backtest/") || name.contains("/store/crew_backtest");
        if is_test || inside {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        for line in text.lines().filter(|l| l.to_lowercase().contains(&word)) {
            let line = line.trim();
            let declaration = (name.ends_with("/crew/mod.rs")
                && line == format!("pub mod {word};"))
                || (name.ends_with("/routes/mod.rs")
                    && (line == format!("mod crew_{word};")
                        || line == format!("mod crew_{word}_tests;")))
                || (name.ends_with("/orbyn-contracts/src/lib.rs")
                    && line.contains(&format!("crew_{word}")));
            let registration = (name.ends_with("/store/mod.rs")
                || name.ends_with("/store/migrations.rs"))
                && (line.contains(&format!("crew_{word}")) || line.starts_with("Backtest"))
                || (name.ends_with("/store/mod.rs") && line.contains("MOST_BUDGET_TOKENS"));
            let wire_types = name.ends_with("/orbyn-contracts/src/crew_backtest.rs");
            let core_slot =
                name.ends_with("/orbyn-core/src/app.rs") && line.contains(&format!("crew_{word}"));
            let markers =
                name.ends_with("/crew/changes.rs") && line.contains(&format!("/api/crew/{word}/"));
            let route_table = name.ends_with("/routes/crew.rs")
                && (line.contains(&format!("/api/crew/{word}/"))
                    || line.contains(&format!("crew_{word}::"))
                    || line.contains(&format!("core crew_{word}_")));
            let route_file = name.ends_with("/routes/crew_backtest.rs");
            assert!(
                declaration
                    || registration
                    || wire_types
                    || core_slot
                    || markers
                    || route_table
                    || route_file,
                "{name} mentions the backtest: `{line}`; only a person's start (the routes) may reach it"
            );
        }
        // The start is called by the route file and nowhere else (outside the tests).
        if !name.ends_with("/routes/crew_backtest.rs") {
            assert!(
                !text.contains(&format!("crew_{word}_start(")),
                "{name} calls the start; only the person's route may"
            );
        }
    }

    // The replay itself schedules and spawns nothing.
    let replay = code_of(&read("src/crew/backtest/replay.rs"));
    for word in [
        "tokio",
        "spawn",
        "interval",
        "sleep",
        "cron",
        "Command::new",
    ] {
        assert!(!replay.contains(word), "replay.rs mentions {word}");
    }

    // The core's entry point has no timer either, runs one background task, and reaches `begin` and `finish` once each,
    // from the start and from nowhere else in the module.
    let api = code_of(&read("src/crew/backtest/api.rs"));
    for word in ["interval", "sleep", "cron", "Command::new", "Instant"] {
        assert!(!api.contains(word), "api.rs mentions {word}");
    }
    assert_eq!(api.matches("spawn_blocking(").count(), 1);
    assert_eq!(api.matches("begin(&").count(), 1);
    assert_eq!(api.matches("finish(").count(), 1);
    let start = api.find("fn crew_backtest_start").unwrap();
    let after = &api[start..];
    let end = after.find("pub fn crew_backtest_stop").unwrap();
    assert!(after[..end].contains("begin(&") && after[..end].contains("finish("));
    for other in ["history.rs", "outcomes.rs", "score.rs", "mod.rs"] {
        let text = code_of(&read(&format!("src/crew/backtest/{other}")));
        assert!(
            !text.contains("begin(") && !text.contains("finish("),
            "{other} reaches the replay"
        );
    }
}
