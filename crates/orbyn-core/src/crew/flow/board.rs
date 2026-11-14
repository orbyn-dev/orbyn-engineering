//! The Taskboard read (Crew Preview): a crew's work tasks grouped into the seven columns of the run states. It reads what Flow
//! reads (`flow_facts`, the crew-wide scrub, the work-task list) and shows less of it: a card has no cell and no count of
//! checks (a count is a follow-up, and never for a task that has not reached verifying). Nothing here writes. A reviewer's run and
//! a fix are lanes and rounds of their task in Flow, not cards. The two flags are advisory; the write routes decide.

use orbyn_contracts as wire;
use orbyn_contracts::{FlowColumnId as Column, ResultState};

use super::facts::Facts;
use super::scrub::{Scrub, TEXT_MAX};
use super::{QUERY_MAX, SCAN_MAX};
use crate::app::Core;
use crate::crew::{Page, valid_id, wire_name};
use crate::error::{CoreError, Result};

/// Cards per column when none is asked for, and the most that can be.
pub const PER_COLUMN_DEFAULT: u32 = 8;
pub const PER_COLUMN_MAX: u32 = 50;
/// The most engines or kinds the header chips list.
const FACETS_MAX: usize = 20;

/// The stop reasons that are not a safety stop: the same list `store::crew_rerun` allows a re-run for.
const BENIGN_STOPS: [&str; 3] = ["repeated_failure", "budget", "engine_error"];

const COLUMNS: [Column; 7] = [
    Column::Queued,
    Column::Routed,
    Column::Running,
    Column::Verifying,
    Column::NeedsYou,
    Column::Passed,
    Column::StoppedCancelled,
];

/// The column a state sits in. `failed` is never at rest and `changed_approach` is a retry, so both read as Running.
pub(super) fn column_of(state: &str) -> Column {
    match state {
        "briefed" => Column::Queued,
        "routed" => Column::Routed,
        "verifying" => Column::Verifying,
        "needs_person" => Column::NeedsYou,
        "passed" => Column::Passed,
        "stopped" | "cancelled" => Column::StoppedCancelled,
        _ => Column::Running,
    }
}

/// The states a task can still leave for `cancelled` (`lifecycle::next_states`).
pub(super) fn can_cancel(state: &str) -> bool {
    crate::crew::lifecycle::next_states(state).contains(&"cancelled")
}

/// Whether a re-run of the checks may apply: it passed, or it stopped for a benign reason after its checks ran, and it has a
/// checkout. The busy cases (a claim, the gap, a live worker) are not read: the route decides.
pub(super) fn can_rerun(
    state: &str,
    stop_reason: Option<&str>,
    has_checkout: bool,
    checks_ran: bool,
) -> bool {
    has_checkout
        && match state {
            "passed" => true,
            "stopped" => checks_ran && stop_reason.is_some_and(|r| BENIGN_STOPS.contains(&r)),
            _ => false,
        }
}

/// Orders a column: newest first. In Needs you, a card that is urgent (a High risk merge, or a review past its due time) comes
/// before the rest. Equal times fall back to the id so the order never wobbles.
fn order(cards: &mut [(bool, wire::FlowCard)], needs_you: bool) {
    cards.sort_by(|(a_urgent, a), (b_urgent, b)| {
        let urgent = if needs_you {
            b_urgent.cmp(a_urgent)
        } else {
            std::cmp::Ordering::Equal
        };
        urgent
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn facets(counts: std::collections::BTreeMap<String, u32>) -> Vec<wire::FlowFacet> {
    let mut list: Vec<wire::FlowFacet> = counts
        .into_iter()
        .map(|(name, count)| wire::FlowFacet { name, count })
        .collect();
    list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    list.truncate(FACETS_MAX);
    list
}

impl Core {
    fn board_card(&self, facts: &Facts, scrub: &Scrub) -> Result<(bool, wire::FlowCard)> {
        let task = &facts.task;
        // The reason is the core's own words for why the person is needed, from the task's latest such result.
        let reason = if task.state == "needs_person" {
            self.store
                .crew_results(&task.id)?
                .into_iter()
                .rev()
                .find(|r| r.state == ResultState::NeedsPerson)
                .map(|r| scrub.text(&r.summary, TEXT_MAX))
        } else {
            None
        };
        let overdue = facts.record.as_ref().is_some_and(|r| {
            r.review_state == "waiting"
                && r.due_unix.is_some_and(|due| due < crate::util::unix_now())
        });
        let high = facts.entry.as_ref().is_some_and(|e| e.risk == "high")
            || facts.record.as_ref().is_some_and(|r| r.risk == "high");
        let checkout = facts.run.as_ref().is_some_and(|r| r.worktree.is_some());
        Ok((
            overdue || high,
            wire::FlowCard {
                id: task.id.clone(),
                name: facts
                    .brief
                    .as_ref()
                    .map_or_else(String::new, |b| scrub.text(&b.outcome, 160)),
                kind: facts
                    .brief
                    .as_ref()
                    .map_or_else(String::new, |b| wire_name(&b.kind)),
                owner: facts
                    .run
                    .as_ref()
                    .map_or_else(String::new, |r| scrub.text(&r.member, 60)),
                engine: facts.route.as_ref().map(|r| r.engine.as_str().to_owned()),
                model: facts
                    .route
                    .as_ref()
                    .map(|r| scrub.text(r.model.as_str(), 80)),
                effort: facts.route.as_ref().map(|r| wire_name(&r.effort)),
                state: task.state.clone(),
                stop_reason: task.stop_reason.clone(),
                updated_at: task.updated_at.clone(),
                reason,
                can_cancel: can_cancel(&task.state),
                can_rerun: can_rerun(
                    &task.state,
                    task.stop_reason.as_deref(),
                    checkout,
                    !facts.evidence.is_empty(),
                ),
            },
        ))
    }

    /// One crew's work tasks as the seven columns. `per_column` (1 to 50, default 8) caps the cards of each; with `column` set only
    /// that column is returned, paged by `page`. `engine`, `kind` and `q` narrow the cards; the header counts are not narrowed.
    #[allow(clippy::too_many_arguments)]
    pub fn crew_flow_board(
        &self,
        crew: &str,
        engine: Option<&str>,
        kind: Option<&str>,
        q: Option<&str>,
        column: Option<&str>,
        per_column: Option<u32>,
        page: Page,
    ) -> Result<wire::FlowBoard> {
        self.crew_preview_only()?;
        valid_id(crew)?;
        for filter in [engine, kind].into_iter().flatten() {
            valid_id(filter)?;
        }
        let only: Option<Column> = column
            .map(|text| {
                serde_json::from_value(serde_json::Value::String(text.to_owned()))
                    .map_err(|_| CoreError::Invalid("that column is not on the board".into()))
            })
            .transpose()?;
        let per_column = per_column.unwrap_or(PER_COLUMN_DEFAULT);
        if !(1..=PER_COLUMN_MAX).contains(&per_column) {
            return Err(CoreError::Invalid(format!(
                "per_column is 1 to {PER_COLUMN_MAX}"
            )));
        }
        let needle = q.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
        if needle
            .as_ref()
            .is_some_and(|q| q.chars().count() > QUERY_MAX)
        {
            return Err(CoreError::Invalid(format!(
                "the search is at most {QUERY_MAX} characters"
            )));
        }
        let person = self.display_redactor()?;
        let scrub = Scrub::of_crew(self, &person, crew)?;
        let mut tasks = self.store.crew_work_tasks(crew, SCAN_MAX + 1)?;
        let truncated = tasks.len() > SCAN_MAX as usize;
        tasks.truncate(SCAN_MAX as usize);

        let total = u32::try_from(tasks.len()).unwrap_or(u32::MAX);
        let mut engines = std::collections::BTreeMap::new();
        let mut kinds = std::collections::BTreeMap::new();
        let mut grouped: Vec<Vec<(bool, wire::FlowCard)>> =
            COLUMNS.iter().map(|_| Vec::new()).collect();
        for task in tasks {
            let facts = self.flow_facts(task)?;
            let (urgent, card) = self.board_card(&facts, &scrub)?;
            if let Some(name) = &card.engine {
                *engines.entry(name.clone()).or_insert(0u32) += 1;
            }
            if !card.kind.is_empty() {
                *kinds.entry(card.kind.clone()).or_insert(0u32) += 1;
            }
            if engine.is_some_and(|e| card.engine.as_deref() != Some(e))
                || kind.is_some_and(|k| card.kind != k)
            {
                continue;
            }
            if let Some(needle) = &needle {
                let hay = format!(
                    "{} {} {}",
                    card.name,
                    card.kind,
                    card.engine.as_deref().unwrap_or("")
                )
                .to_lowercase();
                if !hay.contains(needle.as_str()) {
                    continue;
                }
            }
            let at = COLUMNS
                .iter()
                .position(|c| *c == column_of(&card.state))
                .unwrap_or(2);
            grouped[at].push((urgent, card));
        }

        let mut columns = Vec::new();
        for (id, mut cards) in COLUMNS.into_iter().zip(grouped) {
            if only.is_some_and(|c| c != id) {
                continue;
            }
            order(&mut cards, id == Column::NeedsYou);
            let count = u32::try_from(cards.len()).unwrap_or(u32::MAX);
            let (start, size) = if only.is_some() {
                (page.offset as usize, page.limit as usize)
            } else {
                (0, per_column as usize)
            };
            let start = start.min(cards.len());
            let end = (start + size).min(cards.len());
            columns.push(wire::FlowColumn {
                column: id,
                count,
                next_offset: (end < cards.len()).then(|| u32::try_from(end).unwrap_or(u32::MAX)),
                cards: cards.drain(start..end).map(|(_, card)| card).collect(),
            });
        }
        Ok(wire::FlowBoard {
            crew: crew.to_owned(),
            total,
            engines: facets(engines),
            kinds: facets(kinds),
            columns,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, at: &str) -> wire::FlowCard {
        wire::FlowCard {
            id: id.to_owned(),
            name: String::new(),
            kind: "implement".to_owned(),
            owner: String::new(),
            engine: None,
            model: None,
            effort: None,
            state: "needs_person".to_owned(),
            stop_reason: None,
            updated_at: at.to_owned(),
            reason: None,
            can_cancel: true,
            can_rerun: false,
        }
    }

    #[test]
    fn every_state_has_a_column_and_the_cancel_flag_follows_the_lifecycle() {
        let columns = [
            ("briefed", Column::Queued),
            ("routed", Column::Routed),
            ("running", Column::Running),
            ("changed_approach", Column::Running),
            ("failed", Column::Running),
            ("verifying", Column::Verifying),
            ("needs_person", Column::NeedsYou),
            ("passed", Column::Passed),
            ("stopped", Column::StoppedCancelled),
            ("cancelled", Column::StoppedCancelled),
        ];
        for (state, column) in columns {
            assert_eq!(column_of(state), column, "{state}");
            let live = !matches!(state, "passed" | "stopped" | "cancelled");
            assert_eq!(can_cancel(state), live, "{state}");
        }
        assert_eq!(columns.len(), crate::crew::lifecycle::STATES.len());
    }

    #[test]
    fn a_rerun_may_apply_to_passed_and_to_a_benign_stop_that_ran_its_checks() {
        assert!(can_rerun("passed", None, true, true));
        assert!(!can_rerun("passed", None, false, true), "no checkout");
        for reason in BENIGN_STOPS {
            assert!(can_rerun("stopped", Some(reason), true, true), "{reason}");
            assert!(
                !can_rerun("stopped", Some(reason), true, false),
                "never verified"
            );
        }
        for reason in ["injection_suspected", "too_many_denials", "lead_stopped"] {
            assert!(!can_rerun("stopped", Some(reason), true, true), "{reason}");
        }
        assert!(!can_rerun("stopped", None, true, true));
        for state in [
            "briefed",
            "routed",
            "running",
            "verifying",
            "failed",
            "needs_person",
            "cancelled",
        ] {
            assert!(!can_rerun(state, None, true, true), "{state}");
        }
    }

    #[test]
    fn needs_you_puts_the_urgent_first_and_every_column_is_newest_first() {
        let mut cards = vec![
            (false, card("a", "2026-01-03T00:00:00Z")),
            (true, card("b", "2026-01-01T00:00:00Z")),
            (false, card("c", "2026-01-02T00:00:00Z")),
        ];
        order(&mut cards, true);
        let ids: Vec<&str> = cards.iter().map(|(_, c)| c.id.as_str()).collect();
        assert_eq!(ids, ["b", "a", "c"], "urgent first, then newest");
        order(&mut cards, false);
        let ids: Vec<&str> = cards.iter().map(|(_, c)| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "c", "b"], "newest first elsewhere");
        let mut same = vec![
            (false, card("y", "2026-01-01T00:00:00Z")),
            (false, card("x", "2026-01-01T00:00:00Z")),
        ];
        order(&mut same, false);
        assert_eq!(same[0].1.id, "x", "ties break by id");
    }
}
