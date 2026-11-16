//! A person's choices about one area (slice 4j, enforcement): keep the ceiling over the breaker's lowering, set the area to `Off`, or
//! go back to following the record. These are the only writes of a person here. Rules:
//!
//! - a choice never raises an area above its ceiling: only `keep_ceiling` and `off` exist, and `keep_ceiling` is refused when
//!   nothing is lowered;
//! - the person must have `acknowledged` what it does, and must have seen the record as it is (`seen`), or it is `stale`;
//! - the audit row, then the area event, then the mode row, in one transaction (`Store::record_area_override`), which also compares the
//!   `seen` record inside that transaction, and the merge lock is held around it like the breaker's writes, so a revert cannot land
//!   between the person's look and the write;
//! - the same choice twice records nothing the second time;
//! - an override ends at the next trip: a new revert in the area clears `keep_ceiling` (`breaker`).
//!
//! The routes are in `crew::changes::STATE_CHANGING`, so an agent's request to them is refused by path (N1). A plain local call: any
//! local process that holds the service token can post the same body, the limit `set_mode` has. **`keep_ceiling` loosens**: it returns
//! the area to the ceiling the crew has, so a lead that holds the token could call it to defeat the breaker. That is no stronger than
//! `PUT /autopilot/mode` setting the repository to Full, but it is a loosening. No presence grant is asked. An `off` posted while merge
//! checks are running does not stop the merge already in flight (the same as `set_mode`).

use std::time::Duration;

use orbyn_contracts as wire;

use super::decide::window_start;
use super::effective::area_mode;
use crate::app::Core;
use crate::crew::autopilot::{Mode, valid_id};
use crate::crew::redact;
use crate::error::{CoreError, Result};
use crate::store::{PersonWrite, Seen};

const REASON_MAX: usize = 300;
const AREA_MAX: usize = 300;
/// How long a person's choice waits for a merge that is being checked.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// An area as `area_of` makes it: one or two folders (or a root file), no empty or `..` segment, no leading or trailing `/`.
fn valid_area(area: &str) -> Result<()> {
    let segments: Vec<&str> = area.split('/').collect();
    let fine = !area.is_empty()
        && area.len() <= AREA_MAX
        && !area.chars().any(char::is_control)
        && segments.len() <= 2
        && segments
            .iter()
            .all(|s| !s.is_empty() && *s != "." && *s != "..");
    if fine {
        Ok(())
    } else {
        Err(CoreError::Invalid("that area is not valid".into()))
    }
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

fn seen_of(seen: &wire::AutopilotAreaSeen) -> Seen {
    Seen {
        merges: seen.merges,
        reverts: seen.reverts,
        tripped_at: seen.tripped_at,
    }
}

impl Core {
    /// Keeps the ceiling over the lowering, or sets the area to `Off`. Returns the area as it now stands.
    pub async fn autopilot_area_override(
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
        let _merging = self.merge_lock_for_person().await?;
        let snapshot = self.area_snapshot(&body.project)?;
        let ceiling = self.area_ceiling(&body.project, &snapshot, &body.area);
        let row = self.store.area_mode(&body.project, &body.area)?;
        if choice == "keep_ceiling" && !row.as_ref().is_some_and(|r| r.state == "demoted") {
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
            seen: Some(seen_of(&body.seen)),
            window_since: window_start(now),
            reason: reason.as_deref(),
            from_mode: from.name(),
            to_mode: to.name(),
            at: now,
        })?;
        self.area_row(&body.project, &body.area, now)
    }

    /// Returns the area to following the record: a person's override ends and the breaker's state stands as it is.
    pub async fn autopilot_area_follow(
        &self,
        body: &wire::AutopilotAreaFollowBody,
        now: i64,
    ) -> Result<wire::AutopilotAreaRow> {
        self.crew_preview_only()?;
        valid_id(&body.project)?;
        valid_area(&body.area)?;
        self.store.project(&body.project)?;
        let _merging = self.merge_lock_for_person().await?;
        let snapshot = self.area_snapshot(&body.project)?;
        let ceiling = self.area_ceiling(&body.project, &snapshot, &body.area);
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
            seen: body.seen.as_ref().map(seen_of),
            window_since: window_start(now),
            reason: None,
            from_mode: from.name(),
            to_mode: to.name(),
            at: now,
        })?;
        self.area_row(&body.project, &body.area, now)
    }

    /// The merge lock, as the breaker's writes hold it. A merge being checked can hold it for a long time, so the wait is bounded.
    async fn merge_lock_for_person(&self) -> Result<tokio::sync::MutexGuard<'_, ()>> {
        tokio::time::timeout(LOCK_WAIT, self.crew_merges.lock())
            .await
            .map_err(|_| {
                CoreError::Stale("A merge is being checked. Try again when it is done.".into())
            })
    }
}
