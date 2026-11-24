//! Crew lessons at rest (migrations 80, 81 and 84, KC5 slices 1 to 3): the short, device-local sentences the crew keeps
//! about one repository or all of them, with where each came from.
//!
//! A lesson is data in a brief, never a rule: nothing here routes, verifies, holds a lease, reads an answer key or
//! changes a check. The table holds ids, enums and one short text; evidence is ids only, never content. It is device
//! local and never synced: the settings sync is an allowlist of preference keys and does not name this table.
//!
//! Every lesson-content write is one immediate transaction in this order (architecture rule 6): the ledger's audit
//! row first, then the row. A fingerprint already remembered or forgotten is never rewritten. At a cap (50 per
//! repository, 20 global) one older lesson becomes `hidden_by_cap` -- the least used first, then the oldest -- and stays
//! readable; nothing is ever auto-deleted for the cap.
//!
//! Slice 2 adds the brief's bookkeeping: a lesson a brief carries grows `uses` and `last_used_at` in the same
//! transaction that stores the brief (`record_lesson_use`), and one its answer-key check leaves out grows
//! `left_out_of`. The counters are bookkeeping, not lesson content: they audit nothing and run inside the caller's
//! transaction, the one that stores the brief. The per-repository "use lessons" switch lives in
//! `project_preferences` under a key no setting reads, as the remembered default branch and the worker-provider
//! choice do; absent is on.
//!
//! Slice 3 adds a person's writes on top of the store: `rev` refuses a write made from a screen that changed, `deleted_at`
//! is the 10 s Undo, and `crew_lessons_forgotten` keeps a purged lesson's fingerprint so it is never written again. A
//! soft-deleted row is hidden from every public read except the undo's own; every public lesson read or write first
//! purges the rows whose undo window ended, each with its own `crew.lesson.purge` audit row in the same transaction.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::{format_unix, new_id, now, parse_time, unix_now};

/// How many lessons one repository keeps visible.
pub const REPO_CAP: u32 = 50;
/// How many global lessons this device keeps visible.
pub const GLOBAL_CAP: u32 = 20;
/// The most evidence ids one lesson keeps, whichever path wrote it; the first ones are kept.
pub const EVIDENCE_MAX: usize = 5;
/// The 10 s Undo a delete keeps its lesson restorable for.
pub const UNDO_SECS: i64 = 10;

/// One id a lesson came from, never its content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonEvidence {
    pub kind: String,
    pub id: String,
}

/// One lesson as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewLesson {
    pub id: String,
    /// `repo` or `global`.
    pub scope: String,
    /// The project id of a repository lesson; none for a global one.
    pub project_id: Option<String>,
    /// `fingerprint`, `escape`, `revert` or `person`.
    pub source: String,
    pub text: String,
    /// The dedupe key, when the lesson has one (a person's lesson by hand has none).
    pub fingerprint: Option<String>,
    pub evidence: Vec<LessonEvidence>,
    /// `active` or `disabled`.
    pub state: String,
    /// Over the cap: readable under the Hidden filter, never deleted for it.
    pub hidden_by_cap: bool,
    pub edited_by_person: bool,
    /// The text names a repository-relative path, so it cannot be promoted to global until edited.
    pub names_path: bool,
    /// Briefs that carried it.
    pub uses: u32,
    /// Briefs it was left out of because their answer-key check failed.
    pub left_out_of: u32,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
    /// This screen's version: a write sends it back and a different one is refused.
    pub rev: u32,
    /// The soft-delete time while the 10 s Undo is open; none while the lesson is live.
    pub deleted_at: Option<String>,
}

/// A lesson to write. The store fills the id and times and decides the cap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCrewLesson {
    pub scope: String,
    pub project_id: Option<String>,
    pub source: String,
    pub text: String,
    pub fingerprint: Option<String>,
    pub evidence: Vec<LessonEvidence>,
    pub edited_by_person: bool,
    pub names_path: bool,
}

fn lesson_row(row: &Row<'_>) -> rusqlite::Result<CrewLesson> {
    let evidence: String = row.get("evidence")?;
    Ok(CrewLesson {
        id: row.get("id")?,
        scope: row.get("scope")?,
        project_id: row.get("project_id")?,
        source: row.get("source")?,
        text: row.get("text")?,
        fingerprint: row.get("fingerprint")?,
        evidence: serde_json::from_str(&evidence).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        state: row.get("state")?,
        hidden_by_cap: row.get::<_, i64>("hidden_by_cap")? != 0,
        edited_by_person: row.get::<_, i64>("edited_by_person")? != 0,
        names_path: row.get::<_, i64>("names_path")? != 0,
        uses: u32::try_from(row.get::<_, i64>("uses")?).unwrap_or(u32::MAX),
        left_out_of: u32::try_from(row.get::<_, i64>("left_out_of")?).unwrap_or(u32::MAX),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        last_used_at: row.get("last_used_at")?,
        rev: u32::try_from(row.get::<_, i64>("rev")?).unwrap_or(u32::MAX),
        deleted_at: row.get("deleted_at")?,
    })
}

/// The visible-room rule: a repository keeps 50 lessons, this device 20 global ones.
fn cap(scope: &str) -> u32 {
    if scope == "repo" {
        REPO_CAP
    } else {
        GLOBAL_CAP
    }
}

/// The refusal a write from an older screen gets: its `rev` is not the stored one.
fn stale() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "stale",
        message: "This lesson changed. Reload it and try again.".to_owned(),
        detail: serde_json::Value::Null,
    }
}

fn lesson_missing() -> CoreError {
    CoreError::NotFound("lesson".into())
}

/// The cutoff a public call purges under: the undo window has ended exactly `UNDO_SECS` before now.
fn purge_before() -> String {
    format_unix(unix_now() - UNDO_SECS)
}

fn read_lesson(
    tx: &Transaction<'_>,
    id: &str,
    including_deleted: bool,
) -> Result<Option<CrewLesson>> {
    let sql = if including_deleted {
        "SELECT * FROM crew_lessons WHERE id = ?1"
    } else {
        "SELECT * FROM crew_lessons WHERE id = ?1 AND deleted_at IS NULL"
    };
    Ok(tx.query_row(sql, [id], lesson_row).optional()?)
}

fn require_live(tx: &Transaction<'_>, id: &str) -> Result<CrewLesson> {
    read_lesson(tx, id, false)?.ok_or_else(lesson_missing)
}

fn check_seen(current: &CrewLesson, seen_rev: u32) -> Result<()> {
    if current.rev != seen_rev {
        return Err(stale());
    }
    Ok(())
}

fn audit(
    tx: &Transaction<'_>,
    actor: &str,
    action: &str,
    target: &str,
    detail: serde_json::Value,
) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: actor.to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

fn audit_person(
    tx: &Transaction<'_>,
    action: &str,
    target: &str,
    detail: serde_json::Value,
) -> Result<()> {
    audit(tx, "user", action, target, detail)
}

fn visible_count(tx: &Transaction<'_>, scope: &str, project: Option<&str>) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT COUNT(*) FROM crew_lessons
         WHERE scope = ?1 AND project_id IS ?2 AND hidden_by_cap = 0 AND deleted_at IS NULL",
        params![scope, project],
        |row| row.get(0),
    )?)
}

impl Store {
    /// Every lesson of one scope, newest first, hidden ones included. A repository lesson takes its project id;
    /// a global one takes none. A soft-deleted lesson is hidden, and an expired Undo is purged first.
    pub fn crew_lessons(&self, scope: &str, project: Option<&str>) -> Result<Vec<CrewLesson>> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let mut statement = tx.prepare(
                "SELECT * FROM crew_lessons
                 WHERE scope = ?1 AND project_id IS ?2 AND deleted_at IS NULL
                 ORDER BY created_at DESC, id DESC",
            )?;
            let rows = statement
                .query_map(params![scope, project], lesson_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// The lessons one brief may carry: active and not hidden by the cap, of one scope, newest first. The start path
    /// reads this, not the full history, so a start never pays for lessons it cannot use; the caps bound it.
    pub fn crew_lessons_for_brief(
        &self,
        scope: &str,
        project: Option<&str>,
    ) -> Result<Vec<CrewLesson>> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let mut statement = tx.prepare(
                "SELECT * FROM crew_lessons
                 WHERE scope = ?1 AND project_id IS ?2 AND state = 'active' AND hidden_by_cap = 0
                   AND deleted_at IS NULL
                 ORDER BY created_at DESC, id DESC",
            )?;
            let rows = statement
                .query_map(params![scope, project], lesson_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// One live lesson by id, whatever its scope.
    pub fn crew_lesson(&self, id: &str) -> Result<Option<CrewLesson>> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            read_lesson(tx, id, false)
        })
    }

    /// One lesson by id even while its 10 s Undo is open. Only the undo reads this; every other read hides it.
    pub fn crew_lesson_including_deleted(&self, id: &str) -> Result<Option<CrewLesson>> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            read_lesson(tx, id, true)
        })
    }

    /// The list's figures for one repository: its own lessons plus global ones, soft-deleted rows left out.
    pub fn crew_lessons_counts(&self, project: &str) -> Result<(u32, u32, u32)> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let (active, disabled, hidden): (i64, i64, i64) = tx.query_row(
                "SELECT
                     COALESCE(SUM(CASE WHEN hidden_by_cap = 0 AND state = 'active' THEN 1 ELSE 0 END), 0),
                     COALESCE(SUM(CASE WHEN hidden_by_cap = 0 AND state = 'disabled' THEN 1 ELSE 0 END), 0),
                     COALESCE(SUM(CASE WHEN hidden_by_cap = 1 THEN 1 ELSE 0 END), 0)
                 FROM crew_lessons
                 WHERE deleted_at IS NULL
                   AND (scope = 'global' OR (scope = 'repo' AND project_id = ?1))",
                [project],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            Ok((
                u32::try_from(active).unwrap_or(u32::MAX),
                u32::try_from(disabled).unwrap_or(u32::MAX),
                u32::try_from(hidden).unwrap_or(u32::MAX),
            ))
        })
    }

    /// Writes one lesson in its own immediate transaction. A fingerprint already remembered or forgotten is not
    /// rewritten (the call answers `None` and writes nothing, no audit row): the dedupe key is what stops the same
    /// failure or the same escape from writing twice. At the scope's cap one older visible lesson is hidden.
    pub fn write_crew_lesson(&self, new: NewCrewLesson) -> Result<Option<CrewLesson>> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            write_lesson(tx, &new)
        })
    }

    /// A person's edit of one lesson: text and path flag, `edited_by_person`, a new `rev` and `updated_at`. Source and
    /// fingerprint stay as they were. The audit row comes first in the same transaction.
    pub fn edit_crew_lesson(
        &self,
        id: &str,
        text: &str,
        names_path: bool,
        seen_rev: u32,
    ) -> Result<CrewLesson> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let current = require_live(tx, id)?;
            check_seen(&current, seen_rev)?;
            audit_person(
                tx,
                "crew.lesson.edit",
                id,
                json!({ "rev": current.rev.saturating_add(1), "names_path": names_path }),
            )?;
            let at = now();
            tx.execute(
                "UPDATE crew_lessons
                 SET text = ?2, names_path = ?3, edited_by_person = 1, rev = rev + 1, updated_at = ?4
                 WHERE id = ?1",
                params![id, text, i64::from(names_path), at],
            )?;
            require_live(tx, id)
        })
    }

    /// Turns one lesson on or off. A new `rev`; the audit row is `crew.lesson.enable` or `.disable`, first.
    pub fn set_crew_lesson_state(
        &self,
        id: &str,
        active: bool,
        seen_rev: u32,
    ) -> Result<CrewLesson> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let current = require_live(tx, id)?;
            check_seen(&current, seen_rev)?;
            let (action, state) = if active {
                ("crew.lesson.enable", "active")
            } else {
                ("crew.lesson.disable", "disabled")
            };
            audit_person(
                tx,
                action,
                id,
                json!({ "state": state, "rev": current.rev.saturating_add(1) }),
            )?;
            let at = now();
            tx.execute(
                "UPDATE crew_lessons SET state = ?2, rev = rev + 1, updated_at = ?3 WHERE id = ?1",
                params![id, state, at],
            )?;
            require_live(tx, id)
        })
    }

    /// Moves a repository lesson to global. It refuses one that names a path; the new global scope may be over its cap,
    /// in which case one older visible global lesson is hidden in this same transaction. The audit row comes first.
    pub fn promote_crew_lesson(&self, id: &str, seen_rev: u32) -> Result<CrewLesson> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let current = require_live(tx, id)?;
            check_seen(&current, seen_rev)?;
            if current.names_path {
                return Err(CoreError::Invalid(
                    "Edit this lesson first: it names a path, and a global lesson names path classes only."
                        .into(),
                ));
            }
            if current.scope != "repo" {
                return Err(CoreError::Invalid(
                    "Only a repository lesson can be promoted.".into(),
                ));
            }
            audit_person(
                tx,
                "crew.lesson.promote",
                id,
                json!({ "from": current.project_id, "rev": current.rev.saturating_add(1) }),
            )?;
            let at = now();
            tx.execute(
                "UPDATE crew_lessons
                 SET scope = 'global', project_id = NULL, rev = rev + 1, updated_at = ?2
                 WHERE id = ?1",
                params![id, at],
            )?;
            if visible_count(tx, "global", None)? > i64::from(GLOBAL_CAP) {
                hide_one(tx, "global", None, id, &at)?;
            }
            require_live(tx, id)
        })
    }

    /// Moves a global lesson back to one repository, from the body. The repository scope may be over its cap, in which
    /// case one older visible repository lesson is hidden in this same transaction. The audit row comes first.
    pub fn demote_crew_lesson(
        &self,
        id: &str,
        project_id: &str,
        seen_rev: u32,
    ) -> Result<CrewLesson> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let current = require_live(tx, id)?;
            check_seen(&current, seen_rev)?;
            if current.scope != "global" {
                return Err(CoreError::Invalid(
                    "Only a global lesson can be demoted.".into(),
                ));
            }
            audit_person(
                tx,
                "crew.lesson.demote",
                id,
                json!({ "to": project_id, "rev": current.rev.saturating_add(1) }),
            )?;
            let at = now();
            tx.execute(
                "UPDATE crew_lessons
                 SET scope = 'repo', project_id = ?2, rev = rev + 1, updated_at = ?3
                 WHERE id = ?1",
                params![id, project_id, at],
            )?;
            if visible_count(tx, "repo", Some(project_id))? > i64::from(REPO_CAP) {
                hide_one(tx, "repo", Some(project_id), id, &at)?;
            }
            require_live(tx, id)
        })
    }

    /// The 10 s Undo's first half: hides one lesson and answers the instant its window started. No `rev` bump, so the
    /// undo restores the same version the person had. The audit row comes first.
    pub fn delete_crew_lesson(&self, id: &str, seen_rev: u32) -> Result<String> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let current = require_live(tx, id)?;
            check_seen(&current, seen_rev)?;
            let at = now();
            audit_person(tx, "crew.lesson.delete", id, json!({ "deleted_at": at }))?;
            tx.execute(
                "UPDATE crew_lessons SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![id, at],
            )?;
            Ok(at)
        })
    }

    /// The 10 s Undo's second half: clears the deletion and bumps `rev` (a person sees the lesson again). After the
    /// window or for an id that is not deleted, nothing exists to undo.
    pub fn undo_delete_crew_lesson(&self, id: &str) -> Result<CrewLesson> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let current = read_lesson(tx, id, true)?.ok_or_else(lesson_missing)?;
            let Some(deleted_at) = current.deleted_at.clone() else {
                return Err(lesson_missing());
            };
            let within: i64 = tx.query_row(
                "SELECT COALESCE(julianday(?1) > julianday(?2), 0)",
                params![deleted_at, purge_before()],
                |row| row.get(0),
            )?;
            if within == 0 {
                return Err(lesson_missing());
            }
            audit_person(
                tx,
                "crew.lesson.undo",
                id,
                json!({ "rev": current.rev.saturating_add(1) }),
            )?;
            let at = now();
            tx.execute(
                "UPDATE crew_lessons SET deleted_at = NULL, rev = rev + 1, updated_at = ?2 WHERE id = ?1",
                params![id, at],
            )?;
            require_live(tx, id)
        })
    }

    /// Purges deleted lessons whose Undo ended before `older_than` (an RFC 3339 cutoff), each with its audit row and
    /// remembered fingerprint in this one transaction. Tests and the deadline pass a cutoff instead of sleeping.
    #[cfg(test)]
    pub(crate) fn purge_deleted_lessons(&self, older_than: &str) -> Result<Vec<String>> {
        self.commit(|tx, _| purge_deleted(tx, older_than))
    }

    /// The other tasks of one project whose run carries this failure fingerprint, created at or after
    /// `since_unix`, oldest first. The loop guard stores the fingerprint per task, never its parts, so the same
    /// fingerprint on two tasks is told by this read alone.
    pub fn crew_tasks_with_fingerprint(
        &self,
        project: &str,
        fingerprint: &str,
        since_unix: i64,
        except_task: &str,
    ) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT t.id, t.created_at FROM crew_runs r
             JOIN crew_tasks t ON t.id = r.task_id
             JOIN threads th ON th.id = r.lead_thread_id
             WHERE r.fingerprint = ?1 AND th.project_id = ?2 AND t.id != ?3
             ORDER BY t.created_at, t.id",
        )?;
        let rows = statement
            .query_map(params![fingerprint, project, except_task], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter(|(_, created_at)| parse_time(created_at).is_some_and(|at| at >= since_unix))
            .map(|(id, _)| id)
            .collect())
    }
}

fn write_lesson(tx: &Transaction<'_>, new: &NewCrewLesson) -> Result<Option<CrewLesson>> {
    if let Some(fingerprint) = &new.fingerprint {
        let remembered: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM crew_lessons WHERE fingerprint = ?1)
                 OR EXISTS(SELECT 1 FROM crew_lessons_forgotten WHERE fingerprint = ?1)",
            [fingerprint],
            |row| row.get(0),
        )?;
        if remembered {
            return Ok(None);
        }
    }
    let visible = visible_count(tx, &new.scope, new.project_id.as_deref())?;
    let id = format!("les_{}", new_id());
    let at = now();
    let kept = &new.evidence[..new.evidence.len().min(EVIDENCE_MAX)];
    let evidence = serde_json::to_value(kept)?;
    let detail = json!({
        "scope": new.scope,
        "project": new.project_id,
        "source": new.source,
        "fingerprint": new.fingerprint,
        "evidence": evidence,
    });
    let actor = if new.edited_by_person { "user" } else { "core" };
    insert_audit(
        tx,
        &AuditRecord {
            actor: actor.to_owned(),
            action: "crew.lesson.learn".to_owned(),
            target: id.clone(),
            decision: None,
            source: actor.to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    tx.execute(
        "INSERT INTO crew_lessons
            (id, scope, project_id, source, text, fingerprint, evidence, state, hidden_by_cap,
             edited_by_person, names_path, uses, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', 0, ?8, ?9, 0, ?10, ?10)",
        params![
            id,
            new.scope,
            new.project_id,
            new.source,
            new.text,
            new.fingerprint,
            serde_json::to_string(kept)?,
            i64::from(new.edited_by_person),
            i64::from(new.names_path),
            at,
        ],
    )?;
    if visible >= i64::from(cap(&new.scope)) {
        hide_one(tx, &new.scope, new.project_id.as_deref(), &id, &at)?;
    }
    read_lesson(tx, &id, true)
}

/// Hides one older visible lesson of the scope: the least used first, then the one used longest ago (never used
/// counts as its creation), then the oldest, then the id. The row just written or moved is never the one hidden. The
/// audit row comes before the update.
fn hide_one(
    tx: &Transaction<'_>,
    scope: &str,
    project: Option<&str>,
    written: &str,
    at: &str,
) -> Result<Option<String>> {
    let victim: Option<String> = tx
        .query_row(
            "SELECT id FROM crew_lessons
             WHERE scope = ?1 AND project_id IS ?2 AND hidden_by_cap = 0 AND deleted_at IS NULL
               AND id != ?3
             ORDER BY uses ASC, COALESCE(last_used_at, created_at) ASC, created_at ASC, id ASC
             LIMIT 1",
            params![scope, project, written],
            |row| row.get(0),
        )
        .optional()?;
    let Some(victim) = victim else {
        return Ok(None);
    };
    audit(
        tx,
        "core",
        "crew.lesson.hide",
        &victim,
        json!({
            "cap": cap(scope),
            "scope": scope,
            "project": project,
            "why": "cap",
        }),
    )?;
    tx.execute(
        "UPDATE crew_lessons
         SET hidden_by_cap = 1, rev = rev + 1, updated_at = ?2
         WHERE id = ?1",
        params![victim, at],
    )?;
    Ok(Some(victim))
}

/// Removes every lesson whose 10 s Undo ended before `older_than`: its fingerprint is remembered first (when it has
/// one), one audit row is written per lesson, then the row is deleted. One transaction for the whole pass.
pub(crate) fn purge_deleted(tx: &Transaction<'_>, older_than: &str) -> Result<Vec<String>> {
    let expired: Vec<(String, Option<String>, String)> = {
        let mut statement = tx.prepare(
            "SELECT id, fingerprint, deleted_at FROM crew_lessons
             WHERE deleted_at IS NOT NULL AND julianday(deleted_at) < julianday(?1)
             ORDER BY deleted_at, id",
        )?;
        statement
            .query_map([older_than], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let at = now();
    let mut purged = Vec::with_capacity(expired.len());
    for (id, fingerprint, deleted_at) in expired {
        audit(
            tx,
            "core",
            "crew.lesson.purge",
            &id,
            json!({ "deleted_at": deleted_at }),
        )?;
        if let Some(fingerprint) = fingerprint {
            tx.execute(
                "INSERT OR IGNORE INTO crew_lessons_forgotten (fingerprint, forgotten_at)
                 VALUES (?1, ?2)",
                params![fingerprint, at],
            )?;
        }
        tx.execute("DELETE FROM crew_lessons WHERE id = ?1", [&id])?;
        purged.push(id);
    }
    Ok(purged)
}

/// Counts one brief's lessons in the caller's transaction, the one that stores the brief: a carried lesson gets a use
/// and its use time, a lesson the brief's key check left out grows `left_out_of`. A lesson deleted between the read
/// and this write is skipped, never an error: the brief still starts.
pub(super) fn record_lesson_use(
    tx: &Transaction<'_>,
    used: &[String],
    left_out: &[String],
    at: &str,
) -> Result<()> {
    // Bookkeeping is never a reason to refuse a start: it runs in a savepoint, and a failure rolls only the counters back
    // while the task the caller is storing goes on.
    tx.execute_batch("SAVEPOINT lesson_use")?;
    let counted = (|| -> Result<()> {
        for id in used {
            tx.execute(
                "UPDATE crew_lessons
                 SET uses = uses + 1, last_used_at = ?2, updated_at = ?2
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![id, at],
            )?;
        }
        for id in left_out {
            tx.execute(
                "UPDATE crew_lessons
                 SET left_out_of = left_out_of + 1, updated_at = ?2
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![id, at],
            )?;
        }
        Ok(())
    })();
    if let Err(error) = counted {
        tracing::warn!(%error, "a brief's lessons could not be counted, so none is");
        tx.execute_batch("ROLLBACK TO lesson_use")?;
    }
    tx.execute_batch("RELEASE lesson_use")?;
    Ok(())
}

/// The project preference that turns a repository's lessons on and off. A key no setting reads, as the remembered
/// default branch and the worker-provider choice are kept.
const USE_LESSONS: &str = "crew_use_lessons";

impl Store {
    /// Whether this repository's briefs carry lessons. Absent is on, as it ships under Preview; an explicit `off`
    /// is off; damaged or unknown text reads off, so a value this build does not understand never injects a lesson.
    pub fn crew_use_lessons(&self, project_id: &str) -> Result<bool> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            let text: Option<String> = tx
                .query_row(
                    "SELECT value FROM project_preferences WHERE project_id = ?1 AND key = ?2",
                    params![project_id, USE_LESSONS],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(match text.as_deref() {
                None | Some("on") => true,
                Some("off") => false,
                Some(other) => {
                    tracing::warn!(value = other, "an unknown use-lessons value reads as off");
                    false
                }
            })
        })
    }

    /// A person's "use lessons" switch for one repository, with its audit row in the same transaction. Saving the
    /// default (on) clears the row.
    pub fn set_crew_use_lessons(&self, project_id: &str, on: bool, actor: &str) -> Result<()> {
        self.commit(|tx, _| {
            purge_deleted(tx, &purge_before())?;
            insert_audit(
                tx,
                &AuditRecord {
                    actor: actor.to_owned(),
                    action: "crew.lesson.settings".to_owned(),
                    target: project_id.to_owned(),
                    decision: None,
                    source: "user".to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({ "repo": project_id, "use_lessons": on }),
                },
            )?;
            if on {
                tx.execute(
                    "DELETE FROM project_preferences WHERE project_id = ?1 AND key = ?2",
                    params![project_id, USE_LESSONS],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO project_preferences (project_id, key, value) VALUES (?1, ?2, 'off')
                     ON CONFLICT (project_id, key) DO UPDATE SET value = 'off'",
                    params![project_id, USE_LESSONS],
                )?;
            }
            Ok(())
        })
    }
}
