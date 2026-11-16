//! Backpressure: count the merges that wait for a person and decide whether a new start may go ahead.

use super::{CapacitySettings, Priority};
use crate::crew::autopilot::Risk;
use crate::store::MergeRecord;

/// What happens to a new start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartDecision {
    Go,
    /// Held back: `waiting` merges are over the `limit`.
    Paused {
        waiting: u32,
        limit: u32,
    },
}

/// The merges still waiting for a person whose risk is High or Medium. Low risk, reviewed and reverted merges
/// do not count, and neither does a row whose risk is not a known word.
pub fn waiting_count(merges: &[MergeRecord]) -> u32 {
    let count = merges
        .iter()
        .filter(|m| m.review_state == "waiting")
        .filter(|m| matches!(Risk::parse(&m.risk), Some(Risk::High | Risk::Medium)))
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Whether a NEW start may go ahead. Only a Low-priority task that Crew started itself can be paused, and only
/// when more than `limit` merges wait. A task a person started, and any High or Medium task, always goes.
pub fn start_allowed(
    settings: CapacitySettings,
    waiting: u32,
    priority: Priority,
    crew_started: bool,
) -> StartDecision {
    if crew_started && priority == Priority::Low && waiting > settings.limit {
        StartDecision::Paused {
            waiting,
            limit: settings.limit,
        }
    } else {
        StartDecision::Go
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::fixtures::merge_record;

    fn row(risk: &str, state: &str) -> MergeRecord {
        let mut m = merge_record("t", "/repo", "main", "x");
        m.risk = risk.to_owned();
        m.review_state = state.to_owned();
        m
    }

    #[test]
    fn the_count_takes_only_waiting_high_and_medium() {
        let merges = [
            row("high", "waiting"),
            row("medium", "waiting"),
            row("low", "waiting"),
            row("high", "looks_good"),
            row("medium", "reverted"),
            row("unknown", "waiting"),
        ];
        assert_eq!(waiting_count(&merges), 2);
        assert_eq!(waiting_count(&[]), 0);
    }

    #[test]
    fn over_the_limit_pauses_a_low_start_that_crew_began() {
        let s = CapacitySettings::default();
        assert_eq!(s.limit, 8);
        assert_eq!(
            start_allowed(s, 9, Priority::Low, true),
            StartDecision::Paused {
                waiting: 9,
                limit: 8
            }
        );
    }

    #[test]
    fn exactly_at_the_limit_goes() {
        let s = CapacitySettings::default();
        assert_eq!(start_allowed(s, 8, Priority::Low, true), StartDecision::Go);
        assert_eq!(start_allowed(s, 0, Priority::Low, true), StartDecision::Go);
    }

    #[test]
    fn a_task_a_person_started_never_pauses() {
        let s = CapacitySettings::default();
        assert_eq!(
            start_allowed(s, 50, Priority::Low, false),
            StartDecision::Go
        );
    }

    #[test]
    fn high_and_medium_never_pause() {
        let s = CapacitySettings::default();
        for priority in [Priority::High, Priority::Medium] {
            for crew_started in [true, false] {
                assert_eq!(
                    start_allowed(s, 50, priority, crew_started),
                    StartDecision::Go
                );
            }
        }
    }
}
