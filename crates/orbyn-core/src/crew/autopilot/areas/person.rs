//! A person's choices about one area (slice 4j, enforcement): keep the ceiling over the breaker's lowering, set the area to `Off`, or
//! go back to following the record. These are the only writes of a person here. Rules:
//!
//! - a choice never raises an area above its ceiling: only `keep_ceiling` and `off` exist, and `keep_ceiling` is refused when
//!   nothing is lowered;
//! - the person must have `acknowledged` what it does, and must have seen the record as it is (`seen`), or it is `stale`;
//! - the audit row, then the area event, then the mode row, in one transaction (`Store::record_area_override`);
//! - the same choice twice records nothing the second time;
//! - an override ends at the next trip: a new revert in the area clears `keep_ceiling` (`breaker`).
//!
//! The routes are in `crew::changes::STATE_CHANGING`, so an agent's request to them is refused by path (N1). A plain local call: any
//! local process that holds the service token can post the same body, the limit `set_mode` has (and it can already set the whole
//! repository to Full). No presence grant is asked.

use orbyn_contracts as wire;
use serde_json::Value;

use super::breaker::record_json;
use super::decide::{count, window_start};
use super::effective::area_mode;
use crate::app::Core;
use crate::crew::autopilot::Mode;
use crate::crew::autopilot::valid_id;
use crate::crew::redact;
use crate::error::{CoreError, Result};
use crate::store::{PersonWrite, SeenTrip};

const REASON_MAX: usize = 300;
const AREA_MAX: usize = 300;

fn valid_area(area: &str) -> Result<()> {
    if area.is_empty() || area.len() > AREA_MAX || area.chars().any(char::is_control) {
        return Err(CoreError::Invalid("that area is not valid".into()));
    }
    Ok(())
}

fn reason_of(reason: Option<&str>) -> Result<Option<String>> {
    let Some(text) = reason.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > REASON_MAX {
        return Err(CoreError::Invalid(format!(
            "a reason is at most {REASON_MAX} characters"
        )));
    }
    Ok(Some(redact(text)))
}

impl Core {
    /// The window's record and the trip a person is looking at, as a `seen` must match them.
    fn check_seen(
        &self,
        project: &str,
        area: &str,
        seen: &wire::AutopilotAreaSeen,
        now: i64,
    ) -> Result<(Value, SeenTrip)> {
        let merges = self.store.crew_merges_of_project(project)?;
        let window = count(&merges, area, window_start(now));
        if (seen.merges, seen.reverts) != (window.merges, window.reverts) {
            return Err(CoreError::Stale(
                "This area's record changed since you looked. Reload it and choose again.".into(),
            ));
        }
        Ok((record_json(window), SeenTrip(seen.tripped_at)))
    }

    /// Keeps the ceiling over the lowering, or sets the area to `Off`. Returns the area as it now stands.
    pub fn autopilot_area_override(
        &self,
        body: &wire::AutopilotAreaOverrideBody,
        now: i64,
    ) -> Result<wire::AutopilotAreaRow> {
        self.crew_preview_only()?;
        valid_id(&body.project)?;
        valid_area(&body.area)?;
        self.store.project(&body.project)?;
        let choice = match body.choice.as_str() {
            c @ ("keep_ceiling" | "off") => c,
            _ => {
                return Err(CoreError::Invalid(
                    "choose keep_ceiling or off; a choice never raises an area above its ceiling"
                        .into(),
                ));
            }
        };
        if !body.acknowledged {
            return Err(CoreError::Invalid(
                "Confirm that you read what this does.".into(),
            ));
        }
        let reason = reason_of(body.reason.as_deref())?;
        let (record, seen) = self.check_seen(&body.project, &body.area, &body.seen, now)?;
        let ceiling = self.crew_project_mode(&body.project);
        let row = self.store.area_mode(&body.project, &body.area)?;
        let lowered = row.as_ref().is_some_and(|r| r.state == "demoted");
        if choice == "keep_ceiling" && !(lowered && ceiling == Mode::Full) {
            return Err(CoreError::Invalid(
                "This area is not lowered, so there is nothing to keep.".into(),
            ));
        }
        let (from, _) = area_mode(row.as_ref(), ceiling, false);
        let to = if choice == "off" { Mode::Off } else { ceiling };
        self.store.record_area_override(&PersonWrite {
            project_id: &body.project,
            area: &body.area,
            choice: Some(choice),
            seen: Some(seen),
            reason: reason.as_deref(),
            from_mode: from.name(),
            to_mode: to.name(),
            record: &record,
            at: now,
        })?;
        self.area_row(&body.project, &body.area, now)
    }

    /// Returns the area to following the record: a person's override ends and the breaker's state stands as it is.
    pub fn autopilot_area_follow(
        &self,
        body: &wire::AutopilotAreaFollowBody,
        now: i64,
    ) -> Result<wire::AutopilotAreaRow> {
        self.crew_preview_only()?;
        valid_id(&body.project)?;
        valid_area(&body.area)?;
        self.store.project(&body.project)?;
        let merges = self.store.crew_merges_of_project(&body.project)?;
        let (record, seen) = match &body.seen {
            Some(seen) => {
                let (record, seen) = self.check_seen(&body.project, &body.area, seen, now)?;
                (record, Some(seen))
            }
            None => (
                record_json(count(&merges, &body.area, window_start(now))),
                None,
            ),
        };
        let ceiling = self.crew_project_mode(&body.project);
        let row = self.store.area_mode(&body.project, &body.area)?;
        let (from, _) = area_mode(row.as_ref(), ceiling, false);
        let followed = row.as_ref().map(|r| crate::store::AreaMode {
            person_override: None,
            ..r.clone()
        });
        let (to, _) = area_mode(followed.as_ref(), ceiling, false);
        self.store.record_area_override(&PersonWrite {
            project_id: &body.project,
            area: &body.area,
            choice: None,
            seen,
            reason: None,
            from_mode: from.name(),
            to_mode: to.name(),
            record: &record,
            at: now,
        })?;
        self.area_row(&body.project, &body.area, now)
    }
}
