//! File leases (SPEC.md H10 and H11): the core grants the paths a brief asks for to its worker,
//! refuses a write on a protected path, outside the brief's write files, or for a reviewer, and
//! reads an expiry when a lease is used, never from a timer. A lease that has lapsed is
//! recorded as `expired` the first time anyone looks, and then blocks nobody.

use std::collections::HashMap;

use orbyn_contracts::{
    Brief, BriefKind, CrewAuthor, CrewError, CrewId, CrewMember, CrewObject, CrewPathPattern,
    CrewRole, CrewShortText, CrewTimestamp, CrewToken, CrewUtcSeconds, ErrorCode, Lease, LeaseMode,
    LeaseState,
};
use time::OffsetDateTime;

use crate::app::Core;
use crate::error::CoreError;

fn invalid(message: impl Into<String>) -> CrewError {
    CrewError::new(ErrorCode::InvalidObject, message)
}

pub(super) fn internal(error: CoreError) -> CrewError {
    invalid(error.public_message())
}

pub(super) fn core_author() -> CrewAuthor {
    CrewAuthor {
        role: CrewRole::Core,
        engine: CrewToken::new("orbyn").expect("a token"),
        member: None,
    }
}

fn utc_text(unix: i64) -> Result<String, CrewError> {
    OffsetDateTime::from_unix_timestamp(unix)
        .map_err(|e| invalid(e.to_string()))?
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| invalid(e.to_string()))
}

/// Whole seconds in UTC, so two stamps compare as text.
pub(super) fn utc_seconds(unix: i64) -> Result<CrewUtcSeconds, CrewError> {
    CrewUtcSeconds::new(utc_text(unix)?).map_err(invalid)
}

pub(super) fn timestamp(unix: i64) -> Result<CrewTimestamp, CrewError> {
    CrewTimestamp::new(utc_text(unix)?).map_err(invalid)
}

/// A write lease may never reach a protected path (H11).
pub fn refuse_protected(
    writes: &[&CrewPathPattern],
    protected: &[CrewPathPattern],
) -> Result<(), CrewError> {
    for write in writes {
        if let Some(path) = protected.iter().find(|p| write.overlaps(p)) {
            return Err(CrewError::new(
                ErrorCode::ProtectedPathConflict,
                format!("{write} reaches {path}, which the checks use; a worker never writes it"),
            ));
        }
    }
    Ok(())
}

/// What the core grants for a task: the paths the brief asks for, one lease per mode.
pub struct Grant<'a> {
    pub brief: &'a Brief,
    pub run_id: &'a CrewId,
    pub holder: &'a CrewMember,
    pub now: i64,
}

impl Grant<'_> {
    fn lease(
        &self,
        paths: Vec<CrewPathPattern>,
        mode: LeaseMode,
        state: LeaseState,
        expires_at: CrewUtcSeconds,
        reason: Option<&str>,
    ) -> Result<Lease, CrewError> {
        let mut lease = Lease {
            v: 0,
            id: self.brief.id.clone(),
            crew_id: self.brief.crew_id.clone(),
            author: core_author(),
            created_at: timestamp(self.now)?,
            run_id: self.run_id.clone(),
            holder: self.holder.clone(),
            paths,
            mode,
            state,
            expires_at,
            to: None,
            reason: reason
                .map(|text| CrewShortText::new(text).map_err(invalid))
                .transpose()?,
        };
        lease.validate()?;
        lease.id = CrewObject::Lease(lease.clone()).computed_id()?;
        Ok(lease)
    }

    /// The leases the brief asks for, valid until its budget's minutes run out. A reviewer
    /// is given reads only; a write to it was refused before this.
    pub fn leases(&self) -> Result<Vec<Lease>, CrewError> {
        let until = utc_seconds(self.now + i64::from(self.brief.budget.minutes) * 60)?;
        let mut leases = Vec::new();
        for mode in [LeaseMode::Read, LeaseMode::Write] {
            let paths: Vec<CrewPathPattern> = self
                .brief
                .files
                .iter()
                .filter(|f| f.mode == mode)
                .map(|f| f.path.clone())
                .collect();
            if !paths.is_empty() {
                leases.push(self.lease(paths, mode, LeaseState::Granted, until.clone(), None)?);
            }
        }
        Ok(leases)
    }

    /// The same lease, after it ended: `expired` or `released`.
    pub fn ended(&self, was: &Lease, state: LeaseState, reason: &str) -> Result<Lease, CrewError> {
        self.lease(
            was.paths.clone(),
            was.mode,
            state,
            was.expires_at.clone(),
            Some(reason),
        )
    }
}

/// A reviewer reads and reports; it is never given a write lease (H10).
pub fn is_reviewer(brief: &Brief) -> bool {
    brief.kind == BriefKind::Review
}

impl Core {
    /// The leases in `crew_id` that are held now. A granted lease past its `expires_at` is
    /// written as `expired` here, on use, and left out; so it blocks no other member.
    pub(super) fn live_crew_leases(
        &self,
        crew_id: &str,
        now: i64,
    ) -> Result<Vec<(String, Lease)>, CrewError> {
        let all = self.store.crew_leases_in_crew(crew_id).map_err(internal)?;
        // The newest object of each lineage says where the lease stands.
        let mut newest: HashMap<(String, String, bool), (String, Lease)> = HashMap::new();
        for (task, lease) in all {
            let key = (
                task.clone(),
                lease.holder.as_str().to_owned(),
                lease.mode == LeaseMode::Write,
            );
            newest.insert(key, (task, lease));
        }
        let today = utc_seconds(now)?;
        let mut live = Vec::new();
        for (task, lease) in newest.into_values() {
            if lease.state != LeaseState::Granted {
                continue;
            }
            if lease.expires_at.as_str() > today.as_str() {
                live.push((task, lease));
                continue;
            }
            let run_id = lease.run_id.clone();
            let holder = lease.holder.clone();
            let brief = self
                .store
                .crew_brief(&task)
                .map_err(internal)?
                .ok_or_else(|| invalid("a lease without its brief"))?;
            let grant = Grant {
                brief: &brief,
                run_id: &run_id,
                holder: &holder,
                now,
            };
            let ended = grant.ended(&lease, LeaseState::Expired, "The lease ran out.")?;
            self.store
                .add_crew_object(&task, &CrewObject::Lease(ended))
                .map_err(internal)?;
        }
        live.sort_by(|a, b| a.1.created_at.cmp(&b.1.created_at));
        Ok(live)
    }

    /// `lease_conflict` when a write the brief asks for overlaps a lease another member holds
    /// now, or a read overlaps someone's write (H10). The lead is told; nothing is stored.
    pub(super) fn refuse_lease_conflict(
        &self,
        brief: &Brief,
        holder: &CrewMember,
        now: i64,
    ) -> Result<(), CrewError> {
        let live = self.live_crew_leases(brief.crew_id.as_str(), now)?;
        for file in &brief.files {
            let wanted = Lease {
                v: 0,
                id: brief.id.clone(),
                crew_id: brief.crew_id.clone(),
                author: core_author(),
                created_at: timestamp(now)?,
                run_id: CrewId::new("candidate").map_err(invalid)?,
                holder: holder.clone(),
                paths: vec![file.path.clone()],
                mode: file.mode,
                state: LeaseState::Requested,
                expires_at: utc_seconds(now)?,
                to: None,
                reason: None,
            };
            if let Some((_, held)) = live.iter().find(|(_, held)| wanted.conflicts_with(held)) {
                return Err(CrewError::new(
                    ErrorCode::LeaseConflict,
                    format!(
                        "{} overlaps a lease held by {}; the lead decides who goes first",
                        file.path, held.holder
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Whether `member` may use `path` for `mode` now, for the task. The expiry is read here.
    /// `lease_not_held`, `lease_expired`, `protected_path_conflict` and `not_authorized` are
    /// the refusals (SPEC.md section 9): the last when the path is outside the brief's files
    /// or the member is a reviewer asking to write.
    pub fn crew_check_lease(
        &self,
        task_id: &str,
        member: &str,
        path: &CrewPathPattern,
        mode: LeaseMode,
    ) -> Result<(), CrewError> {
        let task = self
            .store
            .crew_task(task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no such task"))?;
        let brief = self
            .store
            .crew_brief(task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no such brief"))?;
        if mode == LeaseMode::Write {
            if is_reviewer(&brief) {
                return Err(CrewError::new(
                    ErrorCode::NotAuthorized,
                    "a reviewer is never given a write lease",
                ));
            }
            if let Some(key) = self.store.crew_answer_key(task_id).map_err(internal)? {
                refuse_protected(&[path], &key.protected_paths)?;
            }
        }
        let now = crate::util::unix_now();
        let live = self.live_crew_leases(&task.crew_id, now)?;
        let held: Vec<&Lease> = live
            .iter()
            .filter(|(owner, lease)| owner == task_id && lease.holder.as_str() == member)
            .map(|(_, lease)| lease)
            .collect();
        if held.is_empty() {
            let lapsed = self
                .store
                .crew_leases(task_id)
                .map_err(internal)?
                .iter()
                .any(|l| l.holder.as_str() == member && l.state == LeaseState::Expired);
            return Err(if lapsed {
                CrewError::new(ErrorCode::LeaseExpired, "the lease ran out")
            } else {
                CrewError::new(ErrorCode::LeaseNotHeld, "no lease is held for this task")
            });
        }
        let covered = held.iter().any(|lease| {
            let right_mode = lease.mode == mode || lease.mode == LeaseMode::Write;
            right_mode && lease.paths.iter().any(|p| p.overlaps(path))
        });
        if covered {
            Ok(())
        } else {
            Err(CrewError::new(
                ErrorCode::NotAuthorized,
                format!("{path} is outside the files the brief asked for"),
            ))
        }
    }

    /// Writes `released` for every lease the task still holds, when its run ends.
    pub(super) fn release_crew_leases(&self, task_id: &str) -> Result<(), CrewError> {
        let Some(task) = self.store.crew_task(task_id).map_err(internal)? else {
            return Ok(());
        };
        let Some(brief) = self.store.crew_brief(task_id).map_err(internal)? else {
            return Ok(());
        };
        let now = crate::util::unix_now();
        let live = self.live_crew_leases(&task.crew_id, now)?;
        for (owner, lease) in live.into_iter().filter(|(owner, _)| owner == task_id) {
            let grant = Grant {
                brief: &brief,
                run_id: &lease.run_id,
                holder: &lease.holder,
                now,
            };
            let ended = grant.ended(&lease, LeaseState::Released, "The run ended.")?;
            self.store
                .add_crew_object(&owner, &CrewObject::Lease(ended))
                .map_err(internal)?;
        }
        Ok(())
    }
}
