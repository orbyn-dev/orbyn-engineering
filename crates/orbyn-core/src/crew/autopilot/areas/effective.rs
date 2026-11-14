//! What the merge gate reads (slice 4j, enforcement). The mode a merge runs under is the lower of the crew's mode (the ceiling) and the
//! mode of every area its diff touches:
//!
//! - an area that follows its ceiling is on the ceiling;
//! - an area the breaker has lowered is on `On`, but **only when the ceiling is `Full`**: a ceiling of `On` or `Off` is never changed;
//! - a person's `keep_ceiling` override ignores that lowering for the area, and `off` makes it `Off` (stricter than any ceiling);
//! - an area whose state cannot be read is treated as `Off`, so every diff in it waits for a person, and the failure is written to the
//!   approval ledger (rule 7). Off, not On: a person may have set the area to Off, and a failure to read must never loosen that.
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
    /// Its state could not be read, so it is treated as Off.
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
            "Area {area} is on {on}: its state could not be read, so a person releases every merge here"
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
            (Mode::Off, _) if holding.iter().any(|l| l.why == Why::Unreadable) => Some(format!(
                "An area could not be read, so a person releases every merge here. {}.",
                areas()
            )),
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

/// What the area modes of one project are computed from, read once: its merges and its stored area rows.
pub(crate) struct AreaSnapshot {
    merges: Vec<MergeRecord>,
    rows: Vec<AreaMode>,
}

impl AreaSnapshot {
    /// The mode of each of `areas` under `ceiling` at `now`.
    pub(crate) fn lines(&self, areas: &[String], ceiling: Mode, now: i64) -> Vec<AreaLine> {
        areas
            .iter()
            .map(|area| {
                let row = self.rows.iter().find(|r| &r.area == area);
                let stored = stored_of(row);
                let facts = facts_of(&self.merges, area, stored, now);
                let trips_now = decide(&facts, stored, ceiling, now) == Change::Demote;
                let (mode, why) = area_mode(row, ceiling, trips_now);
                AreaLine {
                    area: area.clone(),
                    mode,
                    why,
                    line: sentence(area, mode, why, facts.window.merges, facts.window.reverts),
                }
            })
            .collect()
    }

    /// The ceiling that applies to an area: the mode of the crew whose merge touched it last, else the repository's. Used for the trail of
    /// a person's choice, which belongs to no single merge.
    pub(crate) fn latest_crew_of(&self, area: &str) -> Option<&str> {
        self.merges
            .iter()
            .filter(|m| m.areas.iter().any(|a| a == area))
            .max_by_key(|m| (m.merged_unix, m.id))
            .map(|m| m.crew_id.as_str())
    }
}

impl Core {
    /// The merges and area rows of `project`, read once. An error means the state could not be read.
    pub(crate) fn area_snapshot(&self, project: &str) -> Result<AreaSnapshot> {
        Ok(AreaSnapshot {
            merges: self.store.crew_merges_of_project(project)?,
            rows: self.store.area_modes(project)?,
        })
    }

    /// The mode of each of `areas` of `project` under `ceiling` at `now`. An error means the state could not be read.
    pub(crate) fn area_lines(
        &self,
        project: &str,
        areas: &[String],
        ceiling: Mode,
        now: i64,
    ) -> Result<Vec<AreaLine>> {
        Ok(self.area_snapshot(project)?.lines(areas, ceiling, now))
    }

    /// The ceiling of an area for a person's choice: the crew that merged into it last (`crew_autopilot_mode`, the same function the gate
    /// uses), else the repository's.
    pub(crate) fn area_ceiling(&self, project: &str, snapshot: &AreaSnapshot, area: &str) -> Mode {
        match snapshot.latest_crew_of(area) {
            Some(crew) => self.crew_autopilot_mode(project, crew),
            None => self.crew_project_mode(project),
        }
    }

    /// The mode a task's merge runs under: its ceiling (the crew's mode) lowered by every area its diff touches. **Fails closed:** when
    /// the areas cannot be read, each is treated as `Off` and the failure is written to the approval ledger, where the person sees it.
    pub(crate) fn merge_effect(&self, task_id: &str, areas: &[String], now: i64) -> Result<Effect> {
        let ceiling = self.merge_mode(task_id)?;
        let project = self
            .project_of_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))?;
        let lines = match self.area_lines(&project, areas, ceiling, now) {
            Ok(lines) => lines,
            Err(error) => {
                tracing::warn!(%error, "could not read the areas of a merge; treating them as Off");
                self.merge_event(
                    task_id,
                    "area_read_failed",
                    &json!({
                        "areas": areas,
                        "why": error.public_message(),
                        "treated_as": "off",
                    }),
                );
                areas
                    .iter()
                    .map(|area| AreaLine {
                        area: area.clone(),
                        mode: Mode::Off,
                        why: Why::Unreadable,
                        line: sentence(area, Mode::Off, Why::Unreadable, 0, 0),
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
        self.area_modes_cached(&mut AreaCache::new(), task_id, areas, ceiling, now)
    }

    /// [`Self::area_modes_of`] reading each project's merges and area rows once for a whole page, not once per item.
    pub(crate) fn area_modes_cached(
        &self,
        cache: &mut AreaCache,
        task_id: &str,
        areas: &[String],
        ceiling: Mode,
        now: i64,
    ) -> Vec<AreaLine> {
        let project = match self.project_of_task(task_id) {
            Ok(Some(project)) => project,
            Ok(None) => return Vec::new(),
            Err(error) => {
                tracing::warn!(%error, "could not read the areas of a merge for a view");
                return Vec::new();
            }
        };
        let snapshot = cache.entry(project.clone()).or_insert_with(|| {
            self.area_snapshot(&project)
                .map_err(|error| {
                    tracing::warn!(%error, "could not read the areas of a merge for a view");
                })
                .ok()
        });
        snapshot
            .as_ref()
            .map(|s| s.lines(areas, ceiling, now))
            .unwrap_or_default()
    }
}

/// The snapshots one page of a view reads, by project.
pub(crate) type AreaCache = std::collections::HashMap<String, Option<AreaSnapshot>>;
