//! What the merge gate reads (slice 4j, enforcement). The mode a merge runs under is the lower of the crew's mode (the ceiling) and the
//! mode of every area its diff touches:
//!
//! - an area that follows its ceiling is on the ceiling;
//! - an area the breaker has lowered is on `On`, but **only when the ceiling is `Full`**: a ceiling of `On` or `Off` is never changed;
//! - a person's `keep_ceiling` override ignores that lowering for the area, and `off` makes it `Off` (stricter than any ceiling);
//! - an area whose state cannot be read is treated as lowered and the failure is written to the approval ledger (rule 7).
//!
//! The risk tier is not touched: a high-risk diff in a lowered area waits under the same rule `On` has, and nothing else changes.
//! The chain and the tier do not read an area in version 1.

use serde_json::json;

use super::WINDOW_DAYS;
use super::breaker::{facts_of, stored_of};
use super::decide::{Change, decide};
use super::view::word;
use crate::app::Core;
use crate::crew::autopilot::{Mode, Risk};
use crate::crew::redact;
use crate::error::{CoreError, Result};
use crate::store::{AreaMode, MergeRecord};

fn rank(mode: Mode) -> u8 {
    match mode {
        Mode::Off => 0,
        Mode::On => 1,
        Mode::Full => 2,
    }
}

/// The lower of two modes.
pub(crate) fn lower(a: Mode, b: Mode) -> Mode {
    if rank(a) <= rank(b) { a } else { b }
}

/// Why an area is on the mode it is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    Follows,
    /// The breaker lowered it (stored, or its window trips and the next merge will record it).
    Lowered,
    /// A person kept the ceiling over the lowering.
    Kept,
    /// A person set it to `Off`.
    SetOff,
    /// Its state could not be read.
    Unreadable,
}

/// The mode of one area under `ceiling`, from its stored row and whether its window trips now.
pub(crate) fn area_mode(row: Option<&AreaMode>, ceiling: Mode, trips_now: bool) -> (Mode, Why) {
    match row.and_then(|r| r.person_override.as_deref()) {
        Some("off") => return (Mode::Off, Why::SetOff),
        Some("keep_ceiling") => return (ceiling, Why::Kept),
        _ => {}
    }
    let lowered = row.is_some_and(|r| r.state == "demoted") || trips_now;
    if lowered && ceiling == Mode::Full {
        (Mode::On, Why::Lowered)
    } else {
        (ceiling, Why::Follows)
    }
}

fn plural(n: u32, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// One touched area, the mode it is on, and a sentence that names its record.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AreaLine {
    pub area: String,
    pub mode: Mode,
    pub why: Why,
    pub line: String,
}

impl AreaLine {
    /// The breaker's lowering or a person's `Off` stands for this area: it is marked in the Review queue.
    pub(crate) fn marked(&self) -> bool {
        matches!(self.why, Why::Lowered | Why::SetOff | Why::Unreadable)
    }
}

fn sentence(area: &str, mode: Mode, why: Why, merges: u32, reverts: u32) -> String {
    let record = format!(
        "{} in {WINDOW_DAYS} days · {}",
        plural(reverts, "revert"),
        plural(merges, "merge")
    );
    let area = redact(area);
    let on = word(mode);
    match why {
        Why::Follows | Why::Lowered => format!("Area {area} is on {on}: {record}"),
        Why::Kept => format!(
            "Area {area} is on {on}: a person kept the ceiling over the lowering · {record}"
        ),
        Why::SetOff => format!("Area {area} is on {on}: a person set it · {record}"),
        Why::Unreadable => format!(
            "Area {area} is on {on}: its state could not be read, so it is treated as lowered"
        ),
    }
}

/// What the gate reads for a merge.
#[derive(Clone, Debug)]
pub(crate) struct Effect {
    pub ceiling: Mode,
    /// The lower of the ceiling and every touched area.
    pub mode: Mode,
    pub lines: Vec<AreaLine>,
}

impl Effect {
    /// The lines of the areas that hold the mode below the ceiling.
    fn holding(&self) -> Vec<&AreaLine> {
        self.lines
            .iter()
            .filter(|l| rank(l.mode) < rank(self.ceiling))
            .collect()
    }

    /// Why a merge of this risk waits for a person under this mode, if it does. With no area below the ceiling this is the sentence the
    /// ceiling alone has always given.
    pub(crate) fn wait_note(&self, risk: Risk) -> Option<String> {
        let holding = self.holding();
        let areas = || {
            holding
                .iter()
                .map(|l| l.line.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        };
        match (self.mode, risk) {
            (Mode::Off, _) if holding.is_empty() => {
                Some("Autopilot is off, so a person releases every merge.".to_owned())
            }
            (Mode::Off, _) => Some(format!(
                "An area is set to Off, so a person releases every merge here. {}.",
                areas()
            )),
            (Mode::On, Risk::High) if holding.is_empty() => {
                Some("A high-risk diff waits for a person under On.".to_owned())
            }
            (Mode::On, Risk::High) => Some(format!(
                "A high-risk diff waits for a person: an area is lowered. {}.",
                areas()
            )),
            _ => None,
        }
    }
}

impl Core {
    /// The mode of each of `areas` of `project` under `ceiling` at `now`. An error means the state could not be read.
    pub(crate) fn area_lines(
        &self,
        project: &str,
        areas: &[String],
        ceiling: Mode,
        now: i64,
    ) -> Result<Vec<AreaLine>> {
        let merges: Vec<MergeRecord> = self.store.crew_merges_of_project(project)?;
        let rows = self.store.area_modes(project)?;
        Ok(areas
            .iter()
            .map(|area| {
                let row = rows.iter().find(|r| &r.area == area);
                let stored = stored_of(row);
                let facts = facts_of(&merges, area, stored, now);
                let trips_now = decide(&facts, stored, ceiling, now) == Change::Demote;
                let (mode, why) = area_mode(row, ceiling, trips_now);
                AreaLine {
                    area: area.clone(),
                    mode,
                    why,
                    line: sentence(area, mode, why, facts.window.merges, facts.window.reverts),
                }
            })
            .collect())
    }

    /// The mode a task's merge runs under: its ceiling (the crew's mode) lowered by every area its diff touches. **Fails closed:** when
    /// the areas cannot be read, each is treated as lowered and the failure is written to the approval ledger, where the person sees it.
    pub(crate) fn merge_effect(&self, task_id: &str, areas: &[String], now: i64) -> Result<Effect> {
        let ceiling = self.merge_mode(task_id)?;
        let project = self
            .project_of_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))?;
        let lines = match self.area_lines(&project, areas, ceiling, now) {
            Ok(lines) => lines,
            Err(error) => {
                tracing::warn!(%error, "could not read the areas of a merge; treating them as lowered");
                self.merge_event(
                    task_id,
                    "area_read_failed",
                    &json!({
                        "areas": areas,
                        "why": error.public_message(),
                        "treated_as": "lowered",
                    }),
                );
                areas
                    .iter()
                    .map(|area| {
                        let (mode, why) = if ceiling == Mode::Full {
                            (Mode::On, Why::Unreadable)
                        } else {
                            (ceiling, Why::Unreadable)
                        };
                        AreaLine {
                            area: area.clone(),
                            mode,
                            why,
                            line: sentence(area, mode, why, 0, 0),
                        }
                    })
                    .collect()
            }
        };
        let mode = lines.iter().fold(ceiling, |m, l| lower(m, l.mode));
        Ok(Effect {
            ceiling,
            mode,
            lines,
        })
    }

    /// The marker the risk report carries for a recorded merge: every touched area, with its mode now. A display: when the areas cannot be
    /// read the list is empty and the failure is logged (the gate, not a read, is what fails closed).
    pub(crate) fn area_modes_of(
        &self,
        task_id: &str,
        areas: &[String],
        ceiling: Mode,
        now: i64,
    ) -> Vec<AreaLine> {
        let read = self
            .project_of_task(task_id)
            .and_then(|project| match project {
                Some(project) => self.area_lines(&project, areas, ceiling, now),
                None => Ok(Vec::new()),
            });
        read.unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the areas of a merge for a view");
            Vec::new()
        })
    }
}
