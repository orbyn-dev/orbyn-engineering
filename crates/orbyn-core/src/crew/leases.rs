//! File leases (SPEC.md H10 and H11): the core grants the paths a brief asks for to its worker,
//! refuses a write on a protected path, outside the brief's write files, or for a reviewer, and
//! reads an expiry when a lease is used, never from a timer. A lease that has lapsed is
//! recorded as `expired` the first time anyone looks, and then blocks nobody. A member asks for a
//! lease and releases one it holds through `crew_lease_request` and `crew_lease_release`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use orbyn_contracts::{
    ActionKind, Brief, BriefKind, CrewAuthor, CrewError, CrewId, CrewMember, CrewObject,
    CrewPathPattern, CrewRole, CrewShortText, CrewTimestamp, CrewToken, CrewUtcSeconds, ErrorCode,
    Lease, LeaseMode, LeaseState, reject_floats,
};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;

use super::guard::refuse_secret;
use crate::app::Core;
use crate::error::CoreError;
use crate::governance::approvals::{ActionAsk, resolve_existing};
use crate::tools::mcp::McpBinding;

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

/// The member lease tools (SPEC.md section 5): a member asks for a lease and releases one it holds.
pub const LEASE_REQUEST_TOOL: &str = "crew_lease_request";
pub const LEASE_RELEASE_TOOL: &str = "crew_lease_release";
pub const LEASE_TOOLS: [&str; 2] = [LEASE_REQUEST_TOOL, LEASE_RELEASE_TOOL];

/// The fields a member's lease draft may not send: the envelope and the two the core fills in.
const LEASE_STAMPED: [&str; 9] = [
    "type",
    "v",
    "id",
    "crew_id",
    "author",
    "created_at",
    "run_id",
    "holder",
    "state",
];

/// The two tools as `tools/list` shows them.
pub fn specs() -> Vec<Value> {
    vec![
        json!({
            "name": LEASE_REQUEST_TOOL,
            "description": "Preview, for a crew member: ask for a file lease. `paths` are repository paths, or a \
                directory tree ending `/**`; `mode` is `read` or `write`; `expires_at` is a UTC time to the whole \
                second, later than the request. The core grants it when it overlaps no lease another member holds; a \
                write lease must lie inside the brief's write files, is never granted to a reviewer, and never reaches \
                a protected path. Refused once the run has ended. Returns the lease id and state.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "paths": { "type": "array", "items": { "type": "string" } },
                    "mode": { "type": "string" },
                    "expires_at": { "type": "string" },
                },
                "required": ["paths", "mode", "expires_at"],
            },
        }),
        json!({
            "name": LEASE_RELEASE_TOOL,
            "description": "Preview, for a crew member: release the lease you hold on exactly these `paths`. Each \
                lease of yours naming exactly them is written `released`; a path you do not hold is refused. Refused \
                once the run has ended.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "paths": { "type": "array", "items": { "type": "string" } },
                },
                "required": ["paths"],
            },
        }),
    ]
}

/// A lease draft's paths, each a path or a tree pattern.
fn draft_paths(fields: &Map<String, Value>) -> Result<Vec<CrewPathPattern>, CrewError> {
    let Some(values) = fields.get("paths").and_then(Value::as_array) else {
        return Err(invalid("paths: required, a list of path patterns"));
    };
    values
        .iter()
        .map(|value| {
            let text = value
                .as_str()
                .ok_or_else(|| invalid("paths: each one is a path pattern"))?;
            CrewPathPattern::new(text).map_err(|why| invalid(format!("paths: {why}")))
        })
        .collect()
}

/// The arguments of a lease draft, checked for the fields the core stamps and for any the tool does
/// not take.
fn lease_draft<'a>(
    tool: &str,
    args: &'a Value,
    names: &[&str],
) -> Result<&'a Map<String, Value>, CrewError> {
    let Some(fields) = args.as_object() else {
        return Err(invalid("arguments: an object"));
    };
    if let Some(key) = fields
        .keys()
        .find(|key| LEASE_STAMPED.contains(&key.as_str()))
    {
        return Err(invalid(format!(
            "{key}: the core stamps this field; a draft does not carry it"
        )));
    }
    if let Some(extra) = fields.keys().find(|key| !names.contains(&key.as_str())) {
        return Err(invalid(format!("{extra}: not an argument of {tool}")));
    }
    reject_floats(args)?;
    Ok(fields)
}

fn draft_mode(fields: &Map<String, Value>) -> Result<LeaseMode, CrewError> {
    match fields.get("mode").and_then(Value::as_str) {
        Some("read") => Ok(LeaseMode::Read),
        Some("write") => Ok(LeaseMode::Write),
        Some(_) => Err(invalid("mode: read or write")),
        None => Err(invalid("mode: required, read or write")),
    }
}

fn draft_expiry(fields: &Map<String, Value>) -> Result<CrewUtcSeconds, CrewError> {
    let text = fields
        .get("expires_at")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("expires_at: required"))?;
    CrewUtcSeconds::new(text).map_err(|why| invalid(format!("expires_at: {why}")))
}

/// Whether a granted pattern covers what a requested one can name: the same file, or a tree and
/// anything inside it. A request wider than the grant (`src/**` against `src/lib.rs`) is not covered.
fn covers(granted: &CrewPathPattern, wanted: &CrewPathPattern) -> bool {
    let (granted_dir, granted_tree) = granted.parts();
    let (wanted_dir, wanted_tree) = wanted.parts();
    if granted_tree {
        granted_dir == wanted_dir || inside(granted_dir, wanted_dir)
    } else {
        !wanted_tree && granted_dir == wanted_dir
    }
}

fn inside(dir: &str, path: &str) -> bool {
    path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/'
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
        // The newest object of each lineage says where the lease stands. A lineage is one path
        // set in one mode, so the brief's own grant and a member's request for a narrower set
        // are separate grants: a request or a release never hides the brief's files, and the
        // check below still reads both.
        let mut newest: HashMap<(String, String, bool, Vec<String>), (String, Lease)> =
            HashMap::new();
        for (task, lease) in all {
            let key = (
                task.clone(),
                lease.holder.as_str().to_owned(),
                lease.mode == LeaseMode::Write,
                lease
                    .paths
                    .iter()
                    .map(|path| path.as_str().to_owned())
                    .collect(),
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

    /// The gate's lease check (H10): a worker's file-change tool may name only paths the lease
    /// its brief granted covers. The refusal is the lease's own, made before the write, not left
    /// to the verifier's file_scope to notice afterwards. The run's write scope has already
    /// refused a path outside the checkout, so one that cannot be read is left to it. It never
    /// fails open: an error reading the task, brief or leases is a refusal in that error's words.
    pub(crate) fn crew_refuse_lease_write(
        &self,
        task_id: &str,
        member: &str,
        worktree: &Path,
        ask: &ActionAsk,
    ) -> Option<String> {
        if ask.kind != ActionKind::FileChange {
            return None;
        }
        let Ok(worktree) = crate::util::canonicalize(worktree) else {
            return Some(
                "Orbyn could not read this task's checkout, so a worker may not write here."
                    .to_owned(),
            );
        };
        let single = std::slice::from_ref(&ask.target);
        let paths: &[String] = if ask.paths.is_empty() {
            single
        } else {
            &ask.paths
        };
        for path in paths {
            // A relative path is joined to the task's worktree here. The run's write scope joins it to
            // the engine's own cwd (the library for Claude, the checkout for an engine that runs in
            // it), so the two agree only when those are the same; where they differ, the write scope
            // has already refused or resolved the path somewhere this check would not read.
            let absolute = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                worktree.join(path)
            };
            // `..`, links and a path that is not there yet resolve the way the write scope
            // resolves them, so the lease is read against where the bytes land.
            let Some(resolved) = resolve_existing(&absolute) else {
                // A `..` that climbs past the root: the write scope refuses it as outside the checkout.
                continue;
            };
            let Ok(relative) = resolved.strip_prefix(&worktree) else {
                // Resolves outside the worktree: not a leasable path, and the write scope refuses it.
                continue;
            };
            let mut parts = Vec::new();
            for part in relative.components() {
                let Some(text) = part.as_os_str().to_str() else {
                    return Some(
                        "A path Orbyn cannot read is not one a lease can name, so a worker may not write it."
                            .to_owned(),
                    );
                };
                parts.push(text.to_owned());
            }
            let relative = parts.join("/");
            let Ok(pattern) = CrewPathPattern::new(&relative) else {
                return Some(format!(
                    "{relative} is not a path a lease can name, so a worker may not write it"
                ));
            };
            if let Err(refusal) = self.crew_check_lease(task_id, member, &pattern, LeaseMode::Write)
            {
                return Some(refusal.message);
            }
        }
        None
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

    /// Runs a member lease tool for the MCP service.
    pub(super) async fn crew_lease_tool(
        self: &std::sync::Arc<Self>,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, tool));
        }
        if tool == LEASE_REQUEST_TOOL {
            self.crew_lease_request_tool(binding, args).await
        } else {
            self.crew_lease_release_tool(binding, args).await
        }
    }

    /// `crew_lease_request`: a member asks for a lease it is allowed (H10, H11). The member is the
    /// caller, never the draft; a write lease must lie inside the brief, off the protected paths, and
    /// a lease overlapping another member's is refused and the lead is told.
    async fn crew_lease_request_tool(
        self: &std::sync::Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let who = self.bus_sender(binding, LEASE_REQUEST_TOOL)?;
        let Some(task_id) = who.task.clone() else {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only a member working a task requests a lease",
            ));
        };
        let fields = lease_draft(LEASE_REQUEST_TOOL, args, &["paths", "mode", "expires_at"])?;
        let paths = draft_paths(fields)?;
        let mode = draft_mode(fields)?;
        let expires_at = draft_expiry(fields)?;
        let now = crate::util::unix_now();
        // H10: expires_at is later than the request itself. Whole seconds compare as text.
        if expires_at.as_str() <= timestamp(now)?.as_str() {
            return Err(CrewError::new(
                ErrorCode::LeaseExpired,
                "expires_at is not later than the request",
            ));
        }
        let task = self
            .store
            .crew_task(&task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no such task"))?;
        let brief = self
            .store
            .crew_brief(&task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no such brief"))?;
        if mode == LeaseMode::Write {
            // H10: a reviewer reads and reports, so it takes no write lease.
            if is_reviewer(&brief) {
                return Err(CrewError::new(
                    ErrorCode::NotAuthorized,
                    "a reviewer is never given a write lease",
                ));
            }
            // H11: a write lease never reaches a path the checks use.
            if let Some(key) = self.store.crew_answer_key(&task_id).map_err(internal)? {
                let asked: Vec<&CrewPathPattern> = paths.iter().collect();
                refuse_protected(&asked, &key.protected_paths)?;
            }
            // H10: a write lease lies inside the brief's write files, never wider.
            let granted: Vec<&CrewPathPattern> = brief
                .files
                .iter()
                .filter(|file| file.mode == LeaseMode::Write)
                .map(|file| &file.path)
                .collect();
            if let Some(path) = paths
                .iter()
                .find(|wanted| !granted.iter().any(|held| covers(held, wanted)))
            {
                return Err(CrewError::new(
                    ErrorCode::NotAuthorized,
                    format!("{path} is outside the files the brief asked for"),
                ));
            }
        }
        let run_id = CrewId::new(task.run_id.clone()).map_err(invalid)?;
        // H10: the live read, the conflict check and the insert are one step. A start and a
        // correction take the same lock for their check and grants, so no two grants can both pass.
        let _granting = self.crew_lease_grants.lock().await;
        // H10: the request can wait here for a grant in flight, so the clock is read again: the
        // grant is stamped and its expiry checked after the wait, never before it, and a request
        // whose expiry passed meanwhile is refused rather than stored already lapsed.
        let now = crate::util::unix_now();
        if expires_at.as_str() <= timestamp(now)?.as_str() {
            return Err(CrewError::new(
                ErrorCode::LeaseExpired,
                "expires_at is not later than the request",
            ));
        }
        let grant = Grant {
            brief: &brief,
            run_id: &run_id,
            holder: &who.member,
            now,
        };
        let lease = grant.lease(paths, mode, LeaseState::Granted, expires_at, None)?;
        // H2: nothing an object holds is a secret, a path included.
        refuse_secret(&CrewObject::Lease(lease.clone()).to_value())?;
        let live = self.live_crew_leases(who.crew_id.as_str(), now)?;
        if let Some((_, held)) = live.iter().find(|(_, held)| lease.conflicts_with(held)) {
            let error = CrewError::new(
                ErrorCode::LeaseConflict,
                format!(
                    "the lease overlaps one held by {}; the lead decides who goes first",
                    held.holder
                ),
            );
            self.tell_lead_of(&who, &error);
            return Err(error);
        }
        // H10: the run can end while the request waits, and the run-end cleanup reads the live
        // leases after the task moved to its final state. The grant is stored only if that state
        // still stands when its own transaction reads it, so a waiting request cannot store a
        // grant the cleanup has already passed.
        self.store
            .add_crew_lease_grant(&task.id, &lease)
            .map_err(|error| match error {
                CoreError::RunClosed => CrewError::new(ErrorCode::RunEnded, "this run has ended"),
                other => internal(other),
            })?;
        Ok(json!({ "id": lease.id, "state": lease.state }))
    }

    /// `crew_lease_release`: a member releases its own leases on exactly `paths`. A path it does not
    /// hold is `lease_not_held`; a release writes a new object, so the object table stays append-only.
    async fn crew_lease_release_tool(
        &self,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let who = self.bus_sender(binding, LEASE_RELEASE_TOOL)?;
        let Some(task_id) = who.task.clone() else {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only a member working a task releases a lease",
            ));
        };
        let fields = lease_draft(LEASE_RELEASE_TOOL, args, &["paths"])?;
        let paths = draft_paths(fields)?;
        let now = crate::util::unix_now();
        let brief = self
            .store
            .crew_brief(&task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no such brief"))?;
        // H10: the live read and the terminal objects it writes are one step with every grant, so a
        // release cannot read past a grant that lands before its write and then hide that grant.
        let _granting = self.crew_lease_grants.lock().await;
        let live = self.live_crew_leases(who.crew_id.as_str(), now)?;
        let mine: Vec<(String, Lease)> = live
            .into_iter()
            .filter(|(owner, lease)| {
                owner == &task_id && lease.holder == who.member && lease.paths == paths
            })
            .collect();
        if mine.is_empty() {
            return Err(CrewError::new(
                ErrorCode::LeaseNotHeld,
                "no such lease is held",
            ));
        }
        for (owner, lease) in mine {
            let grant = Grant {
                brief: &brief,
                run_id: &lease.run_id,
                holder: &lease.holder,
                now,
            };
            let ended = grant.ended(&lease, LeaseState::Released, "The member released it.")?;
            self.store
                .add_crew_object(&owner, &CrewObject::Lease(ended))
                .map_err(internal)?;
        }
        Ok(json!({ "ok": true }))
    }
}
