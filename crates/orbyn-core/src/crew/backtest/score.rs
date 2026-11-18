//! Scoring a lane against the escapes. Pure, and all integers: a rate is a numerator and a denominator (the sample
//! size) with the permille rounded down. A finding that matches no escape is "unmatched", never "wrong": the log
//! may simply not know about a defect it found.

use std::collections::{BTreeMap, BTreeSet};

use super::outcomes::Escape;

/// A lane's recall shows only from this many escapes.
pub const RECALL_MIN_ESCAPES: u32 = 5;
/// A lane's precision shows only from this many findings.
pub const PRECISION_MIN_FINDINGS: u32 = 10;

/// A finding a lane raised on a past pull request: what `crew_findings` and `crew_reviews` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastFinding {
    pub pr: u64,
    pub lane: String,
    pub engine: String,
    pub model: String,
    /// The files the finding is about.
    pub files: Vec<String>,
}

/// How one escape fared against one lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Caught,
    Unmatched,
}

/// A share with its sample size, so a screen can say "3 of 5".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub numerator: u32,
    pub denominator: u32,
    /// `numerator * 1000 / denominator`, rounded down.
    pub permille: u32,
}

impl Rate {
    fn of(numerator: u32, denominator: u32) -> Self {
        // A share is never above the whole: a numerator past the denominator is a bug upstream and shows as 1000.
        let permille = u64::from(numerator) * 1000 / u64::from(denominator.max(1));
        Self {
            numerator,
            denominator,
            permille: u32::try_from(permille).map_or(1000, |p| p.min(1000)),
        }
    }
}

/// A count of `usize` as the `u32` a score keeps. A count above `u32::MAX` shows as `u32::MAX`: it never wraps to a small
/// number, which would read as a better or worse lane than it is.
pub(super) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// One lane's counts, and its rates once there are enough of them. Below the minimum only the counts show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneScore {
    /// Escapes the lane caught, over all escapes. `None` below 5 escapes.
    pub recall: Option<Rate>,
    /// Findings that matched an escape, over all findings on eligible pull requests. `None` below 10 findings.
    pub precision: Option<Rate>,
    pub escapes_n: u32,
    pub findings_n: u32,
    /// Findings that matched no escape. Counted, never called wrong.
    pub unmatched_n: u32,
}

/// A path as it compares: `/` separators, no empty or `.` segments, `..` resolved against the segment before it, and a
/// Windows drive letter in lower case. So `./src//a.rs`, `src\\a.rs` and `src/x/../a.rs` are all `src/a.rs`. A `..` that
/// climbs above the start is kept, so `../a.rs` never equals `a.rs`. An absolute path stays absolute (a leading `/` or a
/// drive): the repository root is not known here, so it matches only the same absolute path, never a relative one. A path
/// with nothing left (`""`, `.`, `./`) is `None` and matches nothing.
fn normalise(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let (root, rest) = match path.as_bytes() {
        [drive, b':', ..] if drive.is_ascii_alphabetic() => (
            format!("{}:/", char::from(*drive).to_ascii_lowercase()),
            path[2..].trim_start_matches('/'),
        ),
        [b'/', ..] => ("/".to_owned(), path.trim_start_matches('/')),
        _ => (String::new(), path.as_str()),
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last() {
                Some(&last) if last != ".." => {
                    parts.pop();
                }
                // Above the root of an absolute path there is nothing: stay at the root.
                _ if !root.is_empty() => {}
                _ => parts.push(".."),
            },
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("{root}{}", parts.join("/")))
}

fn same_file(a: &str, b: &str) -> bool {
    match (normalise(a), normalise(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn finding_matches(finding: &PastFinding, escape: &Escape) -> bool {
    // Behaviour matching is undefined: how a finding about behaviour matches a fix that changed none of the files it
    // names has not been decided, so a finding matches an escape by file only.
    finding.pr == escape.pr
        && finding.files.iter().any(|file| {
            escape
                .changed_files
                .iter()
                .any(|changed| same_file(file, changed))
        })
}

/// Whether a lane's findings caught one escape: it raised a finding on that pull request about a file the later
/// revert or fix changed. Anything else is `Unmatched`. The caller passes one lane's findings.
pub fn match_escape(findings: &[PastFinding], escape: &Escape) -> Outcome {
    if findings
        .iter()
        .any(|finding| finding_matches(finding, escape))
    {
        Outcome::Caught
    } else {
        Outcome::Unmatched
    }
}

/// One lane's score. `findings` are that lane's findings on eligible pull requests and `escapes` those of the same
/// pull requests.
///
/// Identical findings count once: the same pull request, lane, engine, model and set of files (after normalising each
/// path) is one finding however often it was raised, so a repeat can neither raise precision nor lower it.
pub fn rates(findings: &[PastFinding], escapes: &[Escape]) -> LaneScore {
    let findings = distinct(findings);
    let escapes_n = count(escapes.len());
    let findings_n = count(findings.len());
    let caught = count(
        escapes
            .iter()
            .filter(|escape| match_escape(&findings, escape) == Outcome::Caught)
            .count(),
    );
    let matched = count(
        findings
            .iter()
            .filter(|finding| {
                escapes
                    .iter()
                    .any(|escape| finding_matches(finding, escape))
            })
            .count(),
    );
    from_counts(escapes_n, caught, findings_n, matched)
}

/// The findings with identical ones (see `rates`) dropped, in the order first seen.
fn distinct(findings: &[PastFinding]) -> Vec<PastFinding> {
    let mut seen = BTreeSet::new();
    findings
        .iter()
        .filter(|finding| {
            let files: BTreeSet<Option<String>> =
                finding.files.iter().map(|file| normalise(file)).collect();
            seen.insert((
                finding.pr,
                finding.lane.clone(),
                finding.engine.clone(),
                finding.model.clone(),
                files,
            ))
        })
        .cloned()
        .collect()
}

/// A lane's score from its counts: `caught` escapes of `escapes_n`, and `matched` findings of `findings_n`. What a
/// stored run keeps, so a re-read gives the score `rates` gave when the run was scored.
pub fn from_counts(escapes_n: u32, caught: u32, findings_n: u32, matched: u32) -> LaneScore {
    LaneScore {
        recall: (escapes_n >= RECALL_MIN_ESCAPES).then(|| Rate::of(caught, escapes_n)),
        precision: (findings_n >= PRECISION_MIN_FINDINGS).then(|| Rate::of(matched, findings_n)),
        escapes_n,
        findings_n,
        unmatched_n: findings_n.saturating_sub(matched),
    }
}

/// Each named lane scored on its own findings. A lane with no findings is still listed.
pub fn score_lanes(
    lanes: &[&str],
    findings: &[PastFinding],
    escapes: &[Escape],
) -> BTreeMap<String, LaneScore> {
    lanes
        .iter()
        .map(|lane| {
            let own: Vec<PastFinding> = findings
                .iter()
                .filter(|finding| finding.lane == *lane)
                .cloned()
                .collect();
            ((*lane).to_owned(), rates(&own, escapes))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(lane: &str, pr: u64, file: &str) -> PastFinding {
        PastFinding {
            pr,
            lane: lane.to_owned(),
            engine: "engine-a".to_owned(),
            model: "model-a".to_owned(),
            files: vec![file.to_owned()],
        }
    }

    fn escape(pr: u64, file: &str) -> Escape {
        Escape {
            pr,
            changed_files: vec![file.to_owned()],
        }
    }

    /// Five escapes: PR 1..=5, each fixed in one file (a.rs .. e.rs).
    fn five_escapes() -> Vec<Escape> {
        vec![
            escape(1, "a.rs"),
            escape(2, "b.rs"),
            escape(3, "c.rs"),
            escape(4, "d.rs"),
            escape(5, "e.rs"),
        ]
    }

    /// Ten `correctness` findings:
    /// 1 PR1 a.rs (matches), 2 PR2 b.rs (matches), 3 PR3 x.rs (the fix changed c.rs: unmatched),
    /// 4 PR4 d.rs and 5 PR4 d.rs (both match the same escape), 6..=10 PR6..=PR10 f.rs (no escape there: unmatched).
    fn correctness() -> Vec<PastFinding> {
        let mut all = vec![
            finding("correctness", 1, "a.rs"),
            finding("correctness", 2, "b.rs"),
            finding("correctness", 3, "x.rs"),
            finding("correctness", 4, "d.rs"),
            // Another finding on the same escape, about d.rs and a second file: not a repeat of the one above.
            PastFinding {
                files: vec!["d.rs".to_owned(), "d2.rs".to_owned()],
                ..finding("correctness", 4, "d.rs")
            },
        ];
        all.extend((6..=10).map(|pr| finding("correctness", pr, "f.rs")));
        all
    }

    #[test]
    fn a_finding_on_the_pr_about_a_changed_file_catches_the_escape() {
        let findings = [finding("correctness", 1, "a.rs")];
        assert_eq!(match_escape(&findings, &escape(1, "a.rs")), Outcome::Caught);
    }

    #[test]
    fn a_fix_that_changed_a_different_file_does_not_match() {
        let findings = [finding("correctness", 3, "x.rs")];
        assert_eq!(
            match_escape(&findings, &escape(3, "c.rs")),
            Outcome::Unmatched
        );
    }

    #[test]
    fn a_finding_on_another_pr_does_not_match() {
        let findings = [finding("correctness", 2, "a.rs")];
        assert_eq!(
            match_escape(&findings, &escape(1, "a.rs")),
            Outcome::Unmatched
        );
    }

    #[test]
    fn a_windows_path_matches_the_same_file() {
        let findings = [finding("correctness", 1, "src\\a.rs")];
        assert_eq!(
            match_escape(&findings, &escape(1, "src/a.rs")),
            Outcome::Caught
        );
    }

    #[test]
    fn no_findings_catch_nothing() {
        assert_eq!(match_escape(&[], &escape(1, "a.rs")), Outcome::Unmatched);
    }

    #[test]
    fn recall_and_precision_follow_the_hand_worked_fixture() {
        // Escapes caught: PR1, PR2, PR4 = 3 of 5 -> recall 600 permille (PR3 and PR5 are not caught).
        // Findings matched: 1, 2, 4, 5 = 4 of 10 -> precision 400 permille; unmatched: 3, 6..=10 = 6.
        let score = rates(&correctness(), &five_escapes());
        assert_eq!(
            score,
            LaneScore {
                recall: Some(Rate {
                    numerator: 3,
                    denominator: 5,
                    permille: 600
                }),
                precision: Some(Rate {
                    numerator: 4,
                    denominator: 10,
                    permille: 400
                }),
                escapes_n: 5,
                findings_n: 10,
                unmatched_n: 6,
            }
        );
    }

    #[test]
    fn recall_shows_from_exactly_five_escapes_and_not_at_four() {
        let findings = [finding("correctness", 1, "a.rs")];
        let four = &five_escapes()[..4];
        let at_four = rates(&findings, four);
        assert_eq!((at_four.recall, at_four.escapes_n), (None, 4));
        let at_five = rates(&findings, &five_escapes());
        // 1 of 5 caught -> 200 permille.
        assert_eq!(at_five.recall.map(|r| r.permille), Some(200));
    }

    #[test]
    fn precision_shows_from_exactly_ten_findings_and_not_at_nine() {
        let ten = correctness();
        let at_nine = rates(&ten[..9], &five_escapes());
        // 9 findings: matched are 1, 2, 4, 5 -> still 4, unmatched 5, and no rate.
        assert_eq!(
            (at_nine.precision, at_nine.findings_n, at_nine.unmatched_n),
            (None, 9, 5)
        );
        assert_eq!(
            rates(&ten, &five_escapes()).precision.map(|r| r.permille),
            Some(400)
        );
    }

    #[test]
    fn a_lane_with_no_findings_has_zero_recall_and_no_precision() {
        // 0 of 5 caught -> 0 permille; no findings, so no precision and nothing unmatched.
        let score = rates(&[], &five_escapes());
        assert_eq!(
            score.recall.map(|r| (r.numerator, r.permille)),
            Some((0, 0))
        );
        assert_eq!(
            (score.precision, score.findings_n, score.unmatched_n),
            (None, 0, 0)
        );
    }

    #[test]
    fn with_no_escapes_every_finding_is_unmatched_and_no_rate_shows() {
        let score = rates(&correctness(), &[]);
        assert_eq!(score.recall, None);
        assert_eq!(score.precision.map(|r| r.permille), Some(0));
        assert_eq!((score.escapes_n, score.unmatched_n), (0, 10));
    }

    #[test]
    fn two_lanes_are_scored_independently() {
        // security: PR5 e.rs and PR1 a.rs match -> 2 of 5 escapes = 400 permille; 2 findings, so no precision.
        // style: no findings -> 0 of 5 = 0 permille. correctness is the fixture above.
        let mut findings = correctness();
        findings.push(finding("security", 5, "e.rs"));
        findings.push(finding("security", 1, "a.rs"));
        let scores = score_lanes(
            &["correctness", "security", "style"],
            &findings,
            &five_escapes(),
        );
        assert_eq!(scores["correctness"].recall.map(|r| r.permille), Some(600));
        assert_eq!(scores["security"].recall.map(|r| r.permille), Some(400));
        assert_eq!(scores["security"].precision, None);
        assert_eq!(scores["security"].unmatched_n, 0);
        assert_eq!(scores["style"].recall.map(|r| r.permille), Some(0));
        assert_eq!(scores["style"].findings_n, 0);
    }

    #[test]
    fn identical_findings_count_once_for_precision() {
        // Ten distinct findings (PR 6..=15, no escape) plus the same finding on PR 1 raised three times: 11 findings,
        // one of which matches. Counting the repeats would give 3 of 13.
        let mut findings: Vec<PastFinding> = (6..=15)
            .map(|pr| finding("correctness", pr, "f.rs"))
            .collect();
        findings.extend((0..3).map(|_| finding("correctness", 1, "a.rs")));
        let score = rates(&findings, &five_escapes());
        assert_eq!((score.findings_n, score.unmatched_n), (11, 10));
        assert_eq!(
            score.precision,
            Some(Rate {
                numerator: 1,
                denominator: 11,
                permille: 90
            })
        );
        // The same finding spelled with another path form is the same finding.
        let spelled = [
            finding("correctness", 1, "a.rs"),
            finding("correctness", 1, "./a.rs"),
            finding("correctness", 1, "src/../a.rs"),
        ];
        assert_eq!(rates(&spelled, &five_escapes()).findings_n, 1);
    }

    #[test]
    fn findings_that_differ_in_pr_lane_or_files_are_not_duplicates() {
        let findings = [
            finding("correctness", 1, "a.rs"),
            finding("correctness", 2, "a.rs"),
            finding("security", 1, "a.rs"),
            finding("correctness", 1, "b.rs"),
        ];
        assert_eq!(rates(&findings, &[]).findings_n, 4);
    }

    #[test]
    fn paths_match_across_dot_segments_separators_and_dot_dot() {
        for (finding_path, changed) in [
            ("./a.rs", "a.rs"),
            ("a.rs", "./a.rs"),
            ("src//a.rs", "src/a.rs"),
            ("src/./a.rs", "src/a.rs"),
            ("src/x/../a.rs", "src/a.rs"),
            ("src\\x\\..\\a.rs", "./src/a.rs"),
            ("C:\\repo\\a.rs", "c:/repo/a.rs"),
            ("/repo/a.rs", "/repo//a.rs"),
            ("/../repo/a.rs", "/repo/a.rs"),
        ] {
            assert_eq!(
                match_escape(&[finding("c", 1, finding_path)], &escape(1, changed)),
                Outcome::Caught,
                "{finding_path} against {changed}"
            );
        }
    }

    #[test]
    fn paths_that_name_different_files_never_match() {
        for (finding_path, changed) in [
            ("../a.rs", "a.rs"),
            ("a.rs", "../a.rs"),
            ("../../a.rs", "../a.rs"),
            // An absolute path is not the relative one: the repository root is not known.
            ("/repo/a.rs", "a.rs"),
            ("a.rs", "/repo/a.rs"),
            ("src/a.rs", "a.rs"),
            ("A.rs", "a.rs"),
            // Nothing left of the path matches nothing, not even itself.
            ("", ""),
            (".", "./"),
            ("src/..", "."),
        ] {
            assert_eq!(
                match_escape(&[finding("c", 1, finding_path)], &escape(1, changed)),
                Outcome::Unmatched,
                "{finding_path} against {changed}"
            );
        }
    }

    #[test]
    fn a_count_past_u32_max_saturates_and_never_wraps() {
        assert_eq!(count(0), 0);
        assert_eq!(count(7), 7);
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(count(u32::MAX as usize), u32::MAX);
            assert_eq!(count(u32::MAX as usize + 1), u32::MAX);
            assert_eq!(count(usize::MAX), u32::MAX);
        }
    }

    #[test]
    fn a_share_is_never_above_the_whole() {
        assert_eq!(Rate::of(5, 3).permille, 1000);
        assert_eq!(Rate::of(u32::MAX, 1).permille, 1000);
        assert_eq!(Rate::of(u32::MAX, u32::MAX).permille, 1000);
        assert_eq!(Rate::of(0, 0).permille, 0);
    }

    #[test]
    fn permille_rounds_down() {
        assert_eq!(Rate::of(1, 3).permille, 333);
        assert_eq!(Rate::of(2, 3).permille, 666);
    }
}
