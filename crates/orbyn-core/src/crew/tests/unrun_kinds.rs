//! The check kinds this version does not run (#793): each is an accepted limit, recorded with what
//! it needs and the decision that has to come first, and a skip carries that sentence instead of
//! reporting `skipped` with nothing beside it. `docs/crew.md`, "Check kinds this version does not
//! run", holds the same sentences, and the last test compares the two.

use std::path::Path;
use std::time::{Duration, Instant};

use orbyn_contracts::{CheckOutcome, CrewCheck};
use serde_json::{Value, json};

use crate::crew::checks::{Site, run_check};
use crate::testing::TempDir;

fn check(kind: &str, params: Value) -> CrewCheck {
    serde_json::from_value(json!({
        "id": "chk-limit",
        "kind": kind,
        "proves": "what this kind is meant to prove",
        "params": params
    }))
    .unwrap()
}

/// The six kinds this version does not run, each with the `params` its own shape takes and a word
/// its sentence has to name: the missing piece, not a generic apology.
fn unrun_kinds() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        (
            "preview",
            json!({ "url": "http://127.0.0.1:1/" }),
            "browser engine",
        ),
        (
            "visual_compare",
            json!({ "frame": "pr-list", "tolerance_permille": 20 }),
            "browser engine",
        ),
        (
            "screenshot_only",
            json!({ "url": "http://127.0.0.1:1/" }),
            "browser engine",
        ),
        (
            "accessibility_audit",
            json!({ "url": "http://127.0.0.1:1/" }),
            "accessibility bar",
        ),
        (
            "performance_budget",
            json!({ "metric": "lcp_ms", "max": 2500 }),
            "metric",
        ),
        ("licence_scan", json!({}), "allowed licences"),
    ]
}

fn site_of(root: &Path) -> Site<'_> {
    Site {
        worktree: root,
        deadline: Instant::now() + Duration::from_secs(60),
        base: None,
    }
}

/// A kind this version does not run still reports `skipped`, and now says what it needs and which
/// decision it waits on: the reason, the log the Evidence keeps, and the not-verified line all
/// carry that sentence.
#[tokio::test]
async fn every_kind_this_version_does_not_run_records_the_limit_it_waits_on() {
    let dir = TempDir::new();
    let site = site_of(dir.path());
    for (kind, params, needle) in unrun_kinds() {
        let ran = run_check(&check(kind, params), &site).await;
        assert_eq!(ran.outcome, CheckOutcome::Skipped, "{kind}: {ran:?}");
        let reason = ran.reason.clone().unwrap_or_default();
        assert!(
            !reason.trim().is_empty(),
            "{kind} was skipped with no reason"
        );
        assert_ne!(
            reason, "not run by this version",
            "{kind} was skipped without saying why"
        );
        assert!(
            reason.to_lowercase().contains("decision"),
            "{kind} does not name the decision it waits on: {reason}"
        );
        assert!(
            reason.contains(needle),
            "{kind} does not name what it needs ({needle}): {reason}"
        );
        // The Evidence excerpt is the log, so the record says why and not only the reason field.
        assert_eq!(ran.log, reason, "{kind}");
        // The not-verified line of the result comes from `command`, so it says why too.
        assert!(
            ran.command.contains(&reason),
            "{kind} drops the limit from its result line: {}",
            ran.command
        );
        assert!(
            ran.command.starts_with(&format!("{kind}: ")),
            "{kind}: {}",
            ran.command
        );
    }
}

/// `manual` is not one of the limits: a person decides it in the verifier, so a runner that is
/// asked for one says that, and it is never listed as a kind waiting for an owner's policy.
#[tokio::test]
async fn a_manual_check_names_the_person_who_decides_it() {
    let dir = TempDir::new();
    let ran = run_check(
        &check("manual", json!({ "name": "a person looks at it" })),
        &site_of(dir.path()),
    )
    .await;
    assert_eq!(ran.outcome, CheckOutcome::Skipped, "{ran:?}");
    let reason = ran.reason.clone().unwrap_or_default();
    assert!(reason.contains("person"), "{reason}");
    assert!(!reason.to_lowercase().contains("decision on"), "{reason}");
}

/// The sentences the runner reports are the ones `docs/crew.md` records, so the accepted limit and
/// the behaviour cannot drift: a new limit, a reworded one, or one removed from the doc is a
/// failure here.
#[tokio::test]
async fn the_documented_limits_are_the_ones_a_skip_reports() {
    let documented = documented_limits();
    assert_eq!(
        documented.len(),
        6,
        "docs/crew.md records {} limits: {documented:?}",
        documented.len()
    );
    let dir = TempDir::new();
    let site = site_of(dir.path());
    let mut reported: Vec<(String, String)> = Vec::new();
    for (kind, params, _) in unrun_kinds() {
        let ran = run_check(&check(kind, params), &site).await;
        assert_eq!(ran.outcome, CheckOutcome::Skipped, "{ran:?}");
        reported.push((kind.to_owned(), ran.reason.unwrap_or_default()));
    }
    reported.sort();
    assert_eq!(reported, documented, "the doc and the runner disagree");
}

/// The `kind: sentence` rows of docs/crew.md's "Check kinds this version does not run" list, sorted.
fn documented_limits() -> Vec<(String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/crew.md");
    let doc = std::fs::read_to_string(&path).expect("docs/crew.md is readable");
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut inside = false;
    for line in doc.lines() {
        if line.contains("Check kinds this version does not run") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        // The rows are one per line, `    - `kind`: sentence`; the next bullet of the outer list
        // ends the section.
        let Some(rest) = line.strip_prefix("    - `") else {
            if !rows.is_empty() {
                break;
            }
            continue;
        };
        let (kind, why) = rest
            .split_once("`: ")
            .unwrap_or_else(|| panic!("a kind and its limit in one line: {line}"));
        rows.push((kind.to_owned(), why.to_owned()));
    }
    rows.sort();
    rows
}

/// The six kinds a skip can name, so a check built from any other kind never reaches this list.
#[test]
fn the_unrun_list_holds_the_six_kinds_of_the_issue() {
    let kinds: Vec<&str> = unrun_kinds().iter().map(|(kind, _, _)| *kind).collect();
    assert_eq!(
        kinds,
        [
            "preview",
            "visual_compare",
            "screenshot_only",
            "accessibility_audit",
            "performance_budget",
            "licence_scan"
        ]
    );
    // None of them is a kind this version runs: `http_probe` has its runner and `manual` is a
    // person's decision, so neither is an accepted limit.
    assert!(!kinds.contains(&"http_probe"));
    assert!(!kinds.contains(&"manual"));
}
