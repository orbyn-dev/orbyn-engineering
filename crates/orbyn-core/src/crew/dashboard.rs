//! The Crew dashboard read (`GET /api/crew/dashboard`, W12 in `docs/crew.md`, row G18 in
//! `docs/parity.md`): one repository's figures for one window, from this device's own Crew
//! records. The contract is [docs/crew-dashboard.md](../../../docs/crew-dashboard.md), and its
//! "Answers to the open questions" section says which record each figure comes from.
//!
//! The read writes nothing, needs no person key, and is refused as not found while Crew Preview
//! is off, like every Crew route. Nothing here decides anything and nothing is invented: a
//! figure the records cannot measure is left out (`cycle`, `cost`) rather than sent as `0`, and
//! free text is scrubbed and capped as the trust read does it.

use std::collections::BTreeMap;

use orbyn_contracts as wire;
use serde_json::Value;

use crate::app::Core;
use crate::crew::{redact, valid_id};
use crate::error::{CoreError, Result};
use crate::store::{AutopilotEvent, MergeRecord};
use crate::util::parse_time;

/// A rate shows from this many merges; it is in the answer so the page never hard-codes it.
const MIN_RATE_SAMPLE: u32 = 10;
/// A median shows from this many merges.
const MIN_MEDIAN_SAMPLE: u32 = 5;
/// The longest free text the read sends, in characters. The page caps again at 60.
const TEXT_MAX: usize = 200;
/// What each list holds.
const ENGINES_MAX: usize = 12;
const AREAS_MAX: usize = 50;
const REVERTS_MAX: usize = 20;
const RECENT_MAX: usize = 20;
/// The most weeks one answer holds: 90 days is thirteen weeks and a part.
const WEEKS_MAX: usize = 14;
/// The widest UTC offset a person can name, in minutes.
const OFFSET_MINUTES_MAX: i32 = 840;
const DAY_SECS: i64 = 86_400;
const WEEK_SECS: i64 = 7 * DAY_SECS;

/// Ledger kinds that mean a person allowed, denied or dismissed a held command.
const APPROVAL_KINDS: [&str; 5] = [
    "held",
    "approved_once",
    "denied_by_person",
    "dismissed_by_person",
    "hold_used",
];

/// Ledger kinds that mean something waited on the person and none of them is an approval.
const OTHER_NEEDED_KINDS: [&str; 3] = ["person", "main_broken", "merge_waiting"];

/// One merge with the records the answer needs beside it.
struct Merge {
    record: MergeRecord,
    /// The brief's outcome, which the page shows as the title: scrubbed and capped.
    title: String,
    /// The model the brief named, when it named one.
    model: Option<String>,
    /// The moment the brief was accepted, in Unix seconds. Cycle time starts here.
    created_unix: Option<i64>,
    /// The task's rows in the approval ledger.
    events: Vec<AutopilotEvent>,
    /// The person answered an engine's question in this task.
    answered: bool,
}

impl Merge {
    /// Every ledger row of one kind.
    fn events_of<'a>(
        &'a self,
        kind: &'a str,
    ) -> impl DoubleEndedIterator<Item = &'a AutopilotEvent> {
        self.events.iter().filter(move |e| e.kind == kind)
    }

    /// What the merge needed from the person after its brief. An approval and an answer come
    /// before "other"; everything else that waited on the person is "other".
    fn needed(&self) -> wire::CrewNeeded {
        if self
            .events
            .iter()
            .any(|e| APPROVAL_KINDS.contains(&e.kind.as_str()))
        {
            wire::CrewNeeded::Approval
        } else if self.answered {
            wire::CrewNeeded::Answer
        } else if self
            .events
            .iter()
            .any(|e| OTHER_NEEDED_KINDS.contains(&e.kind.as_str()))
        {
            wire::CrewNeeded::Other
        } else {
            wire::CrewNeeded::Nothing
        }
    }

    /// True when nothing waited on the person after the brief.
    fn unaided(&self) -> bool {
        self.needed() == wire::CrewNeeded::Nothing
    }

    /// True when the watch on main reverted the merge by itself: it tells the person after
    /// reverting what it reverted, and a person's own Revert writes no such row.
    fn reverted_by_auto(&self) -> bool {
        self.events_of("person_told")
            .any(|e| e.body.get("about").and_then(Value::as_str) == Some("reverted"))
    }

    /// Whether the cause of the revert was certain. An automatic revert is only made from a
    /// certain break; a person's revert is certain only when the watch recorded that.
    fn revert_certain(&self) -> Option<bool> {
        if self.reverted_by_auto()
            || self
                .events_of("main_broken")
                .any(|e| e.body.get("certain").and_then(Value::as_bool) == Some(true))
        {
            Some(true)
        } else {
            None
        }
    }

    /// When the revert was recorded, in Unix seconds. The merge's own time stands in when the
    /// ledger row carries no readable stamp, so the revert still has its place in the list.
    fn reverted_at(&self) -> i64 {
        self.events_of("reverted")
            .filter_map(|e| parse_time(&e.at))
            .next_back()
            .unwrap_or(self.record.merged_unix)
    }

    /// The seconds from the brief to the merge, when both moments are known.
    fn cycle_secs(&self) -> Option<i64> {
        let secs = self.record.merged_unix - self.created_unix?;
        (secs >= 0).then_some(secs)
    }

    /// The task's own checks, as the proof written at the merge records them.
    fn checks(&self) -> wire::CrewChecks {
        let outcomes: Vec<&str> = self
            .record
            .proof
            .get("task_checks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|check| check.get("outcome").and_then(Value::as_str))
            .collect();
        if outcomes
            .iter()
            .any(|outcome| matches!(*outcome, "fail" | "error"))
        {
            wire::CrewChecks::Failed
        } else if outcomes.is_empty() {
            wire::CrewChecks::None_
        } else {
            wire::CrewChecks::Passed
        }
    }

    /// The cost the merge's result reported, in its own unit. Tokens are summed from the two
    /// directions an engine may report; failing that, the billing unit's millionths.
    fn cost(&self) -> Option<(String, u64)> {
        let cost = self.record.proof.get("cost")?.as_object()?;
        let field = |key: &str| cost.get(key).and_then(Value::as_u64);
        let (tokens_in, tokens_out) = (field("tokens_in"), field("tokens_out"));
        if tokens_in.is_some() || tokens_out.is_some() {
            return Some((
                "tokens".to_owned(),
                tokens_in.unwrap_or(0) + tokens_out.unwrap_or(0),
            ));
        }
        field("cost_micros").map(|micros| ("micros".to_owned(), micros))
    }
}

/// One engine's running total, and the one unit its cost may be added in.
struct EngineTotal {
    model: Option<String>,
    row: wire::CrewDashboardEngine,
    amount: u64,
    unit: Option<String>,
    /// The engine reported two units, so no total is sent: a cost is never added across units.
    mixed: bool,
}

/// The first day of the week `unix` falls in, counted from Monday, in the person's own local
/// time: the offset the client sends, as the daily digest takes it. Returned as Unix seconds,
/// so the page draws the week the person sees and not a UTC one.
fn local_week_start(unix: i64, offset_minutes: i32) -> i64 {
    let local = unix + i64::from(offset_minutes) * 60;
    // 1970-01-01 was a Thursday, so Monday is 0 and the day number moves by three.
    let weekday = (local.div_euclid(DAY_SECS) + 3).rem_euclid(7);
    (local.div_euclid(DAY_SECS) - weekday) * DAY_SECS - i64::from(offset_minutes) * 60
}

/// The median of a sorted, non-empty list.
fn median(sorted: &[i64]) -> i64 {
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2
    } else {
        sorted[middle]
    }
}

/// The nearest-rank 90th percentile of a sorted, non-empty list.
fn p90(sorted: &[i64]) -> i64 {
    sorted[(sorted.len() * 9).div_ceil(10).max(1) - 1]
}

/// Free text the page draws: scrubbed as the trust read scrubs it, then capped, so a long or a
/// hostile outcome can be neither a whole screen nor a megabyte.
fn shown(text: &str) -> String {
    redact(text).chars().take(TEXT_MAX).collect()
}

impl Core {
    /// One repository's dashboard figures for a window: `days` of 30, 60 or 90 (60 when absent),
    /// with the weeks counted from the person's local Monday through `utc_offset_minutes`.
    pub fn crew_dashboard(
        &self,
        project: &str,
        days: Option<u32>,
        utc_offset_minutes: i32,
        now: i64,
    ) -> Result<wire::CrewDashboard> {
        self.crew_preview_only()?;
        valid_id(project)?;
        let days = days.unwrap_or(wire::CREW_DASHBOARD_DAYS_DEFAULT);
        if !wire::CREW_DASHBOARD_DAYS.contains(&days) {
            return Err(CoreError::Invalid("days is 30, 60 or 90".into()));
        }
        if !(-OFFSET_MINUTES_MAX..=OFFSET_MINUTES_MAX).contains(&utc_offset_minutes) {
            return Err(CoreError::Invalid(format!(
                "utc_offset_minutes is -{OFFSET_MINUTES_MAX} to {OFFSET_MINUTES_MAX}"
            )));
        }
        // A project the service does not list is not found; a project whose record cannot be read
        // is an error, never a repository with no merges.
        self.store.project(project)?;
        let since = now - i64::from(days) * DAY_SECS;
        let window = self
            .store
            .crew_merges_of_project_since(project, since)?
            .into_iter()
            .filter(|merge| merge.merged_unix >= since)
            .map(|record| self.merge_facts(record))
            .collect::<Result<Vec<_>>>()?;
        Ok(figures(project, days, now, utc_offset_minutes, window))
    }

    /// One merge with the records the answer needs beside it. A record that cannot be read stops
    /// the read, rather than counting that merge as one that needed nothing.
    fn merge_facts(&self, record: MergeRecord) -> Result<Merge> {
        let task_id = record.task_id.clone();
        let brief = self.store.crew_brief(&task_id)?;
        let title = brief
            .as_ref()
            .map_or_else(String::new, |brief| shown(&brief.outcome));
        let model = brief
            .as_ref()
            .and_then(|brief| brief.engine.as_ref())
            .and_then(|engine| engine.model.as_ref())
            .map(|model| shown(model.as_str()));
        let created_unix = self
            .store
            .crew_task(&task_id)?
            .as_ref()
            .and_then(|task| parse_time(&task.created_at));
        let events = self.store.crew_autopilot_events(&task_id)?;
        let answered = self.task_question_answered(&task_id)?;
        Ok(Merge {
            record,
            title,
            model,
            created_unix,
            events,
            answered,
        })
    }

    /// Whether the person answered an engine's question in this task: on the lead's thread or the
    /// worker's, since a question holds whichever run asked it.
    fn task_question_answered(&self, task_id: &str) -> Result<bool> {
        let Some(run) = self.store.crew_run(task_id)? else {
            return Ok(false);
        };
        for thread in [Some(run.lead_thread_id), run.worker_thread_id] {
            let Some(thread) = thread else { continue };
            let answered = self
                .store
                .list_questions(Some(&thread), None)?
                .iter()
                .any(|question| question.state == orbyn_contracts::QuestionState::Answered);
            if answered {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Sums a window into the answer. Kept apart from the store so the arithmetic is the same for
/// every caller and can be read on its own.
fn figures(
    project: &str,
    days: u32,
    now: i64,
    offset_minutes: i32,
    merges: Vec<Merge>,
) -> wire::CrewDashboard {
    let mut unaided = 0u32;
    let mut reverted = 0u32;
    let mut reverted_auto = 0u32;
    let mut cycles: Vec<i64> = Vec::new();
    let mut weeks: BTreeMap<i64, wire::CrewDashboardWeek> = BTreeMap::new();
    let mut engines: BTreeMap<String, EngineTotal> = BTreeMap::new();
    let mut areas: BTreeMap<String, wire::CrewDashboardArea> = BTreeMap::new();
    let mut reverts: Vec<wire::CrewDashboardRevert> = Vec::new();
    let mut recent: Vec<wire::CrewDashboardRecent> = Vec::new();

    for merge in &merges {
        let was_unaided = merge.unaided();
        let was_reverted = merge.record.review_state == "reverted";
        let by_auto = was_reverted && merge.reverted_by_auto();
        unaided += u32::from(was_unaided);
        reverted += u32::from(was_reverted);
        reverted_auto += u32::from(by_auto);
        cycles.extend(merge.cycle_secs());

        let start = local_week_start(merge.record.merged_unix, offset_minutes);
        let week = weeks.entry(start).or_insert(wire::CrewDashboardWeek {
            start,
            merges: 0,
            unaided: 0,
            reverted: 0,
        });
        week.merges += 1;
        week.unaided += u32::from(was_unaided);
        week.reverted += u32::from(was_reverted);

        let engine = engines
            .entry(merge.record.engine.clone())
            .or_insert_with(|| EngineTotal {
                model: None,
                row: wire::CrewDashboardEngine {
                    name: shown(&merge.record.engine),
                    model: None,
                    merges: 0,
                    unaided: 0,
                    reverted: 0,
                    cost: None,
                },
                amount: 0,
                unit: None,
                mixed: false,
            });
        engine.row.merges += 1;
        engine.row.unaided += u32::from(was_unaided);
        engine.row.reverted += u32::from(was_reverted);
        // The model of the engine's newest merge in the window names the row.
        if merge.model.is_some() {
            engine.model = merge.model.clone();
        }
        if let Some((unit, amount)) = merge.cost() {
            if engine.unit.as_ref().is_some_and(|seen| seen != &unit) {
                engine.mixed = true;
            } else {
                engine.amount = engine.amount.saturating_add(amount);
                engine.unit = Some(unit);
            }
        }

        for area in &merge.record.areas {
            let row = areas
                .entry(area.clone())
                .or_insert_with(|| wire::CrewDashboardArea {
                    name: shown(area),
                    merges: 0,
                    unaided: 0,
                    reverted: 0,
                });
            row.merges += 1;
            row.unaided += u32::from(was_unaided);
            row.reverted += u32::from(was_reverted);
        }

        if was_reverted {
            reverts.push(wire::CrewDashboardRevert {
                title: merge.title.clone(),
                at: merge.reverted_at(),
                by: if by_auto {
                    wire::CrewRevertBy::Auto
                } else {
                    wire::CrewRevertBy::Person
                },
                certain: merge.revert_certain(),
            });
        }
        recent.push(wire::CrewDashboardRecent {
            title: merge.title.clone(),
            engine: shown(&merge.record.engine),
            at: merge.record.merged_unix,
            needed: merge.needed(),
            checks: merge.checks(),
        });
    }

    reverts.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.title.cmp(&a.title)));
    recent.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.title.cmp(&a.title)));
    let mut engine_rows: Vec<wire::CrewDashboardEngine> = engines
        .into_values()
        .map(|total| {
            let mut row = total.row;
            row.model = total.model;
            row.cost = if total.mixed {
                None
            } else {
                total.unit.map(|unit| wire::CrewDashboardCost {
                    unit,
                    amount: total.amount,
                })
            };
            row
        })
        .collect();
    engine_rows.sort_by(|a, b| b.merges.cmp(&a.merges).then_with(|| a.name.cmp(&b.name)));
    let mut area_rows: Vec<wire::CrewDashboardArea> = areas.into_values().collect();
    area_rows.sort_by(|a, b| b.merges.cmp(&a.merges).then_with(|| a.name.cmp(&b.name)));

    let (engine_rows, more_engines) = capped(engine_rows, ENGINES_MAX);
    let (area_rows, more_areas) = capped(area_rows, AREAS_MAX);
    let (reverts, more_reverts) = capped(reverts, REVERTS_MAX);
    let (recent, more_recent) = capped(recent, RECENT_MAX);

    wire::CrewDashboard {
        project: project.to_owned(),
        days,
        as_of: now,
        min_rate_sample: MIN_RATE_SAMPLE,
        min_median_sample: MIN_MEDIAN_SAMPLE,
        merges: u32::try_from(merges.len()).unwrap_or(u32::MAX),
        unaided,
        reverted,
        reverted_auto,
        reverted_by_person: reverted.saturating_sub(reverted_auto),
        cycle: cycle_row(cycles),
        weeks: week_rows(weeks, now, offset_minutes),
        engines: engine_rows,
        areas: area_rows,
        reverts,
        recent,
        truncated: more_engines || more_areas || more_reverts || more_recent,
    }
}

/// The window's weeks, oldest first: one row for every week from the first week with a merge to
/// the week of `now`, so a quiet week is a zero bar and never a missing one. At most
/// [`WEEKS_MAX`], newest kept.
///
/// No merges at all means no weeks: an empty window is empty, not one zero bar.
fn week_rows(
    mut weeks: BTreeMap<i64, wire::CrewDashboardWeek>,
    now: i64,
    offset_minutes: i32,
) -> Vec<wire::CrewDashboardWeek> {
    let Some(first) = weeks.keys().next().copied() else {
        return Vec::new();
    };
    let last = local_week_start(now, offset_minutes);
    let mut rows = Vec::new();
    let mut start = first;
    while start <= last {
        rows.push(weeks.remove(&start).unwrap_or(wire::CrewDashboardWeek {
            start,
            merges: 0,
            unaided: 0,
            reverted: 0,
        }));
        start += WEEK_SECS;
    }
    if rows.len() > WEEKS_MAX {
        rows.drain(..rows.len() - WEEKS_MAX);
    }
    rows
}

/// How long the merges took, or nothing at all when no merge measured it.
fn cycle_row(mut cycles: Vec<i64>) -> Option<wire::CrewDashboardCycle> {
    if cycles.is_empty() {
        return None;
    }
    cycles.sort_unstable();
    Some(wire::CrewDashboardCycle {
        median_s: Some(median(&cycles)),
        p90_s: Some(p90(&cycles)),
        fastest_s: cycles.first().copied(),
        slowest_s: cycles.last().copied(),
    })
}

/// At most `max` rows, and whether more were dropped.
fn capped<T>(mut rows: Vec<T>, max: usize) -> (Vec<T>, bool) {
    let more = rows.len() > max;
    rows.truncate(max);
    (rows, more)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-22T00:00:00Z, a Sunday.
    const SUNDAY: i64 = 1_795_305_600;

    #[test]
    fn a_local_week_starts_on_the_monday_the_person_sees() {
        let monday = SUNDAY - 6 * DAY_SECS;
        assert_eq!(local_week_start(SUNDAY, 0), monday);
        // The person's Monday 00:00 in a +10:00 zone falls ten hours before UTC's Monday.
        assert_eq!(local_week_start(SUNDAY, 600), monday - 10 * 3_600);
        // Behind UTC it falls after UTC's Monday.
        assert_eq!(local_week_start(SUNDAY, -660), monday + 11 * 3_600);
    }

    #[test]
    fn every_instant_of_a_local_week_names_the_same_monday() {
        let start = local_week_start(SUNDAY, 600);
        for hour in [0i64, 1, 23, 24, 6 * 24] {
            assert_eq!(
                local_week_start(SUNDAY - 6 * DAY_SECS + hour * 3_600, 600),
                start
            );
        }
        assert_eq!(local_week_start(start + WEEK_SECS, 600), start + WEEK_SECS);
    }

    #[test]
    fn the_percentiles_are_the_nearest_rank_ones() {
        assert_eq!(p90(&[1]), 1);
        assert_eq!(p90(&(1..=10).collect::<Vec<_>>()), 9);
        assert_eq!(p90(&(1..=20).collect::<Vec<_>>()), 18);
        assert_eq!(median(&[1, 2, 3]), 2);
        assert_eq!(median(&[1, 2, 3, 4]), 2);
    }

    #[test]
    fn a_week_with_no_merge_is_a_zero_bar_between_its_neighbours() {
        let now = SUNDAY;
        let mut weeks = BTreeMap::new();
        let first = local_week_start(SUNDAY - 14 * DAY_SECS, 0);
        weeks.insert(
            first,
            wire::CrewDashboardWeek {
                start: first,
                merges: 2,
                unaided: 1,
                reverted: 0,
            },
        );
        let rows = week_rows(weeks, now, 0);
        assert_eq!(rows.len(), 3, "the first week, the quiet one and this one");
        assert_eq!(rows[0].merges, 2);
        assert_eq!(rows[1].merges, 0);
        assert_eq!(rows[2].start, local_week_start(now, 0));
    }

    #[test]
    fn the_weeks_stop_at_the_newest_fourteen() {
        let mut weeks = BTreeMap::new();
        let first = local_week_start(SUNDAY - 140 * DAY_SECS, 0);
        for week in 0..21 {
            let start = first + week * WEEK_SECS;
            weeks.insert(
                start,
                wire::CrewDashboardWeek {
                    start,
                    merges: 1,
                    unaided: 1,
                    reverted: 0,
                },
            );
        }
        let rows = week_rows(weeks, SUNDAY, 0);
        assert_eq!(rows.len(), WEEKS_MAX);
        assert_eq!(rows[rows.len() - 1].start, local_week_start(SUNDAY, 0));
    }

    #[test]
    fn a_long_or_hostile_title_is_capped_and_scrubbed() {
        let long = "x".repeat(TEXT_MAX + 50);
        assert_eq!(shown(&long).chars().count(), TEXT_MAX);
        assert!(!shown("token ghp_1234567890123456789012345678901234567890").contains("ghp_1"));
    }

    #[test]
    fn no_cycle_measured_leaves_the_cycle_out() {
        assert!(cycle_row(Vec::new()).is_none());
        let row = cycle_row(vec![10, 20, 30, 40, 50]).unwrap();
        assert_eq!((row.fastest_s, row.slowest_s), (Some(10), Some(50)));
        assert_eq!(row.median_s, Some(30));
    }

    #[test]
    fn a_list_keeps_its_bound_and_says_when_it_was_cut() {
        let (kept, more) = capped(vec![1, 2, 3], 3);
        assert_eq!((kept, more), (vec![1, 2, 3], false));
        let (kept, more) = capped(vec![1, 2, 3, 4], 3);
        assert_eq!((kept, more), (vec![1, 2, 3], true));
        assert_eq!(capped(Vec::<u8>::new(), 2), (Vec::<u8>::new(), false));
    }
}
