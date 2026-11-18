//! What the routes read: the log of one project and one escape with its trail. Read-only; a GET never writes.

use orbyn_contracts as wire;

use super::thresholds::escape_thresholds;
use crate::app::Core;
use crate::crew::autopilot::valid_id;
use crate::crew::{Page, redact};
use crate::error::{CoreError, Result};
use crate::store::{EVENTS_PER_ESCAPE_MAX, EscapeEventRecord, EscapeRecord};
use crate::util::unix_now;

fn source_words(source: wire::EscapeSource) -> &'static str {
    match source {
        wire::EscapeSource::Revert => "the merge was reverted",
        wire::EscapeSource::PersonMarked => "a person marked it",
        wire::EscapeSource::PostMergeCheck => "a check failed after the merge",
        wire::EscapeSource::FollowUpFix => "a later fix touched the same files",
    }
}

/// One plain sentence. A possible escape says it is not confirmed; only a person's confirm makes it count as an escape.
fn line(record: &EscapeRecord) -> String {
    let why = source_words(record.source);
    match record.state {
        wire::EscapeState::Possible => {
            format!("Possible escape: {why}. A person has not confirmed it.")
        }
        wire::EscapeState::Confirmed => format!("Confirmed by a person: {why}."),
        wire::EscapeState::Dismissed => format!("Dismissed: {why}."),
    }
}

fn row_wire(record: &EscapeRecord) -> wire::EscapeRow {
    wire::EscapeRow {
        id: record.id.clone(),
        project: record.project_id.clone(),
        merge_task_id: record.merge_task_id.clone(),
        source: record.source,
        state: record.state,
        gap: record.gap,
        gap_by: record.gap_by,
        engine: redact(&record.engine),
        model: record.model.as_deref().map(redact),
        areas: record.areas.iter().map(|a| redact(a)).collect(),
        lane: record.lane.as_deref().map(redact),
        reason: record.reason.as_deref().map(redact),
        line: line(record),
        found_unix: record.found_unix,
        decided_unix: record.decided_unix,
        decided_by: record.decided_by,
    }
}

fn event_wire(event: &EscapeEventRecord) -> wire::EscapeEventRow {
    wire::EscapeEventRow {
        id: event.id,
        kind: event.kind,
        from_state: event.from_state,
        to_state: event.to_state,
        decided_by: event.decided_by,
        cause: redact(&event.cause),
        at: event.at,
    }
}

fn state_filter(state: Option<&str>) -> Result<Option<wire::EscapeState>> {
    match state {
        None => Ok(None),
        Some("possible") => Ok(Some(wire::EscapeState::Possible)),
        Some("confirmed") => Ok(Some(wire::EscapeState::Confirmed)),
        Some("dismissed") => Ok(Some(wire::EscapeState::Dismissed)),
        Some(_) => Err(CoreError::Invalid(
            "state is possible, confirmed or dismissed".into(),
        )),
    }
}

impl Core {
    /// A project's escape log: counts by engine for the report window (possible, confirmed and dismissed apart; no rate), and one
    /// page of rows, newest found first, optionally of one `state`.
    pub fn crew_escape_log(
        &self,
        project: &str,
        state: Option<&str>,
        page: Page,
    ) -> Result<wire::EscapeLog> {
        self.crew_preview_only()?;
        valid_id(project)?;
        let state = state_filter(state)?;
        self.store.project(project)?;
        let thresholds = escape_thresholds(&self.store)?;
        let now = unix_now();
        let since = now - i64::from(thresholds.report_days) * 86_400;
        let tallies = self.store.crew_escape_tallies(project, since)?;
        let (rows, total) = self
            .store
            .crew_escape_page(project, state, page.limit, page.offset)?;
        let engines: Vec<wire::EscapeEngineCounts> = tallies
            .iter()
            .map(|(engine, t)| wire::EscapeEngineCounts {
                engine: redact(engine),
                merges: t.merges,
                possible: t.possible,
                confirmed: t.confirmed,
                dismissed: t.dismissed,
                low_sample: t.merges < thresholds.min_merges,
            })
            .collect();
        let sum = |f: fn(&crate::store::EscapeTally) -> u32| tallies.values().map(f).sum::<u32>();
        let merges = sum(|t| t.merges);
        Ok(wire::EscapeLog {
            project: project.to_owned(),
            as_of: now,
            window_days: thresholds.report_days,
            merges,
            possible: sum(|t| t.possible),
            confirmed: sum(|t| t.confirmed),
            dismissed: sum(|t| t.dismissed),
            low_sample: merges < thresholds.min_merges,
            min_merges: thresholds.min_merges,
            engines,
            total,
            rows: rows.iter().map(row_wire).collect(),
        })
    }

    /// One escape and its audit trail, oldest first.
    pub fn crew_escape_detail(&self, id: &str) -> Result<wire::EscapeDetail> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let record = self
            .store
            .crew_escape(id)?
            .ok_or_else(|| CoreError::NotFound("escape".into()))?;
        let mut events = self
            .store
            .crew_escape_events(id, EVENTS_PER_ESCAPE_MAX + 1)?;
        let truncated = events.len() > EVENTS_PER_ESCAPE_MAX as usize;
        events.truncate(EVENTS_PER_ESCAPE_MAX as usize);
        Ok(wire::EscapeDetail {
            escape: row_wire(&record),
            events: events.iter().map(event_wire).collect(),
            truncated,
        })
    }
}
