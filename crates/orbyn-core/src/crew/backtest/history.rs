//! Which pull requests a backtest may replay: merged within the history window and old enough that the outcome
//! window has closed, so a later revert or fix would already show.

/// How far back a backtest looks, in days.
pub const HISTORY_DAYS: i64 = 180;
/// How long a merge waits for its outcome, in days. A younger merge has not had its chance to escape yet.
pub const OUTCOME_DAYS: i64 = 30;

const DAY_SECS: i64 = 86_400;

/// A merged pull request of the project's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastPr {
    pub number: u64,
    pub merged_unix: i64,
}

/// The pull requests merged at most 180 days ago and at least 30 days ago (both ends included), in the order given.
pub fn eligible_prs(prs: &[PastPr], now_unix: i64) -> Vec<&PastPr> {
    prs.iter()
        .filter(|pr| {
            let age = now_unix.saturating_sub(pr.merged_unix);
            (OUTCOME_DAYS * DAY_SECS..=HISTORY_DAYS * DAY_SECS).contains(&age)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn aged(number: u64, days: i64) -> PastPr {
        PastPr {
            number,
            merged_unix: NOW - days * DAY_SECS,
        }
    }

    fn numbers(prs: &[&PastPr]) -> Vec<u64> {
        prs.iter().map(|pr| pr.number).collect()
    }

    #[test]
    fn the_history_window_includes_day_180_and_excludes_day_181() {
        let prs = [aged(179, 179), aged(180, 180), aged(181, 181)];
        assert_eq!(numbers(&eligible_prs(&prs, NOW)), vec![179, 180]);
    }

    #[test]
    fn the_outcome_window_must_have_closed_by_day_30() {
        let prs = [aged(29, 29), aged(30, 30), aged(31, 31)];
        assert_eq!(numbers(&eligible_prs(&prs, NOW)), vec![30, 31]);
    }

    #[test]
    fn one_second_either_side_of_a_boundary_is_decided_exactly() {
        let just_young = PastPr {
            number: 1,
            merged_unix: NOW - 30 * DAY_SECS + 1,
        };
        let just_old = PastPr {
            number: 2,
            merged_unix: NOW - 180 * DAY_SECS - 1,
        };
        assert!(eligible_prs(&[just_young, just_old], NOW).is_empty());
    }

    #[test]
    fn a_merge_dated_in_the_future_is_not_eligible() {
        let prs = [aged(1, -2)];
        assert!(eligible_prs(&prs, NOW).is_empty());
    }

    #[test]
    fn an_empty_history_gives_nothing() {
        assert!(eligible_prs(&[], NOW).is_empty());
    }
}
