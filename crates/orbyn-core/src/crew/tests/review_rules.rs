//! The pure parts of review: the rule file, how a finding stands against later rounds, and when two findings are the same place.

use super::review_support::finding;
use crate::crew::review::{FindingStatus, RuleLane, fingerprint, status_of};

/// The test's own rounds, numbered by the place it lists them in: here the index *is* the round number the
/// assertions ask for, so the place is the value rather than a fixture order a test would look up past.
fn rounds(each: &[&[&str]]) -> Vec<(u32, Vec<String>)> {
    each.iter()
        .enumerate()
        .map(|(n, found)| {
            (
                n as u32 + 1,
                found.iter().map(|s| (*s).to_owned()).collect(),
            )
        })
        .collect()
}

/// The rule lane named `lane`: the rule file lists its lanes in its own order, so a test names the lane it means
/// instead of reading the place the file happened to put it in.
fn lane<'a>(rules: &'a [RuleLane], lane: &str) -> &'a RuleLane {
    rules
        .iter()
        .find(|rule| rule.lane.as_str() == lane)
        .unwrap_or_else(|| panic!("the rules name no {lane:?} lane: {rules:?}"))
}

#[test]
fn a_finding_is_open_fixed_regressed_or_superseded_by_the_later_rounds() {
    // Reported, then gone: fixed.
    let r = rounds(&[&["a"], &[]]);
    assert_eq!(status_of(&r, 1, "a"), FindingStatus::Fixed);
    // Reported, and again: the later one is open, the earlier one stands for it.
    let r = rounds(&[&["a"], &["a"]]);
    assert_eq!(status_of(&r, 2, "a"), FindingStatus::Open);
    assert_eq!(status_of(&r, 1, "a"), FindingStatus::Superseded);
    // Reported, gone, back: regressed.
    let r = rounds(&[&["a"], &[], &["a"]]);
    assert_eq!(status_of(&r, 3, "a"), FindingStatus::Regressed);
    assert_eq!(status_of(&r, 1, "a"), FindingStatus::Superseded);
    // A round that has not finished counts for nothing; a lane with no finished round is open.
    assert_eq!(status_of(&[], 1, "a"), FindingStatus::Open);
    assert_eq!(status_of(&rounds(&[&["b"]]), 2, "a"), FindingStatus::Open);
    // Another finding in the later round does not hide this one's fix.
    assert_eq!(
        status_of(&rounds(&[&["a"], &["b"]]), 1, "a"),
        FindingStatus::Fixed
    );
}

#[test]
fn a_fingerprint_ignores_the_line_the_case_and_the_spacing() {
    let a = fingerprint("correctness", "src/lib.txt", "It  wraps on OVERFLOW");
    assert_eq!(
        a,
        fingerprint("correctness", "src/lib.txt", "it wraps on overflow")
    );
    assert_ne!(
        a,
        fingerprint("security", "src/lib.txt", "it wraps on overflow")
    );
    assert_ne!(
        a,
        fingerprint("correctness", "src/other.txt", "it wraps on overflow")
    );
    assert_ne!(a, fingerprint("correctness", "src/lib.txt", "it wraps"));
}

#[test]
fn rules_only_add_lanes_and_a_broken_file_is_an_error() {
    let rules = super::super::review::read_rules(std::path::Path::new("/no/such/dir")).unwrap();
    assert!(rules.is_empty(), "a missing file is no rules");
    let dir = crate::testing::TempDir::new();
    std::fs::create_dir_all(dir.path().join(".orbyn")).unwrap();
    let write = |text: &str| std::fs::write(dir.path().join(".orbyn/crew.json"), text).unwrap();
    write(
        r#"{"lanes":[{"lane":"security","when":{"paths":["src/auth/**"]},"locked":true},{"lane":"docs"}],"checks":[1]}"#,
    );
    let rules = super::super::review::read_rules(dir.path()).unwrap();
    assert_eq!(rules.len(), 2, "the fixture names two lanes");
    let security = lane(&rules, "security");
    let docs = lane(&rules, "docs");
    assert!(security.locked && !docs.locked);
    assert!(security.applies(Some(&["src/auth/login.rs".to_owned()])));
    assert!(!security.applies(Some(&["src/ui/page.rs".to_owned()])));
    assert!(
        security.applies(None),
        "not knowing the paths is never a reason to skip a review"
    );
    assert!(
        docs.applies(Some(&[])),
        "a lane with no condition always runs"
    );
    for bad in [
        "{ nope",
        r#"{"lanes":[{"lane":"Not A Token"}]}"#,
        r#"{"lanes":[{"lane":"x","when":{"paths":["/abs"]}}]}"#,
    ] {
        write(bad);
        assert!(
            super::super::review::read_rules(dir.path()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn two_findings_are_one_place_in_the_same_file_a_few_lines_apart() {
    let make = |path: &str, line: u32| {
        let draft = finding(path, line, "x");
        let stamper = super::fake::stamper();
        let run = orbyn_contracts::CrewId::new("run-01").unwrap();
        let lane = orbyn_contracts::CrewToken::new("correctness").unwrap();
        stamper
            .finding(
                &draft,
                &run,
                &lane,
                "0123456789abcdef0123456789abcdef01234567",
            )
            .unwrap()
    };
    let (a, b) = (make("src/a.rs", 10), make("src/a.rs", 13));
    assert!(crate::crew::review::agrees(&a, &b));
    assert!(!crate::crew::review::agrees(&a, &make("src/a.rs", 14)));
    assert!(!crate::crew::review::agrees(&a, &make("src/b.rs", 10)));
}
