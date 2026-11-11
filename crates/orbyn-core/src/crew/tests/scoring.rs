//! Scores: pass rate for the brief's kind, then quota headroom, then the person's default
//! engine as a tie-break only. Every number is an integer in permille.

use orbyn_contracts::{BriefKind, PassRate};
use serde_json::json;

use super::fake::*;
use crate::crew::SEED_PASS_PERMILLE;

fn score_of(
    decision: &orbyn_contracts::RouteDecision,
    engine: &str,
) -> orbyn_contracts::RouteScore {
    decision
        .scores
        .iter()
        .find(|s| s.engine.as_str() == engine)
        .cloned()
        .unwrap()
}

#[test]
fn with_no_results_every_engine_scores_the_seeded_default_and_says_so() {
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), engine("codex")],
        &Ask::default(),
    );
    for score in &decision.scores {
        assert_eq!(score.score, SEED_PASS_PERMILLE);
        assert!(score.used_defaults);
        assert!(score.pass_rate.is_none());
    }
    assert!(
        decision
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "seeded_defaults")
    );
    verified(&decision);
}

#[test]
fn more_headroom_ranks_higher_and_the_alternatives_follow_in_order() {
    let mut busy = engine("codex");
    busy.used_percent = Some(80);
    let mut easy = engine("claude");
    easy.used_percent = Some(20);
    let mut idle = engine("zed");
    idle.used_percent = Some(0);
    let decision = routed(&brief(json!({})), &[busy, easy, idle], &Ask::default());
    // 500 pass * 7 + headroom * 3, over 10.
    assert_eq!(score_of(&decision, "zed").score, 650);
    assert_eq!(score_of(&decision, "claude").score, 590);
    assert_eq!(score_of(&decision, "codex").score, 410);
    assert_eq!(decision.engine.as_str(), "zed");
    let ranked: Vec<&str> = decision.scores.iter().map(|s| s.engine.as_str()).collect();
    assert_eq!(ranked, ["zed", "claude", "codex"]);
}

#[test]
fn an_engine_that_reports_no_usage_is_neither_helped_nor_hurt() {
    let mut half = engine("claude");
    half.used_percent = Some(50);
    let decision = routed(&brief(json!({})), &[half, engine("codex")], &Ask::default());
    assert_eq!(
        score_of(&decision, "claude").score,
        score_of(&decision, "codex").score
    );
}

#[test]
fn a_measured_rate_replaces_the_default_and_carries_its_sample_size() {
    let mut history = crate::crew::EvalHistory::default();
    history.record(
        "codex",
        BriefKind::Implement,
        PassRate {
            permille: 900,
            samples: 12,
        },
    );
    history.record(
        "claude",
        BriefKind::Implement,
        PassRate {
            permille: 300,
            samples: 20,
        },
    );
    let ask = Ask {
        history,
        ..Ask::default()
    };
    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), engine("codex")],
        &ask,
    );
    assert_eq!(decision.engine.as_str(), "codex");
    let codex = score_of(&decision, "codex");
    assert!(!codex.used_defaults);
    assert_eq!(
        codex.pass_rate,
        Some(PassRate {
            permille: 900,
            samples: 12
        })
    );
    assert_eq!(codex.score, (900 * 7 + 500 * 3) / 10);
    assert!(
        decision
            .reasons
            .iter()
            .any(|r| r.text.as_str().contains("measured, 12 samples"))
    );
    verified(&decision);

    // The rate is for a kind: a docs brief gets the default again.
    let docs = routed(
        &brief(json!({ "kind": "docs" })),
        &[engine("claude"), engine("codex")],
        &ask,
    );
    assert!(docs.scores.iter().all(|s| s.used_defaults));
}

#[test]
fn the_default_engine_breaks_a_tie_and_adds_nothing() {
    let engines = [engine("claude"), engine("codex")];
    let plain = routed(&brief(json!({})), &engines, &Ask::default());
    assert_eq!(
        plain.engine.as_str(),
        "claude",
        "ties fall back to the engine id"
    );
    let preferred = routed(
        &brief(json!({})),
        &engines,
        &Ask {
            default_engine: Some("codex"),
            ..Ask::default()
        },
    );
    assert_eq!(preferred.engine.as_str(), "codex");
    assert!(
        preferred
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "default_engine_tiebreak")
    );
    assert_eq!(
        score_of(&preferred, "codex").score,
        score_of(&preferred, "claude").score,
        "the default engine earns no points"
    );

    // A real score gap beats the default.
    let mut worn = engine("codex");
    worn.used_percent = Some(90);
    let beaten = routed(
        &brief(json!({})),
        &[engine("claude"), worn],
        &Ask {
            default_engine: Some("codex"),
            ..Ask::default()
        },
    );
    assert_eq!(beaten.engine.as_str(), "claude");
    assert!(
        !beaten
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "default_engine_tiebreak")
    );
}

#[test]
fn effort_is_medium_and_the_decision_is_deterministic() {
    let engines = [engine("codex"), engine("claude")];
    let first = routed(&brief(json!({})), &engines, &Ask::default());
    let second = routed(
        &brief(json!({})),
        &[engine("claude"), engine("codex")],
        &Ask::default(),
    );
    assert_eq!(first.effort, orbyn_contracts::RouteEffort::Medium);
    assert_eq!(
        first.id, second.id,
        "input order does not change the decision"
    );
    assert!(
        first
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "effort_medium")
    );
}

#[test]
fn many_engines_still_fit_the_protocol_limits() {
    let engines: Vec<_> = (0..40).map(|n| engine(&format!("zed-{n:02}"))).collect();
    let decision = routed(&brief(json!({})), &engines, &Ask::default());
    assert_eq!(decision.scores.len(), 40);
    assert!(decision.reasons.len() <= 32);
    verified(&decision);
}

#[test]
fn the_most_used_window_counts_and_only_a_full_one_is_exhausted() {
    let limits = |used: &[f64]| orbyn_contracts::EngineLimits {
        engine: "codex".into(),
        checked_at: "2026-11-12T09:00:00Z".into(),
        limits: serde_json::Value::Null,
        windows: used
            .iter()
            .map(|u| orbyn_contracts::LimitWindow {
                id: "w".into(),
                label: "5h".into(),
                used_percent: *u,
                window_minutes: None,
                resets_at: None,
            })
            .collect(),
        plan: None,
    };
    let view = |used: &[f64]| engine("codex").with_limits(&limits(used));
    assert_eq!(view(&[10.0, 40.5]).used_percent, Some(40));
    assert_eq!(view(&[99.9]).used_percent, Some(99));
    assert_eq!(view(&[100.0, 20.0]).used_percent, Some(100));
    assert_eq!(view(&[250.0]).used_percent, Some(100));
    assert_eq!(view(&[]).used_percent, None);
}
