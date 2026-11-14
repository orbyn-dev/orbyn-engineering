//! What the routes read about the areas of a project: rows with the record, the state and a sentence, the trail of one area, and the
//! thresholds. Nothing here writes, and no GET starts a return (that waits for the next merge). Names are scrubbed as the trust
//! record's are. The merge gate reads the same rows (`effective`).

use std::collections::BTreeSet;

use orbyn_contracts as wire;

use super::breaker::{clean_days, days_needed, facts_of, stored_of};
use super::decide::{Stored, recovered, trips};
use super::{MIN_MERGES, RECOVERY_DAYS, RECOVERY_MERGES, TRIP_REVERTS, WINDOW_DAYS};
use crate::app::Core;
use crate::crew::autopilot::{Mode, Page, valid_id};
use crate::crew::redact;
use crate::error::{CoreError, Result};
use crate::store::{AreaEvent, AreaMode, MergeRecord};

/// The areas a project lists: those with a merge in this many days, and those with a stored row.
const SHOWN_DAYS: i64 = 60;
/// The most rows of one area's trail.
const HISTORY_MAX: usize = 100;
/// The longest area name a request may carry.
const AREA_MAX: usize = 300;

pub(super) fn mode_wire(mode: Mode) -> wire::AutopilotMode {
    match mode {
        Mode::Off => wire::AutopilotMode::Off,
        Mode::On => wire::AutopilotMode::On,
        Mode::Full => wire::AutopilotMode::Full,
    }
}

pub(super) fn word(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "Off",
        Mode::On => "On",
        Mode::Full => "Full",
    }
}

fn to_u32(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

fn valid_area(area: &str) -> Result<()> {
    if area.is_empty() || area.len() > AREA_MAX || area.chars().any(char::is_control) {
        return Err(CoreError::Invalid("that area is not valid".into()));
    }
    Ok(())
}

fn record_wire(merges: u32, reverts: u32) -> wire::AutopilotAreaRecord {
    wire::AutopilotAreaRecord {
        merges,
        reverts,
        escapes: None,
        window_days: to_u32(WINDOW_DAYS),
    }
}

/// The row of one area. `rank` orders the list: lowered areas first.
fn row(
    area: &str,
    stored_row: Option<&AreaMode>,
    merges: &[MergeRecord],
    ceiling: Mode,
    now: i64,
) -> (u8, wire::AutopilotAreaRow) {
    let stored = stored_of(stored_row);
    let facts = facts_of(merges, area, stored, now);
    let window = facts.window;
    let person_override = stored_row.and_then(|r| {
        r.person_override
            .clone()
            .map(|choice| wire::AutopilotAreaOverride {
                choice,
                at: r.override_at.unwrap_or(r.updated_at),
            })
    });
    let mut recovery = None;
    let mut since = None;
    let (state, line, shown, rank) = if let Some(choice) = &person_override {
        let mode = if choice.choice == "off" {
            Mode::Off
        } else {
            ceiling
        };
        (
            wire::AutopilotAreaState::Overridden,
            format!(
                "A person set this area to {}.",
                choice.choice.replace('_', " ")
            ),
            mode,
            2,
        )
    } else {
        match stored {
            Stored::Demoted { recovery_since } => {
                since = stored_row.and_then(|r| r.tripped_at);
                let met = recovered(recovery_since, facts.since_recovery, now);
                recovery = Some(wire::AutopilotAreaRecovery {
                    clean_days: clean_days(recovery_since, now),
                    days_needed: days_needed(),
                    merges: facts.since_recovery.merges,
                    merges_needed: RECOVERY_MERGES,
                });
                let shown = if ceiling == Mode::Full {
                    Mode::On
                } else {
                    ceiling
                };
                if met {
                    (
                        wire::AutopilotAreaState::CriteriaMet,
                        "Criteria met; returns at the next merge.".to_owned(),
                        shown,
                        1,
                    )
                } else {
                    let fresh =
                        facts.since_recovery.merges == 0 && clean_days(recovery_since, now) == 0;
                    (
                        if fresh {
                            wire::AutopilotAreaState::Demoted
                        } else {
                            wire::AutopilotAreaState::Recovering
                        },
                        format!(
                            "Lowered after {} of {} merges were reverted in {WINDOW_DAYS} days. Returns after {RECOVERY_DAYS} days with no revert and {RECOVERY_MERGES} merges since.",
                            trip_count(stored_row, "reverts", window.reverts),
                            trip_count(stored_row, "merges", window.merges),
                        ),
                        shown,
                        0,
                    )
                }
            }
            Stored::Follows if ceiling != Mode::Full => (
                wire::AutopilotAreaState::CeilingLower,
                format!(
                    "The ceiling is {}, so the breaker has nothing to lower. The record is shown only.",
                    word(ceiling)
                ),
                ceiling,
                3,
            ),
            Stored::Follows if window.merges < MIN_MERGES => (
                wire::AutopilotAreaState::NotEnoughData,
                format!(
                    "Not enough data: {} of {MIN_MERGES} merges in {WINDOW_DAYS} days.",
                    window.merges
                ),
                ceiling,
                3,
            ),
            Stored::Follows if trips(window) => (
                wire::AutopilotAreaState::Follows,
                format!(
                    "{} of {} merges were reverted in {WINDOW_DAYS} days; it is evaluated at the next merge or revert.",
                    window.reverts, window.merges
                ),
                ceiling,
                3,
            ),
            Stored::Follows => (
                wire::AutopilotAreaState::Follows,
                format!(
                    "Follows the ceiling. {} of {} merges reverted in {WINDOW_DAYS} days.",
                    window.reverts, window.merges
                ),
                ceiling,
                3,
            ),
        }
    };
    (
        rank,
        wire::AutopilotAreaRow {
            area: redact(area),
            area_key: area.to_owned(),
            ceiling: mode_wire(ceiling),
            now: mode_wire(shown),
            state,
            line,
            since,
            record: record_wire(window.merges, window.reverts),
            recovery,
            person_override,
        },
    )
}

/// A count the trip recorded, else the one counted now.
fn trip_count(row: Option<&AreaMode>, key: &str, fallback: u32) -> u32 {
    row.and_then(|r| r.trip_reason.as_ref())
        .and_then(|v| v[key].as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(fallback)
}

fn event_wire(event: &AreaEvent) -> wire::AutopilotAreaEvent {
    let count = |key: &str| {
        event.record[key]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    wire::AutopilotAreaEvent {
        id: event.id,
        kind: event.kind.clone(),
        from_mode: event.from_mode.clone(),
        to_mode: event.to_mode.clone(),
        decided_by: event.decided_by.clone(),
        cause: redact(&event.cause),
        record: record_wire(count("merges"), count("reverts")),
        at: event.at,
    }
}

impl Core {
    /// The areas of a project: the ceiling, and for each area its record, state and a sentence. Lowered areas first, then by merges.
    pub fn autopilot_areas(
        &self,
        project: &str,
        page: Page,
        now: i64,
    ) -> Result<wire::AutopilotAreas> {
        self.crew_preview_only()?;
        valid_id(project)?;
        self.store.project(project)?;
        let merges = self.store.crew_merges_of_project(project)?;
        let ceiling = self.crew_project_mode(project);
        let stored = self.store.area_modes(project)?;
        let horizon = now - SHOWN_DAYS * 86_400;
        let mut names: BTreeSet<String> = merges
            .iter()
            .filter(|m| m.merged_unix >= horizon)
            .flat_map(|m| m.areas.iter().cloned())
            .collect();
        names.extend(stored.iter().map(|r| r.area.clone()));
        let mut rows: Vec<(u8, wire::AutopilotAreaRow)> = names
            .iter()
            .map(|area| {
                let mine = stored.iter().find(|r| &r.area == area);
                row(area, mine, &merges, ceiling, now)
            })
            .collect();
        rows.sort_by(|a, b| {
            (a.0, std::cmp::Reverse(a.1.record.merges), &a.1.area).cmp(&(
                b.0,
                std::cmp::Reverse(b.1.record.merges),
                &b.1.area,
            ))
        });
        let total = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        Ok(wire::AutopilotAreas {
            project: project.to_owned(),
            ceiling: mode_wire(ceiling),
            as_of: now,
            enforced: true,
            total,
            rows: rows
                .into_iter()
                .skip(page.offset as usize)
                .take(page.limit as usize)
                .map(|(_, r)| r)
                .collect(),
        })
    }

    /// The row of one area, as the areas read shows it. Used to answer a person's override or follow with the area as it now stands.
    pub(super) fn area_row(
        &self,
        project: &str,
        area: &str,
        now: i64,
    ) -> Result<wire::AutopilotAreaRow> {
        let merges = self.store.crew_merges_of_project(project)?;
        let ceiling = self.crew_project_mode(project);
        let stored = self.store.area_mode(project, area)?;
        Ok(row(area, stored.as_ref(), &merges, ceiling, now).1)
    }

    /// The trail of one area of a project, newest first. The area comes in the query: it contains `/`.
    pub fn autopilot_area_history(
        &self,
        project: &str,
        area: &str,
    ) -> Result<wire::AutopilotAreaHistory> {
        self.crew_preview_only()?;
        valid_id(project)?;
        valid_area(area)?;
        self.store.project(project)?;
        let mut events =
            self.store
                .area_events(project, area, u32::try_from(HISTORY_MAX + 1).unwrap_or(101))?;
        let truncated = events.len() > HISTORY_MAX;
        events.truncate(HISTORY_MAX);
        Ok(wire::AutopilotAreaHistory {
            project: project.to_owned(),
            area: redact(area),
            events: events.iter().map(event_wire).collect(),
            truncated,
        })
    }

    /// The breaker's constants as they are. Proposed, not confirmed.
    pub fn autopilot_thresholds(&self) -> Result<wire::AutopilotThresholds> {
        self.crew_preview_only()?;
        Ok(wire::AutopilotThresholds {
            window_days: to_u32(WINDOW_DAYS),
            min_merges: MIN_MERGES,
            trip_reverts: TRIP_REVERTS,
            recovery_days: to_u32(RECOVERY_DAYS),
            recovery_merges: RECOVERY_MERGES,
            proposed: true,
        })
    }
}
