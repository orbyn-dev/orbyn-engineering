//! Durable intent before a forge effect, with optimistic revision checks.

use orbyn_contracts::AgUiEvent;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::is_commit;

pub(super) const SCHEMA: &str = r#"
CREATE TABLE crew_forge_merges (
    task_id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision >= 0),
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE TABLE crew_merge_work (
    identity TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    epoch TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('claimed', 'delivered'))
);
"#;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForgePhase {
    Prepared,
    Publishing,
    AwaitingCi,
    MergeRequested,
    Merged,
    NeedsPerson,
    /// A person dropped the intent. Terminal: the entry left the lane and nothing was sent to the forge.
    Rejected,
}

/// What a person (or the core, reading a merge a person made on the forge) does with a `needs_person` intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeResolution {
    /// Drop it. The entry becomes `rejected`; no forge effect is made.
    Reject,
    /// Back to the phase the pipeline re-evaluates from: `awaiting_ci` when the PR exists, `prepared` when nothing was published.
    Retry,
    /// The forge already has the merge (a person merged the PR by hand). The merge record follows from the receipt.
    Merged { merge_commit: String },
    /// A rejected intent whose merge request may have been sent: the forge shows it closed unmerged, so nothing is left to wait for.
    Settle,
}

/// The only way out of `needs_person`, and out of a `rejected` intent whose merge request may still land. Deliberately not part of
/// [`Store::save_crew_forge_merge`]'s table: the pipeline can enter `needs_person` but never leave it on its own, so a stuck intent
/// changes only by a person's audited answer or a forge receipt.
fn resolves(from: ForgePhase, to: ForgePhase) -> bool {
    match from {
        ForgePhase::NeedsPerson => matches!(
            to,
            ForgePhase::Prepared
                | ForgePhase::AwaitingCi
                | ForgePhase::Merged
                | ForgePhase::Rejected
        ),
        ForgePhase::Rejected => matches!(to, ForgePhase::Merged | ForgePhase::Rejected),
        _ => false,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ForgeMerge {
    pub task_id: String,
    pub repo: String,
    pub remote: String,
    pub branch: String,
    pub source: String,
    pub candidate: String,
    pub base: String,
    pub combined: Option<String>,
    pub pull: Option<u64>,
    pub phase: ForgePhase,
    pub merge_commit: Option<String>,
    pub evidence: Vec<String>,
    pub note: Option<String>,
    /// A merge request may have reached the forge (saved just before it is sent, never cleared by the pipeline). A request can
    /// outlive the call (a merge queue, auto-merge), so such an intent can never be retried, and a reject stays reconcilable.
    #[serde(default)]
    pub merge_sent: bool,
    pub revision: i64,
}

impl ForgeMerge {
    fn validate(&self) -> Result<()> {
        if self.task_id.is_empty()
            || self.repo.is_empty()
            || self.remote.is_empty()
            || self.source.is_empty()
            || self.branch.is_empty()
            || self.source == self.branch
            || !is_commit(&self.candidate)
            || !is_commit(&self.base)
            || self.combined.as_deref().is_some_and(|s| !is_commit(s))
            || self.merge_commit.as_deref().is_some_and(|s| !is_commit(s))
            || self.pull == Some(0)
            || self.revision < 0
            || (matches!(
                self.phase,
                ForgePhase::AwaitingCi | ForgePhase::MergeRequested | ForgePhase::Merged
            ) && self.pull.is_none())
            || (self.phase == ForgePhase::Merged && self.merge_commit.is_none())
            || (matches!(
                self.phase,
                ForgePhase::Publishing
                    | ForgePhase::AwaitingCi
                    | ForgePhase::MergeRequested
                    | ForgePhase::Merged
            ) && (self.combined.is_none() || self.evidence.is_empty()))
            || self
                .evidence
                .iter()
                .any(|id| orbyn_contracts::CrewHash::new(id.clone()).is_err())
        {
            return Err(CoreError::Invalid("invalid Crew forge intent".into()));
        }
        Ok(())
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.task_id == other.task_id
            && self.repo == other.repo
            && self.remote == other.remote
            && self.branch == other.branch
            && self.source == other.source
            && self.candidate == other.candidate
            && self.base == other.base
    }
}

impl Store {
    /// Unfinished remote effects and confirmed receipts not yet projected into the Review queue.
    /// The bound refuses excess work rather than silently leaving records out of recovery.
    pub fn crew_forge_unfinished(&self) -> Result<Vec<ForgeMerge>> {
        let conn = self.reader();
        let mut statement = conn.prepare("SELECT body FROM crew_forge_merges AS intent WHERE json_extract(body,'$.phase') IN ('publishing','merge_requested') OR (json_extract(body,'$.phase')='rejected' AND json_extract(body,'$.merge_sent')=1) OR (json_extract(body,'$.phase')='merged' AND NOT EXISTS(SELECT 1 FROM crew_merges AS merged WHERE merged.task_id=intent.task_id)) ORDER BY rowid LIMIT 129")?;
        let bodies = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if bodies.len() > 128 {
            return Err(CoreError::Invalid(
                "more than 128 forge intents need reconciliation; a person must reduce the queue"
                    .into(),
            ));
        }
        bodies
            .into_iter()
            .map(|body| {
                let intent: ForgeMerge = serde_json::from_str(&body)?;
                intent.validate()?;
                Ok(intent)
            })
            .collect()
    }

    pub fn crew_forge_merge(&self, task_id: &str) -> Result<Option<ForgeMerge>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_forge_merges WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|body| {
            let intent: ForgeMerge = serde_json::from_str(&body)?;
            intent.validate()?;
            Ok(intent)
        })
        .transpose()
    }

    /// Creates only once; a retry can neither replace the pinned identity nor reset an uncertain effect.
    pub fn add_crew_forge_merge(&self, intent: &ForgeMerge) -> Result<()> {
        intent.validate()?;
        if intent.phase != ForgePhase::Prepared || intent.revision != 0 {
            return Err(CoreError::Invalid("a forge intent starts prepared".into()));
        }
        let body = serde_json::to_string(intent)?;
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_forge_merges(task_id,revision,body) VALUES (?1,0,?2)",
                params![intent.task_id, body],
            )?;
            Ok(())
        })
    }

    pub fn save_crew_forge_merge(&self, intent: &mut ForgeMerge) -> Result<()> {
        intent.validate()?;
        let expected = intent.revision;
        let revision = expected
            .checked_add(1)
            .ok_or_else(|| CoreError::Invalid("forge revision exhausted".into()))?;
        let mut next = intent.clone();
        next.revision = revision;
        let next = self.commit(|tx, _| {
            let old: String = tx.query_row(
                "SELECT body FROM crew_forge_merges WHERE task_id=?1 AND revision=?2",
                params![intent.task_id, expected],
                |r| r.get(0),
            )?;
            let old: ForgeMerge = serde_json::from_str(&old)?;
            let mut next = next.clone();
            // The pipeline can set the flag but never clear it.
            next.merge_sent |= old.merge_sent;
            let body = serde_json::to_string(&next)?;
            let transition = old.phase == next.phase
                || matches!(
                    (old.phase, next.phase),
                    (
                        ForgePhase::Prepared,
                        ForgePhase::Publishing | ForgePhase::NeedsPerson
                    ) | (
                        ForgePhase::Publishing,
                        ForgePhase::AwaitingCi | ForgePhase::Merged | ForgePhase::NeedsPerson
                    ) | (
                        ForgePhase::AwaitingCi,
                        ForgePhase::MergeRequested | ForgePhase::NeedsPerson
                    ) | (
                        ForgePhase::MergeRequested,
                        ForgePhase::Merged | ForgePhase::NeedsPerson
                    )
                );
            if !transition {
                return Err(CoreError::Invalid("invalid forge intent transition".into()));
            }
            if !old.same_identity(&next)
                || old.pull.is_some_and(|p| Some(p) != next.pull)
                || old
                    .merge_commit
                    .is_some_and(|m| Some(m) != next.merge_commit)
            {
                return Err(CoreError::Invalid(
                    "a forge intent keeps its identity".into(),
                ));
            }
            let changed = tx.execute(
                "UPDATE crew_forge_merges SET revision=?3,body=?4 WHERE task_id=?1 AND revision=?2",
                params![intent.task_id, expected, revision, body],
            )?;
            if changed != 1 {
                return Err(CoreError::Invalid("forge intent changed".into()));
            }
            Ok(next)
        })?;
        *intent = next;
        Ok(())
    }

    /// Leaves `needs_person` (or settles a rejected intent whose merge request may still land), audit row first, in one transaction
    /// with the intent and its queue entry, so a crash leaves either the old state (still resolvable) or the whole new one.
    /// `seen_revision` is the intent as the caller saw it; a different revision or phase is `stale`. `actor` is `user` for a
    /// person's answer and `core` for a merge read from the forge; `via` says which path drove it, and goes in the audit row.
    pub fn resolve_crew_forge_merge(
        &self,
        task_id: &str,
        seen_revision: i64,
        resolution: &ForgeResolution,
        actor: &str,
        via: &str,
    ) -> Result<ForgeMerge> {
        self.commit(|tx, _| {
            let found: Option<(i64, String)> = tx
                .query_row(
                    "SELECT revision, body FROM crew_forge_merges WHERE task_id=?1",
                    [task_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (revision, body) =
                found.ok_or_else(|| CoreError::NotFound(format!("forge intent {task_id}")))?;
            let old: ForgeMerge = serde_json::from_str(&body)?;
            let waiting = old.phase == ForgePhase::NeedsPerson;
            let pending = old.phase == ForgePhase::Rejected && old.merge_sent;
            let stale = || {
                CoreError::Stale("this merge changed since you saw it; look again".into())
            };
            if revision != seen_revision || !(waiting || pending) {
                return Err(stale());
            }
            let mut next = old.clone();
            let (entry_state, entry_note): (Option<&str>, Option<&str>) = match resolution {
                ForgeResolution::Reject if waiting => {
                    next.phase = ForgePhase::Rejected;
                    next.note = Some(
                        if old.pull.is_some() {
                            "rejected by a person; its PR is left as it is on the forge"
                        } else if old.combined.is_some() {
                            "rejected by a person; the candidate branch may have been pushed to the forge"
                        } else {
                            "rejected by a person; nothing was sent to the forge"
                        }
                        .into(),
                    );
                    (Some("rejected"), Some("rejected by a person"))
                }
                ForgeResolution::Retry if waiting => {
                    if old.merge_sent {
                        return Err(CoreError::Invalid(
                            "a merge request may already be on the forge, so it cannot be sent again; reject it, or finish or close the PR on the forge".into(),
                        ));
                    }
                    next.phase = if old.pull.is_some() {
                        ForgePhase::AwaitingCi
                    } else if old.combined.is_none() {
                        ForgePhase::Prepared
                    } else {
                        return Err(CoreError::Invalid(
                            "publication may have happened without a PR number; reject it, or merge or close the PR on the forge".into(),
                        ));
                    };
                    next.note = None;
                    (Some("queued"), None)
                }
                ForgeResolution::Merged { merge_commit } => {
                    next.phase = ForgePhase::Merged;
                    next.merge_commit = Some(merge_commit.clone());
                    (None, None)
                }
                ForgeResolution::Settle if pending => {
                    next.merge_sent = false;
                    (None, None)
                }
                _ => return Err(stale()),
            };
            if next.phase != old.phase && !resolves(old.phase, next.phase) {
                return Err(CoreError::Invalid("invalid forge intent transition".into()));
            }
            next.validate()?;
            if !old.same_identity(&next) || old.pull.is_some_and(|p| Some(p) != next.pull) {
                return Err(CoreError::Invalid("a forge intent keeps its identity".into()));
            }
            if waiting {
                let queued: Option<String> = tx
                    .query_row(
                        "SELECT state FROM crew_merge_queue WHERE task_id=?1",
                        [task_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let queued =
                    queued.ok_or_else(|| CoreError::NotFound(format!("queued merge {task_id}")))?;
                if queued == "merged" || queued == "rejected" {
                    return Err(CoreError::Invalid("this merge already ended".into()));
                }
            }
            next.revision = revision
                .checked_add(1)
                .ok_or_else(|| CoreError::Invalid("forge revision exhausted".into()))?;
            // Audit first: no state change without its row, and the row rolls back with a failed change.
            insert_audit(
                tx,
                &AuditRecord {
                    actor: actor.to_owned(),
                    action: "crew.merge.forge.resolve".to_owned(),
                    target: task_id.to_owned(),
                    decision: None,
                    source: if actor == "user" { "user" } else { "core" }.to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: serde_json::json!({
                        "from": old.phase,
                        "to": next.phase,
                        "settled": matches!(resolution, ForgeResolution::Settle),
                        "via": via,
                        "revision": revision,
                        "pull": old.pull,
                        "merge_sent": old.merge_sent,
                        "reason": old.note,
                    }),
                },
            )?;
            let changed = tx.execute(
                "UPDATE crew_forge_merges SET revision=?3,body=?4 WHERE task_id=?1 AND revision=?2",
                params![task_id, revision, next.revision, serde_json::to_string(&next)?],
            )?;
            if changed != 1 {
                return Err(CoreError::Invalid("forge intent changed".into()));
            }
            if let Some(state) = entry_state {
                tx.execute(
                    "UPDATE crew_merge_queue SET state=?2, note=?3, updated_unix=?4 WHERE task_id=?1 AND state NOT IN ('merged','rejected')",
                    params![task_id, state, entry_note, crate::util::unix_now()],
                )?;
            }
            Ok(next)
        })
    }

    /// The queue's own reject (`crew_reject_merge`) turned an entry down without going through the intent. Writes the same audit
    /// row, naming that path, and leaves the intent as it is: an intent that was `merge_requested` stays in the reconcile set.
    pub fn note_crew_forge_entry_rejected(&self, task_id: &str, why: &str) -> Result<()> {
        self.commit(|tx, _| {
            let body: Option<String> = tx
                .query_row(
                    "SELECT body FROM crew_forge_merges WHERE task_id=?1",
                    [task_id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(body) = body else { return Ok(()) };
            let intent: ForgeMerge = serde_json::from_str(&body)?;
            insert_audit(
                tx,
                &AuditRecord {
                    actor: "core".to_owned(),
                    action: "crew.merge.forge.resolve".to_owned(),
                    target: task_id.to_owned(),
                    decision: None,
                    source: "core".to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: serde_json::json!({
                        "from": intent.phase,
                        "to": intent.phase,
                        "via": "queue reject: the entry was turned down and the intent was left as it was",
                        "revision": intent.revision,
                        "pull": intent.pull,
                        "merge_sent": intent.merge_sent,
                        "reason": why,
                    }),
                },
            )?;
            Ok(())
        })
    }

    /// A completed identity is never dispatched twice. A crash permits only read-only watch reconciliation.
    pub fn claim_crew_merge_watch(&self, identity: &str, task_id: &str) -> Result<bool> {
        self.commit(|tx, _| {
            let n = tx.execute("INSERT INTO crew_merge_work(identity,task_id,epoch,state) VALUES(?1,?2,?3,'claimed') ON CONFLICT(identity) DO UPDATE SET epoch=?3 WHERE state='claimed' AND epoch != ?3 AND task_id=?2", params![identity, task_id, self.epoch])?;
            Ok(n == 1)
        })
    }

    pub fn finish_crew_merge_watch(&self, identity: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("UPDATE crew_merge_work SET state='delivered' WHERE identity=?1 AND epoch=?2 AND state='claimed'", params![identity, self.epoch])?;
            Ok(())
        })
    }

    /// The visible thread message and its dedupe receipt commit together. No provider turn is started.
    pub fn crew_merge_notice(
        &self,
        identity: &str,
        task_id: &str,
        thread_id: &str,
        text: &str,
    ) -> Result<bool> {
        self.commit(|tx, appended| {
            let n = tx.execute("INSERT OR IGNORE INTO crew_merge_work(identity,task_id,epoch,state) VALUES(?1,?2,?3,'delivered')", params![identity, task_id, self.epoch])?;
            if n == 0 { return Ok(false); }
            let message_id = crate::util::new_id();
            for event in [
                AgUiEvent::TextMessageStart { message_id: message_id.clone(), role: "assistant".into() },
                AgUiEvent::TextMessageContent { message_id: message_id.clone(), delta: text.into() },
                AgUiEvent::TextMessageEnd { message_id },
            ] {
                super::insert_event(tx, appended, thread_id, None, &event)?;
            }
            Ok(true)
        })
    }

    pub fn release_crew_merge_watch(&self, identity: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM crew_merge_work WHERE identity=?1 AND epoch=?2 AND state='claimed'",
                params![identity, self.epoch],
            )?;
            Ok(())
        })
    }
}
