//! Review coverage: how many of the lines Crew's agents wrote in merges a person has reviewed. Pure: the merges, their
//! stored hunks and the time provenance began come in, a count goes out.
//!
//! Review here is **merge-level**: a hunk is reviewed when its merge was marked Looks good or Commented. Nothing says a
//! person read a given line, and no count here claims it. A merge from before provenance began, one with no stored hunk
//! and one whose hunks carry no note are "Not measured": they are counted as merges and add no line.

use std::collections::{BTreeMap, BTreeSet};

use crate::crew::autopilot::{Risk, area_of};
use crate::store::{MergeRecord, StoredHunk};

use super::Author;

/// One merge with its stored hunks.
pub struct MergeProvenance<'a> {
    pub merge: &'a MergeRecord,
    pub hunks: &'a [StoredHunk],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AreaCoverage {
    pub area: String,
    pub merges: u32,
    pub agent_lines: u32,
    pub reviewed_lines: u32,
    pub in_queue_lines: u32,
    pub not_measured_merges: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineCoverage {
    pub engine: String,
    pub model: Option<String>,
    pub agent_lines: u32,
    pub reviewed_lines: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoverageMap {
    /// The earliest time provenance began for the repositories involved. Absent when it never has.
    pub began_unix: Option<i64>,
    /// Merges with measured lines.
    pub merges: u32,
    pub not_measured_merges: u32,
    pub agent_lines: u32,
    pub reviewed_lines: u32,
    pub in_queue_lines: u32,
    pub areas: Vec<AreaCoverage>,
    pub engines: Vec<EngineCoverage>,
}

/// What a merge's review state means for its lines.
enum Standing {
    Reviewed,
    InQueue,
    Unreviewed,
}

fn standing(merge: &MergeRecord) -> Option<Standing> {
    match merge.review_state.as_str() {
        "looks_good" | "commented" => Some(Standing::Reviewed),
        "waiting" if matches!(Risk::parse(&merge.risk), Some(Risk::High | Risk::Medium)) => {
            Some(Standing::InQueue)
        }
        // A reverted merge's lines are no longer on the branch.
        "reverted" => None,
        _ => Some(Standing::Unreviewed),
    }
}

fn add(total: &mut u32, n: u32) {
    *total = total.saturating_add(n);
}

/// The coverage of `merges` merged at or after `since` (unix seconds, 0 for all). `began` is the time provenance began
/// for each repository, keyed by the merge's `repo`.
pub fn coverage(
    merges: &[MergeProvenance<'_>],
    began: &BTreeMap<String, i64>,
    since: i64,
) -> CoverageMap {
    let mut map = CoverageMap {
        began_unix: began.values().min().copied(),
        ..CoverageMap::default()
    };
    let mut areas: BTreeMap<String, AreaCoverage> = BTreeMap::new();
    let mut engines: BTreeMap<(String, Option<String>), EngineCoverage> = BTreeMap::new();
    for item in merges.iter().filter(|m| m.merge.merged_unix >= since) {
        let Some(standing) = standing(item.merge) else {
            continue;
        };
        let measured = began
            .get(&item.merge.repo)
            .is_some_and(|began| item.merge.merged_unix >= *began)
            && item.hunks.iter().any(|h| h.note_ref.is_some());
        if !measured {
            add(&mut map.not_measured_merges, 1);
            for area in &item.merge.areas {
                let row = areas.entry(area.clone()).or_default();
                add(&mut row.not_measured_merges, 1);
            }
            continue;
        }
        add(&mut map.merges, 1);
        let mut touched = BTreeSet::new();
        for stored in item.hunks {
            let Author::Agent { engine, model } = &stored.hunk.author else {
                continue;
            };
            let (len, area) = (stored.hunk.hunk.len, area_of(&stored.hunk.hunk.path));
            let row = areas.entry(area.clone()).or_default();
            let by = engines.entry((engine.clone(), model.clone())).or_default();
            for total in [
                &mut map.agent_lines,
                &mut row.agent_lines,
                &mut by.agent_lines,
            ] {
                add(total, len);
            }
            match standing {
                Standing::Reviewed => {
                    for total in [
                        &mut map.reviewed_lines,
                        &mut row.reviewed_lines,
                        &mut by.reviewed_lines,
                    ] {
                        add(total, len);
                    }
                }
                Standing::InQueue => {
                    add(&mut map.in_queue_lines, len);
                    add(&mut row.in_queue_lines, len);
                }
                Standing::Unreviewed => {}
            }
            touched.insert(area);
        }
        for area in touched {
            add(&mut areas.entry(area).or_default().merges, 1);
        }
    }
    map.areas = areas
        .into_iter()
        .map(|(area, row)| AreaCoverage { area, ..row })
        .collect();
    map.engines = engines
        .into_iter()
        .map(|((engine, model), row)| EngineCoverage {
            engine,
            model,
            ..row
        })
        .collect();
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::capacity::{AuthoredHunk, Hunk};
    use crate::crew::fixtures::merge_record;

    const REPO: &str = "/repo";

    fn merge(task: &str, state: &str, risk: &str, at: i64) -> MergeRecord {
        let mut m = merge_record(task, REPO, "main", "x");
        m.review_state = state.to_owned();
        m.risk = risk.to_owned();
        m.merged_unix = at;
        m.areas = vec!["auth/login".to_owned()];
        m
    }

    fn hunk(path: &str, len: u32, author: Author, note: bool) -> StoredHunk {
        StoredHunk {
            hunk: AuthoredHunk {
                hunk: Hunk {
                    path: path.to_owned(),
                    start: 1,
                    len,
                },
                author,
            },
            note_ref: note.then(|| "refs/notes/x".to_owned()),
        }
    }

    fn agent() -> Author {
        Author::Agent {
            engine: "claude".to_owned(),
            model: Some("sonnet".to_owned()),
        }
    }

    fn began(at: i64) -> BTreeMap<String, i64> {
        BTreeMap::from([(REPO.to_owned(), at)])
    }

    fn run(m: &MergeRecord, hunks: &[StoredHunk], began: &BTreeMap<String, i64>) -> CoverageMap {
        coverage(&[MergeProvenance { merge: m, hunks }], began, 0)
    }

    #[test]
    fn looks_good_and_commented_merges_count_their_hunks_as_reviewed() {
        for state in ["looks_good", "commented"] {
            let m = merge("t", state, "high", 100);
            let h = [hunk("auth/login/a.rs", 10, agent(), true)];
            let c = run(&m, &h, &began(50));
            assert_eq!(
                (c.agent_lines, c.reviewed_lines, c.in_queue_lines),
                (10, 10, 0)
            );
            assert_eq!((c.merges, c.not_measured_merges), (1, 0));
            assert_eq!(c.areas[0].area, "auth/login");
            assert_eq!(c.areas[0].reviewed_lines, 10);
            assert_eq!(c.engines[0].reviewed_lines, 10);
        }
    }

    #[test]
    fn a_waiting_high_or_medium_merge_is_in_the_queue_and_a_low_one_is_not() {
        let h = [hunk("web/a.ts", 7, agent(), true)];
        let queued = run(&merge("t", "waiting", "medium", 100), &h, &began(50));
        assert_eq!((queued.in_queue_lines, queued.reviewed_lines), (7, 0));
        let low = run(&merge("t", "waiting", "low", 100), &h, &began(50));
        assert_eq!(
            (low.agent_lines, low.in_queue_lines, low.reviewed_lines),
            (7, 0, 0)
        );
    }

    #[test]
    fn a_merge_before_provenance_began_is_not_measured_and_adds_no_line() {
        let m = merge("t", "looks_good", "high", 40);
        let h = [hunk("a/b/c.rs", 10, agent(), true)];
        let c = run(&m, &h, &began(50));
        assert_eq!((c.merges, c.not_measured_merges, c.agent_lines), (0, 1, 0));
        assert_eq!(c.areas[0].not_measured_merges, 1);
        assert_eq!(c.began_unix, Some(50));
    }

    #[test]
    fn a_merge_with_no_hunk_or_no_note_is_not_measured() {
        let m = merge("t", "looks_good", "high", 100);
        let none = run(&m, &[], &began(50));
        assert_eq!((none.not_measured_merges, none.agent_lines), (1, 0));
        let unnoted = run(&m, &[hunk("a/b/c.rs", 4, agent(), false)], &began(50));
        assert_eq!((unnoted.not_measured_merges, unnoted.agent_lines), (1, 0));
    }

    #[test]
    fn without_a_provenance_start_nothing_is_measured() {
        let m = merge("t", "looks_good", "high", 100);
        let c = run(&m, &[hunk("a/b/c.rs", 4, agent(), true)], &BTreeMap::new());
        assert_eq!(
            (c.began_unix, c.not_measured_merges, c.agent_lines),
            (None, 1, 0)
        );
    }

    #[test]
    fn person_hunks_and_reverted_merges_add_no_agent_line() {
        let h = [
            hunk("a/b/c.rs", 3, Author::Person, true),
            hunk("a/b/d.rs", 5, agent(), true),
        ];
        let c = run(&merge("t", "looks_good", "high", 100), &h, &began(50));
        assert_eq!(c.agent_lines, 5);
        let reverted = run(&merge("t", "reverted", "high", 100), &h, &began(50));
        assert_eq!(
            (
                reverted.merges,
                reverted.not_measured_merges,
                reverted.agent_lines
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn since_drops_older_merges_entirely_and_areas_split_by_path() {
        let old = merge("old", "looks_good", "high", 60);
        let new = merge("new", "waiting", "high", 200);
        let h = [
            hunk("auth/login/a.rs", 10, agent(), true),
            hunk("docs/x.md", 6, agent(), true),
        ];
        let all = [
            MergeProvenance {
                merge: &old,
                hunks: &h,
            },
            MergeProvenance {
                merge: &new,
                hunks: &h,
            },
        ];
        let c = coverage(&all, &began(50), 100);
        assert_eq!((c.merges, c.agent_lines, c.in_queue_lines), (1, 16, 16));
        let areas: Vec<_> = c.areas.iter().map(|a| a.area.as_str()).collect();
        assert_eq!(areas, ["auth/login", "docs/x.md"]);
        assert_eq!(c.areas[0].merges, 1);
    }
}
