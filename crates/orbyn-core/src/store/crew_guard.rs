//! The blast-radius guard's lists at rest (docs/crew.md, "As built (slice 4i, guard)"). The caller validates the pattern and the
//! note; this layer keeps the rules that must hold whatever the caller does: at most [`ENTRIES_MAX`] entries, no duplicate, and an
//! audit row written in the same transaction, before the change (rule 6).

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde_json::{Value, json};

use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::{new_id, now};
use orbyn_contracts::{GuardList, GuardTarget, GuardTargetKind};

/// The most entries the two lists hold together, so the lists stay readable and a loop cannot fill the table.
pub const ENTRIES_MAX: u32 = 200;

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> rusqlite::Result<T> {
    serde_json::from_value(Value::String(text.to_owned())).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn target(row: &Row<'_>) -> rusqlite::Result<GuardTarget> {
    Ok(GuardTarget {
        id: row.get("id")?,
        list: parse(&row.get::<_, String>("list")?)?,
        pattern: row.get("pattern")?,
        kind: parse(&row.get::<_, String>("kind")?)?,
        note: row.get("note")?,
        added_at: row.get("added_at")?,
    })
}

fn audit(tx: &Transaction<'_>, action: &str, id: &str, detail: Value) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "user".to_owned(),
            action: action.to_owned(),
            target: id.to_owned(),
            decision: None,
            source: "user".to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

fn name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl Store {
    /// Both lists, oldest first.
    pub fn crew_guard_targets(&self) -> Result<Vec<GuardTarget>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT * FROM crew_guard_targets ORDER BY added_at, id")?;
        Ok(statement
            .query_map([], target)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One entry.
    pub fn crew_guard_target(&self, id: &str) -> Result<Option<GuardTarget>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_guard_targets WHERE id = ?1",
                [id],
                target,
            )
            .optional()?)
    }

    /// Adds one entry, with its audit row first in the same transaction.
    pub fn add_crew_guard_target(
        &self,
        list: GuardList,
        pattern: &str,
        kind: GuardTargetKind,
        note: Option<&str>,
    ) -> Result<GuardTarget> {
        let id = format!("gt-{}", new_id());
        let (list_name, kind_name) = (name(&list), name(&kind));
        self.commit(|tx, _| {
            let held: u32 =
                tx.query_row("SELECT COUNT(*) FROM crew_guard_targets", [], |row| {
                    row.get(0)
                })?;
            if held >= ENTRIES_MAX {
                return Err(CoreError::Invalid(format!(
                    "the two lists hold at most {ENTRIES_MAX} entries"
                )));
            }
            let same: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM crew_guard_targets WHERE list = ?1 AND kind = ?2 AND pattern = ?3)",
                params![list_name, kind_name, pattern],
                |row| row.get(0),
            )?;
            if same {
                return Err(CoreError::Invalid("this entry is already on the list".into()));
            }
            audit(
                tx,
                "crew.guard.add",
                &id,
                json!({ "list": list_name, "pattern": pattern, "kind": kind_name, "note": note }),
            )?;
            tx.execute(
                "INSERT INTO crew_guard_targets (id, list, pattern, kind, note, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, list_name, pattern, kind_name, note, now()],
            )?;
            Ok(tx.query_row(
                "SELECT * FROM crew_guard_targets WHERE id = ?1",
                [&id],
                target,
            )?)
        })
    }

    /// Removes one entry, with its audit row first in the same transaction. The entry removed is returned.
    pub fn remove_crew_guard_target(&self, id: &str) -> Result<GuardTarget> {
        self.commit(|tx, _| {
            let found = tx
                .query_row(
                    "SELECT * FROM crew_guard_targets WHERE id = ?1",
                    [id],
                    target,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("guard target".into()))?;
            audit(
                tx,
                "crew.guard.remove",
                id,
                json!({
                    "list": name(&found.list),
                    "pattern": found.pattern,
                    "kind": name(&found.kind),
                }),
            )?;
            tx.execute("DELETE FROM crew_guard_targets WHERE id = ?1", [id])?;
            Ok(found)
        })
    }
}
