//! Crew skill proposals at rest (migration 85, KC9 part 1): the fixed-template skill a person may approve when three or
//! more active lessons of one repository share a group key.
//!
//! A lesson and a proposal are data, never a rule: nothing here routes, verifies, holds a lease, reads an answer key or
//! changes a check, and a proposal blocks nothing. It is device-local and never synced: the settings sync is an allowlist
//! of preference keys and does not name this table. The text is made by the core from the source lessons' own words; no
//! model is called and nothing leaves the device.
//!
//! A group key is one repository's source and the two grouping columns: the failing check kinds of a fingerprint lesson
//! and the path area of a fingerprint or escape lesson. Person lessons have neither and never group. There is one proposal
//! per (project, group key) ever: a dismissed or approved row stays, so the same pattern is never proposed again, even
//! after its source lessons are purged. Every write is one immediate transaction with its audit row first (architecture
//! rule 6); `rev` refuses a write made from a screen that changed.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde_json::json;

use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::{new_id, now};

/// The longest proposal text, in characters.
pub const PROPOSAL_TEXT_MAX: usize = 600;
/// The group key's field separator: the unit separator, which repository names, source words and path areas do not hold.
const GROUP_SEP: char = '\u{1f}';

/// One skill proposal as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewSkillProposal {
    pub id: String,
    pub project_id: String,
    /// The source and grouping columns of the lessons it came from.
    pub group_key: String,
    /// `open`, `approved` or `dismissed`.
    pub state: String,
    pub text: String,
    /// The source lessons, ids only.
    pub lessons: Vec<String>,
    /// The skill the approval wrote, once approved.
    pub skill_name: Option<String>,
    pub rev: u32,
    pub created_at: String,
    pub updated_at: String,
    pub approved_at: Option<String>,
}

/// A proposal to write. The store fills the id, the state and the times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCrewSkillProposal {
    pub project_id: String,
    pub group_key: String,
    pub text: String,
    pub lessons: Vec<String>,
}

/// One group of three or more active lessons that has no proposal yet, newest lesson first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyLessonGroup {
    pub group_key: String,
    pub source: String,
    pub check_kind: Option<String>,
    pub path_class: Option<String>,
    pub lessons: Vec<super::CrewLesson>,
}

/// The group key of one lesson: its source and its two grouping columns, missing columns as empty.
pub fn proposal_group_key(
    source: &str,
    check_kind: Option<&str>,
    path_class: Option<&str>,
) -> String {
    format!(
        "{source}{GROUP_SEP}{}{GROUP_SEP}{}",
        check_kind.unwrap_or_default(),
        path_class.unwrap_or_default()
    )
}

fn proposal_row(row: &Row<'_>) -> rusqlite::Result<CrewSkillProposal> {
    let lessons: String = row.get("lessons")?;
    let lessons: Vec<String> = serde_json::from_str(&lessons).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(CrewSkillProposal {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        group_key: row.get("group_key")?,
        state: row.get("state")?,
        text: row.get("text")?,
        lessons,
        skill_name: row.get("skill_name")?,
        rev: u32::try_from(row.get::<_, i64>("rev")?).unwrap_or(u32::MAX),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        approved_at: row.get("approved_at")?,
    })
}

fn read_proposal(tx: &Transaction<'_>, id: &str) -> Result<Option<CrewSkillProposal>> {
    Ok(tx
        .query_row(
            "SELECT * FROM crew_skill_proposals WHERE id = ?1",
            [id],
            proposal_row,
        )
        .optional()?)
}

fn require_proposal(tx: &Transaction<'_>, id: &str) -> Result<CrewSkillProposal> {
    read_proposal(tx, id)?.ok_or_else(|| CoreError::NotFound("skill proposal".into()))
}

fn stale() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "stale",
        message: "This proposal changed. Reload it and try again.".to_owned(),
        detail: serde_json::Value::Null,
    }
}

fn not_open() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "not_open",
        message: "This proposal is not open. Reload it and try again.".to_owned(),
        detail: serde_json::Value::Null,
    }
}

fn check_seen(current: &CrewSkillProposal, seen_rev: u32) -> Result<()> {
    if current.rev != seen_rev {
        return Err(stale());
    }
    Ok(())
}

fn check_open(current: &CrewSkillProposal) -> Result<()> {
    if current.state != "open" {
        return Err(not_open());
    }
    Ok(())
}

fn audit(
    tx: &Transaction<'_>,
    action: &str,
    target: &str,
    detail: serde_json::Value,
) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "core".to_owned(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "core".to_owned(),
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
    insert_audit(
        tx,
        &AuditRecord {
            actor: "user".to_owned(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "user".to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

impl Store {
    /// Every proposal of one repository, newest first, whatever its state.
    pub fn skill_proposals(&self, project_id: &str) -> Result<Vec<CrewSkillProposal>> {
        self.commit(|tx, _| {
            let mut statement = tx.prepare(
                "SELECT * FROM crew_skill_proposals
                 WHERE project_id = ?1
                 ORDER BY created_at DESC, id DESC",
            )?;
            let rows = statement
                .query_map([project_id], proposal_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// One proposal by id, whatever its state.
    pub fn skill_proposal(&self, id: &str) -> Result<Option<CrewSkillProposal>> {
        self.commit(|tx, _| read_proposal(tx, id))
    }

    /// The groups of three or more active lessons of one repository that have no proposal yet, one query, newest lesson
    /// first. A disabled, hidden, soft-deleted or person lesson never counts.
    pub fn crew_group_ready(&self, project_id: &str) -> Result<Vec<ReadyLessonGroup>> {
        self.commit(|tx, _| {
            let mut statement = tx.prepare(
                "SELECT l.* FROM crew_lessons AS l
                 WHERE l.scope = 'repo' AND l.project_id = ?1
                   AND l.state = 'active' AND l.hidden_by_cap = 0 AND l.deleted_at IS NULL
                   AND l.source != 'person'
                   AND (l.check_kind IS NOT NULL OR l.path_class IS NOT NULL)
                   AND (
                       SELECT COUNT(*) FROM crew_lessons AS g
                       WHERE g.scope = 'repo' AND g.project_id = l.project_id
                         AND g.source = l.source
                         AND g.check_kind IS l.check_kind
                         AND g.path_class IS l.path_class
                         AND g.state = 'active' AND g.hidden_by_cap = 0 AND g.deleted_at IS NULL
                   ) >= 3
                   AND NOT EXISTS (
                       SELECT 1 FROM crew_skill_proposals AS p
                       WHERE p.project_id = l.project_id
                         AND p.group_key = l.source || char(31)
                             || COALESCE(l.check_kind, '') || char(31)
                             || COALESCE(l.path_class, '')
                   )
                 ORDER BY l.created_at DESC, l.id DESC",
            )?;
            let lessons = statement
                .query_map([project_id], super::crew_lessons::lesson_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut groups: Vec<ReadyLessonGroup> = Vec::new();
            for lesson in lessons {
                let group_key = proposal_group_key(
                    &lesson.source,
                    lesson.check_kind.as_deref(),
                    lesson.path_class.as_deref(),
                );
                if let Some(group) = groups.iter_mut().find(|group| group.group_key == group_key) {
                    group.lessons.push(lesson);
                    continue;
                }
                groups.push(ReadyLessonGroup {
                    group_key,
                    source: lesson.source.clone(),
                    check_kind: lesson.check_kind.clone(),
                    path_class: lesson.path_class.clone(),
                    lessons: vec![lesson],
                });
            }
            Ok(groups)
        })
    }

    /// Writes one proposal in its own immediate transaction, with its audit row first. The unique (project, group key)
    /// makes it idempotent: an existing proposal is answered `None` and nothing is written.
    pub fn create_skill_proposal(
        &self,
        new: NewCrewSkillProposal,
    ) -> Result<Option<CrewSkillProposal>> {
        self.commit(|tx, _| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM crew_skill_proposals
                     WHERE project_id = ?1 AND group_key = ?2
                 )",
                params![new.project_id, new.group_key],
                |row| row.get(0),
            )?;
            if exists {
                return Ok(None);
            }
            let id = format!("prop_{}", new_id());
            audit(
                tx,
                "crew.proposal.create",
                &id,
                json!({
                    "project": new.project_id,
                    "group": new.group_key,
                    "lessons": new.lessons,
                }),
            )?;
            let at = now();
            tx.execute(
                "INSERT INTO crew_skill_proposals
                    (id, project_id, group_key, state, text, lessons, rev, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'open', ?4, ?5, 1, ?6, ?6)",
                params![
                    id,
                    new.project_id,
                    new.group_key,
                    new.text,
                    serde_json::to_string(&new.lessons)?,
                    at,
                ],
            )?;
            read_proposal(tx, &id)
        })
    }

    /// Approves one open proposal from the `rev` its screen saw: the text the person approved, the skill it wrote, the
    /// new revision and `approved_at`. The audit row comes first in the same transaction.
    pub fn approve_skill_proposal(
        &self,
        id: &str,
        seen_rev: u32,
        text: &str,
        skill_name: &str,
    ) -> Result<CrewSkillProposal> {
        self.commit(|tx, _| {
            let current = require_proposal(tx, id)?;
            check_open(&current)?;
            check_seen(&current, seen_rev)?;
            let rev = current.rev.saturating_add(1);
            audit_person(
                tx,
                "crew.proposal.approve",
                id,
                json!({ "rev": rev, "skill": skill_name, "lessons": current.lessons }),
            )?;
            let at = now();
            tx.execute(
                "UPDATE crew_skill_proposals
                 SET state = 'approved', text = ?2, skill_name = ?3, approved_at = ?4,
                     rev = rev + 1, updated_at = ?4
                 WHERE id = ?1",
                params![id, text, skill_name, at],
            )?;
            require_proposal(tx, id)
        })
    }

    /// Dismisses one open proposal from the `rev` its screen saw; the row is kept so the pattern is not proposed again.
    /// The audit row comes first in the same transaction.
    pub fn dismiss_skill_proposal(&self, id: &str, seen_rev: u32) -> Result<CrewSkillProposal> {
        self.commit(|tx, _| {
            let current = require_proposal(tx, id)?;
            check_open(&current)?;
            check_seen(&current, seen_rev)?;
            let rev = current.rev.saturating_add(1);
            audit_person(tx, "crew.proposal.dismiss", id, json!({ "rev": rev }))?;
            let at = now();
            tx.execute(
                "UPDATE crew_skill_proposals
                 SET state = 'dismissed', rev = rev + 1, updated_at = ?2
                 WHERE id = ?1",
                params![id, at],
            )?;
            require_proposal(tx, id)
        })
    }

    /// Whether every source lesson is still active, visible and not soft-deleted. A lesson purged after its undo window
    /// is gone, so this answers false too. An empty list is never approvable.
    pub fn proposal_sources_ready(&self, lessons: &[String]) -> Result<bool> {
        if lessons.is_empty() {
            return Ok(false);
        }
        let conn = self.reader();
        let ids = serde_json::to_string(lessons)?;
        let ready: i64 = conn.query_row(
            "SELECT COUNT(*) FROM crew_lessons
             WHERE id IN (SELECT value FROM json_each(?1))
               AND state = 'active' AND hidden_by_cap = 0 AND deleted_at IS NULL",
            [ids],
            |row| row.get(0),
        )?;
        Ok(usize::try_from(ready).unwrap_or(usize::MAX) == lessons.len())
    }
}
