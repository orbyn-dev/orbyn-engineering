//! The breaker's rule. Pure: counts and a clock in, a change out; no store, no git.

use super::{MIN_MERGES, RECOVERY_DAYS, RECOVERY_MERGES, TRIP_REVERTS, WINDOW_DAYS};
use crate::crew::autopilot::Mode;
use crate::store::MergeRecord;

/// Merges that touched an area, and how many of them were reverted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub merges: u32,
    pub reverts: u32,
}

/// What the rule reads: the merges of the window, and the merges since the recovery clock started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub window: Counts,
    pub since_recovery: Counts,
}

/// Where an area is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stored {
    Follows,
    Demoted { recovery_since: i64 },
}

/// What the rule asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Nothing,
    /// Lower the area from `Full` to `On`.
    Demote,
    /// The area follows its ceiling again.
    Restore,
}

/// Merges of `area` made at or after `since`, and how many are now reverted. The same meaning as the 60-day rate: merged in the
/// window, now reverted.
pub fn count(merges: &[MergeRecord], area: &str, since: i64) -> Counts {
    let mine = merges
        .iter()
        .filter(|m| m.merged_unix >= since && m.areas.iter().any(|a| a == area));
    let (mut merges, mut reverts) = (0u32, 0u32);
    for m in mine {
        merges = merges.saturating_add(1);
        if m.review_state == "reverted" {
            reverts = reverts.saturating_add(1);
        }
    }
    Counts { merges, reverts }
}

/// The start of the window that ends at `now`.
pub fn window_start(now: i64) -> i64 {
    now - WINDOW_DAYS * 86_400
}

/// The window has enough merges and enough of them were reverted.
pub fn trips(window: Counts) -> bool {
    window.merges >= MIN_MERGES && window.reverts >= TRIP_REVERTS
}

/// A lowered area has had no revert for the recovery days and enough merges since, none reverted.
pub fn recovered(recovery_since: i64, since_recovery: Counts, now: i64) -> bool {
    now - recovery_since >= RECOVERY_DAYS * 86_400
        && since_recovery.merges >= RECOVERY_MERGES
        && since_recovery.reverts == 0
}

/// What to do with an area. A ceiling that is not `Full` is never changed: the record is shown and nothing is lowered.
pub fn decide(facts: &Facts, stored: Stored, ceiling: Mode, now: i64) -> Change {
    match stored {
        Stored::Follows if ceiling == Mode::Full && trips(facts.window) => Change::Demote,
        Stored::Follows => Change::Nothing,
        Stored::Demoted { recovery_since }
            if recovered(recovery_since, facts.since_recovery, now) =>
        {
            Change::Restore
        }
        Stored::Demoted { .. } => Change::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;

    fn facts(merges: u32, reverts: u32) -> Facts {
        Facts {
            window: Counts { merges, reverts },
            since_recovery: Counts::default(),
        }
    }

    #[test]
    fn four_merges_with_two_reverts_do_not_trip_and_five_do() {
        assert_eq!(
            decide(&facts(4, 2), Stored::Follows, Mode::Full, NOW),
            Change::Nothing
        );
        assert_eq!(
            decide(&facts(5, 2), Stored::Follows, Mode::Full, NOW),
            Change::Demote
        );
        assert_eq!(
            decide(&facts(9, 1), Stored::Follows, Mode::Full, NOW),
            Change::Nothing
        );
        assert_eq!(
            decide(&facts(5, 5), Stored::Follows, Mode::Full, NOW),
            Change::Demote
        );
    }

    #[test]
    fn a_ceiling_below_full_is_never_changed() {
        for ceiling in [Mode::On, Mode::Off] {
            assert_eq!(
                decide(&facts(8, 4), Stored::Follows, ceiling, NOW),
                Change::Nothing
            );
        }
    }

    #[test]
    fn the_way_back_needs_both_counts_and_no_revert() {
        let since = NOW - 14 * DAY;
        let back = |days: i64, merges: u32, reverts: u32| {
            let facts = Facts {
                window: Counts::default(),
                since_recovery: Counts { merges, reverts },
            };
            decide(
                &facts,
                Stored::Demoted {
                    recovery_since: NOW - days * DAY,
                },
                Mode::Full,
                NOW,
            )
        };
        assert_eq!(back(14, 5, 0), Change::Restore);
        assert_eq!(back(13, 5, 0), Change::Nothing, "13 days is short");
        assert_eq!(back(30, 4, 0), Change::Nothing, "4 merges is short");
        assert_eq!(back(30, 6, 1), Change::Nothing, "a revert since");
        assert!(recovered(
            since,
            Counts {
                merges: 5,
                reverts: 0
            },
            NOW
        ));
        assert!(!recovered(
            since + 1,
            Counts {
                merges: 5,
                reverts: 0
            },
            NOW
        ));
    }

    #[test]
    fn the_window_includes_its_first_second_and_not_the_one_before() {
        let start = window_start(NOW);
        assert_eq!(start, NOW - 14 * DAY);
        let record = |at: i64, state: &str| MergeRecord {
            id: 0,
            task_id: String::new(),
            crew_id: String::new(),
            repo: String::new(),
            branch: String::new(),
            before_commit: String::new(),
            merge_commit: String::new(),
            candidate: String::new(),
            risk: "low".into(),
            reasons: serde_json::json!([]),
            areas: vec!["src/a".into()],
            engine: String::new(),
            mode: "full".into(),
            files: 0,
            lines: 0,
            proof: serde_json::json!({}),
            not_verified: serde_json::json!([]),
            merged_unix: at,
            due_unix: None,
            review_state: state.into(),
            comment: None,
            watch: "unknown".into(),
            reverted_commit: None,
            needs_person: None,
        };
        let merges = [
            record(NOW - 13 * DAY, "reverted"),
            record(NOW - 14 * DAY, "reverted"),
            record(NOW - 15 * DAY, "reverted"),
            record(NOW - DAY, "waiting"),
        ];
        assert_eq!(
            count(&merges, "src/a", start),
            Counts {
                merges: 3,
                reverts: 2
            }
        );
        assert_eq!(count(&merges, "src/b", start), Counts::default());
    }
}
