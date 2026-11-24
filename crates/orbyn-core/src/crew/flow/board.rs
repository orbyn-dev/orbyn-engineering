//! The Taskboard read (Crew Preview): a crew's work tasks grouped into the seven columns of the run states. It reads what Flow
//! reads (`flow_facts`, the crew-wide scrub, the work-task list) and shows less of it: a card has no cell, and its only word
//! about checks is two counts, passed and run, for a passed task (any other state has none). Nothing here writes. A reviewer's run and
//! a fix are lanes and rounds of their task in Flow, not cards. The two flags are advisory; the write routes decide.
//!
//! KC3: the Needs you column reads the one needs-you field, not the task's run state. A card sits there when the field names it
//! (a task that waits on a person, a merge that waits, a trip-wire stop), with the field's kind and reason. A task whose
//! dismissible wait was dismissed on this device keeps its card, because its run state is still `needs_person` and the board never
//! hides a task that still cannot run: it has no field kind and no reason, so it draws no sentence, and the Inbox and dashboard
//! (which read only the field) do not list it. The board reads the same field they do, not necessarily the same count. A card's
//! Working line comes from the one `working` function (`crew/working.rs`), never from a copy of its truth table.

use std::collections::BTreeMap;

use orbyn_contracts as wire;
use orbyn_contracts::FlowColumnId as Column;

use super::cells::{build_state, check_counts};
use super::facts::Facts;
use super::scrub::Scrub;
use super::stop_detail;
use super::{QUERY_MAX, SCAN_MAX};
use crate::app::Core;
use crate::crew::working::{flow_working, working};
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
    /// One card, and whether it is urgent in the Needs you column. `wait` is the one needs-you field's item for this task,
    /// when the field names it: the column, the kind and the reason then come from that field, never from a second rule. The
    /// reason is the field's own line (a task's scrubbed result, or the core's fixed words for a merge or a stop). A card the
    /// field does not name, because its dismissible wait was dismissed here, has no kind and no reason: its run state puts it in
    /// Needs you and it draws no sentence.
    fn board_card(
        &self,
        facts: &Facts,
        scrub: &Scrub,
        wait: Option<&wire::NeedsYouItem>,
    ) -> Result<(bool, wire::FlowCard)> {
        let task = &facts.task;
        let reason = wait.and_then(|item| item.reason.clone());
        let overdue = facts.record.as_ref().is_some_and(|r| {
            r.review_state == "waiting"
                && r.due_unix.is_some_and(|due| due < crate::util::unix_now())
        });
        let high = facts.entry.as_ref().is_some_and(|e| e.risk == "high")
            || facts.record.as_ref().is_some_and(|r| r.risk == "high");
        let checkout = facts.run.as_ref().is_some_and(|r| r.worktree.is_some());
        // KC3: the one working function answers, and the latest fix is the only work that outlives the task's state.
        let latest_fix = facts.fixes.last().map(|(_, state)| state.as_str());
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
                stop_detail: stop_detail(task.stop_detail.as_deref()),
                updated_at: task.updated_at.clone(),
                reason,
                could_not_start: facts.could_not_start(),
                can_cancel: can_cancel(&task.state),
                can_rerun: can_rerun(
                    &task.state,
                    task.stop_reason.as_deref(),
                    checkout,
                    !facts.evidence.is_empty(),
                ),
                checks: check_counts(
                    build_state(facts),
                    &facts.build_evidence,
                    &facts.build_gates,
                )
                .map(|(passed, of)| wire::FlowChecks { passed, of }),
                needs_you: wait.map(|item| item.kind),
                working: flow_working(working(&task.state, latest_fix)),
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
        // KC3: the one needs-you field. A card sits in Needs you because the field names it, not because of its run state: a
        // passed task whose merge waits and a stopped task whose trip wire stopped it are cards there too, and a dismissed
        // stop is not. A background run's wait has no task and no card; the same read serves the Inbox and the dashboard.
        let waits: BTreeMap<String, wire::NeedsYouItem> = self
            .needs_you_all(&|_, crew_id| crew_id == crew)?
            .into_iter()
            .filter(|item| item.crew_id.as_deref() == Some(crew))
            .map(|item| (item.id.clone(), item))
            .collect();
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
            let wait = waits.get(&facts.task.id);
            let (urgent, card) = self.board_card(&facts, &scrub, wait)?;
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
            // The field decides the column when it names the card; otherwise the task's run state does. A task whose dismissible
            // wait was dismissed still reads needs_person and stays in Needs you, with no field kind and no sentence.
            let at = if card.needs_you.is_some() {
                COLUMNS
                    .iter()
                    .position(|c| *c == Column::NeedsYou)
                    .unwrap_or(4)
            } else {
                COLUMNS
                    .iter()
                    .position(|c| *c == column_of(&card.state))
                    .unwrap_or(2)
            };
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
            stop_detail: None,
            updated_at: at.to_owned(),
            reason: None,
            could_not_start: false,
            can_cancel: true,
            can_rerun: false,
            checks: None,
            needs_you: None,
            working: None,
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
