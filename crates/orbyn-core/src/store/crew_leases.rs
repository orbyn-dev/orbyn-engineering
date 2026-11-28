//! Lease transitions share the immediate writer: a terminal snapshot can end only its predecessor.

use orbyn_contracts::{CrewObject, CrewTimestamp, Lease, LeaseState};
use rusqlite::{Transaction, params};
use time::{Duration, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

use super::Store;
use super::crew::{belongs_to, checked, find_task, insert_object, read_object, refuse};
use crate::error::{CoreError, Result};

fn same_lineage(a: &Lease, b: &Lease) -> bool {
    a.holder == b.holder && a.mode == b.mode && a.paths == b.paths
}

/// Newest first, using the same task/holder/mode/ordered-path lineage as the core's live fold.
fn history(tx: &Transaction<'_>, task_id: &str, lease: &Lease) -> Result<Vec<Lease>> {
    let mut query = tx.prepare(
        "SELECT body FROM crew_leases WHERE task_id = ?1 AND holder = ?2 ORDER BY rowid DESC",
    )?;
    let bodies = query
        .query_map(params![task_id, lease.holder.as_str()], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut history = Vec::new();
    for body in bodies {
        let CrewObject::Lease(stored) = read_object(&body)? else {
            return Err(CoreError::Internal("a non-lease in the lease table".into()));
        };
        if same_lineage(&stored, lease) {
            history.push(stored);
        }
    }
    Ok(history)
}

fn creation_time(lease: &Lease) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(lease.created_at.as_str(), &Rfc3339)
        .map_err(|error| CoreError::Invalid(format!("invalid lease creation time: {error}")))
}

/// Content IDs remain append-only. Fractions are already part of CrewTimestamp's RFC3339 contract;
/// expiry still uses whole UTC seconds. Advance past all history, including after clock rollback.
fn append_fresh(
    tx: &Transaction<'_>,
    task_id: &str,
    candidate: &Lease,
    history: &[Lease],
) -> Result<Lease> {
    let mut lease = candidate.clone();
    let latest_time = history
        .iter()
        .map(creation_time)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max();
    if let Some(latest_time) = latest_time
        && creation_time(&lease)? <= latest_time
    {
        let stamp = latest_time
            .checked_add(Duration::nanoseconds(1))
            .ok_or_else(|| CoreError::Invalid("lease creation time is exhausted".into()))?
            .to_offset(UtcOffset::UTC)
            .format(&Rfc3339)
            .map_err(|error| CoreError::Invalid(format!("invalid lease creation time: {error}")))?;
        lease.created_at = CrewTimestamp::new(stamp).map_err(CoreError::Invalid)?;
    }
    lease.id = CrewObject::Lease(lease.clone())
        .computed_id()
        .map_err(refuse)?;
    let object = CrewObject::Lease(lease.clone());
    insert_object(tx, task_id, &object, &checked(&object)?)?;
    Ok(lease)
}

impl Store {
    /// Appends a fresh grant only while its task is live (H10), or returns its equivalent current
    /// grant on retry. Finality, lineage selection and insertion share one immediate transaction.
    pub fn add_crew_lease_grant(&self, task_id: &str, lease: &Lease) -> Result<Lease> {
        let object = CrewObject::Lease(lease.clone());
        checked(&object)?;
        if lease.state != LeaseState::Granted {
            return Err(CoreError::Invalid("a lease grant must be granted".into()));
        }
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if crate::crew::lifecycle::is_final(&task.state) {
                return Err(CoreError::RunClosed);
            }
            belongs_to(&task, &object)?;
            let history = history(tx, task_id, lease)?;
            if let Some(current) = history.first() {
                let mut retry = lease.clone();
                retry.id = current.id.clone();
                retry.created_at = current.created_at.clone();
                if &retry == current {
                    return Ok(current.clone());
                }
            }
            append_fresh(tx, task_id, lease, &history)
        })
    }

    /// Ends the named grant only if it is still current. A stale expiry/release appends nothing
    /// and returns the actual current object, so an expiry reader can keep a renewed grant live.
    pub(crate) fn end_crew_lease_snapshot(
        &self,
        task_id: &str,
        was: &Lease,
        ended: &Lease,
    ) -> Result<Option<Lease>> {
        let before = CrewObject::Lease(was.clone());
        let after = CrewObject::Lease(ended.clone());
        checked(&before)?;
        checked(&after)?;
        if was.state != LeaseState::Granted
            || !matches!(ended.state, LeaseState::Released | LeaseState::Expired)
            || !same_lineage(was, ended)
        {
            return Err(CoreError::Invalid("invalid lease end transition".into()));
        }
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            belongs_to(&task, &before)?;
            belongs_to(&task, &after)?;
            let history = history(tx, task_id, was)?;
            if history.first().is_none_or(|current| current.id != was.id) {
                return Ok(history.into_iter().next());
            }
            append_fresh(tx, task_id, ended, &history).map(Some)
        })
    }
}
