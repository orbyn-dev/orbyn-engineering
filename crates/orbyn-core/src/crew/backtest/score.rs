//! Scoring a lane against the escapes. Pure, and all integers: a rate is a numerator and a denominator (the sample
//! size) with the permille rounded down. A finding that matches no escape is "unmatched", never "wrong": the log
//! may simply not know about a defect it found.

use std::collections::BTreeMap;

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
        let permille = u64::from(numerator) * 1000 / u64::from(denominator.max(1));
        Self {
            numerator,
            denominator,
            permille: permille as u32,
        }
    }
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

/// Paths compare with `/` separators, so a path from Windows tooling matches the same file from git.
fn same_file(a: &str, b: &str) -> bool {
    a.replace('\\', "/") == b.replace('\\', "/")
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
pub fn rates(findings: &[PastFinding], escapes: &[Escape]) -> LaneScore {
    let escapes_n = escapes.len() as u32;
    let findings_n = findings.len() as u32;
    let caught = escapes
        .iter()
        .filter(|escape| match_escape(findings, escape) == Outcome::Caught)
        .count() as u32;
    let matched = findings
        .iter()
        .filter(|finding| {
            escapes
                .iter()
                .any(|escape| finding_matches(finding, escape))
        })
        .count() as u32;
    LaneScore {
        recall: (escapes_n >= RECALL_MIN_ESCAPES).then(|| Rate::of(caught, escapes_n)),
        precision: (findings_n >= PRECISION_MIN_FINDINGS).then(|| Rate::of(matched, findings_n)),
        escapes_n,
        findings_n,
        unmatched_n: findings_n - matched,
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
            finding("correctness", 4, "d.rs"),
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
    fn permille_rounds_down() {
        assert_eq!(Rate::of(1, 3).permille, 333);
        assert_eq!(Rate::of(2, 3).permille, 666);
    }
}
