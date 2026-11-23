//! Crew lessons at rest (migration 80, KC5 slice 1): the short, device-local sentences the crew keeps about one
//! repository or all of them, with where each came from.
//!
//! A lesson is data in a brief, never a rule: nothing here routes, verifies, holds a lease, reads an answer key or
//! changes a check. The table holds ids, enums and one short text; evidence is ids only, never content. It is device
//! local and never synced: the settings sync is an allowlist of preference keys and does not name this table.
//!
//! Every write is one immediate transaction in this order (architecture rule 6): the ledger's audit row first, then
//! the row. A fingerprint already remembered is never rewritten. At a cap (50 per repository, 20 global) one older
//! lesson becomes `hidden_by_cap` -- the least used first, then the oldest -- and stays readable; nothing is ever
//! auto-deleted.
//!
//! Slice 2 adds the brief's bookkeeping: a lesson a brief carries grows `uses` and `last_used_at` in the same
//! transaction that stores the brief (`record_lesson_use`), and one its answer-key check leaves out grows
//! `left_out_of`. The per-repository "use lessons" switch lives in `project_preferences` under a key no setting
//! reads, as the remembered default branch and the worker-provider choice do; absent is on.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AuditRecord, Store, insert_audit};
use crate::error::Result;
use crate::util::{new_id, now, parse_time};

/// How many lessons one repository keeps visible.
pub const REPO_CAP: u32 = 50;
/// How many global lessons this device keeps visible.
pub const GLOBAL_CAP: u32 = 20;

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

impl Store {
    /// Every lesson of one scope, newest first, hidden ones included. A repository lesson takes its project id;
    /// a global one takes none.
    pub fn crew_lessons(&self, scope: &str, project: Option<&str>) -> Result<Vec<CrewLesson>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM crew_lessons WHERE scope = ?1 AND project_id IS ?2
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement
            .query_map(params![scope, project], lesson_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// One lesson by id, whatever its scope.
    pub fn crew_lesson(&self, id: &str) -> Result<Option<CrewLesson>> {
        let conn = self.reader();
        Ok(conn
            .query_row("SELECT * FROM crew_lessons WHERE id = ?1", [id], lesson_row)
            .optional()?)
    }

    /// Writes one lesson in its own immediate transaction. A fingerprint already remembered is not rewritten
    /// (the call answers `None` and writes nothing, no audit row): the dedupe key is what stops the same failure
    /// or the same escape from writing twice. At the scope's cap one older visible lesson is hidden.
    pub fn write_crew_lesson(&self, new: NewCrewLesson) -> Result<Option<CrewLesson>> {
        self.commit(|tx, _| write_lesson(tx, &new))
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
            "SELECT EXISTS(SELECT 1 FROM crew_lessons WHERE fingerprint = ?1)",
            [fingerprint],
            |row| row.get(0),
        )?;
        if remembered {
            return Ok(None);
        }
    }
    let visible: i64 = tx.query_row(
        "SELECT COUNT(*) FROM crew_lessons
         WHERE scope = ?1 AND project_id IS ?2 AND hidden_by_cap = 0",
        params![new.scope, new.project_id],
        |row| row.get(0),
    )?;
    let id = format!("les_{}", new_id());
    let at = now();
    let evidence = serde_json::to_value(&new.evidence)?;
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
            serde_json::to_string(&new.evidence)?,
            i64::from(new.edited_by_person),
            i64::from(new.names_path),
            at,
        ],
    )?;
    if visible >= i64::from(cap(&new.scope)) {
        hide_one(tx, new, &id, &at)?;
    }
    Ok(tx
        .query_row(
            "SELECT * FROM crew_lessons WHERE id = ?1",
            [&id],
            lesson_row,
        )
        .optional()?)
}

/// Hides one older visible lesson of the scope: the least used first, then the one used longest ago (never used
/// counts as its creation), then the oldest, then the id. The new row is never the one hidden. The audit row comes
/// before the update.
fn hide_one(tx: &Transaction<'_>, new: &NewCrewLesson, written: &str, at: &str) -> Result<()> {
    let victim: Option<String> = tx
        .query_row(
            "SELECT id FROM crew_lessons
             WHERE scope = ?1 AND project_id IS ?2 AND hidden_by_cap = 0 AND id != ?3
             ORDER BY uses ASC, COALESCE(last_used_at, created_at) ASC, created_at ASC, id ASC
             LIMIT 1",
            params![new.scope, new.project_id, written],
            |row| row.get(0),
        )
        .optional()?;
    let Some(victim) = victim else {
        return Ok(());
    };
    insert_audit(
        tx,
        &AuditRecord {
            actor: "core".to_owned(),
            action: "crew.lesson.hide".to_owned(),
            target: victim.clone(),
            decision: None,
            source: "core".to_owned(),
            thread_id: None,
            run_id: None,
            detail: json!({
                "cap": cap(&new.scope),
                "scope": new.scope,
                "project": new.project_id,
                "why": "cap",
            }),
        },
    )?;
    tx.execute(
        "UPDATE crew_lessons SET hidden_by_cap = 1, updated_at = ?2 WHERE id = ?1",
        params![victim, at],
    )?;
    Ok(())
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
    for id in used {
        tx.execute(
            "UPDATE crew_lessons SET uses = uses + 1, last_used_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
    }
    for id in left_out {
        tx.execute(
            "UPDATE crew_lessons SET left_out_of = left_out_of + 1, updated_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
    }
    Ok(())
}

/// The project preference that turns a repository's lessons on and off. A key no setting reads, as the remembered
/// default branch and the worker-provider choice are kept.
const USE_LESSONS: &str = "crew_use_lessons";

impl Store {
    /// Whether this repository's briefs carry lessons. Absent is on, as it ships under Preview; an explicit `off`
    /// is off; damaged or unknown text reads off, so a value this build does not understand never injects a lesson.
    pub fn crew_use_lessons(&self, project_id: &str) -> Result<bool> {
        let text: Option<String> = self
            .reader()
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
    }

    /// A person's "use lessons" switch for one repository, with its audit row in the same transaction. Saving the
    /// default (on) clears the row.
    pub fn set_crew_use_lessons(&self, project_id: &str, on: bool, actor: &str) -> Result<()> {
        self.commit(|tx, _| {
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
