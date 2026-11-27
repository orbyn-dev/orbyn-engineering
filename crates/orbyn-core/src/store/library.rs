//! The personal library at rest (docs/execution-profiles-and-library.md, Part 2): workflows, prompts and environment sets. An
//! environment set is a name, key NAMES and whether each name has a value (values live in the OS keychain): no table or column
//! here can hold a value, and a test fails if one could. Every write adds its audit row first, in the same transaction, with ids,
//! set names and key names only, never a command's text, a prompt's body or a value.

use orbyn_contracts::Param;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::{new_id, now};

/// The count limits of "Limits" that answer `409 library_limit`.
pub const MAX_WORKFLOWS: usize = 200;
pub const MAX_PROMPTS: usize = 200;
pub const MAX_ENV_SETS: usize = 50;

fn refused(status: u16, code: &'static str, message: &str, detail: Value) -> CoreError {
    CoreError::Refused {
        status,
        code,
        message: message.to_owned(),
        detail,
    }
}

fn limit(what: &str, max: usize) -> CoreError {
    refused(
        409,
        "library_limit",
        &format!("The library holds at most {max} {what}. Delete one first."),
        json!({ "limit": max }),
    )
}

fn taken(what: &str) -> CoreError {
    refused(
        409,
        "name_taken",
        &format!("A {what} with this name already exists."),
        json!({ "field": "name" }),
    )
}

fn conflict(what: &str, revision: u32) -> CoreError {
    refused(
        409,
        "revision_conflict",
        &format!("This {what} changed. Review it again before saving."),
        json!({ "revision": revision }),
    )
}

/// The key a name is unique by: trimmed and lowercased.
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn revision_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<u32> {
    Ok(u32::try_from(row.get::<_, i64>("revision")?).unwrap_or(u32::MAX))
}

fn json_column<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    column: &str,
) -> rusqlite::Result<T> {
    let text: String = row.get(column)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn audit(tx: &Transaction<'_>, action: &str, target: &str, detail: Value) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "user".into(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

/// What a create writes and a change leaves behind, already validated.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowFields {
    pub name: String,
    pub description: String,
    pub command: String,
    pub params: Vec<Param>,
    pub env_set_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowRow {
    pub id: String,
    pub fields: WorkflowFields,
    pub source: String,
    /// The hash of the command and parameters a person acknowledged, for a non-local entry.
    pub acked_hash: Option<String>,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptFields {
    pub name: String,
    pub description: String,
    pub body: String,
    pub params: Vec<Param>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptRow {
    pub id: String,
    pub fields: PromptFields,
    pub source: String,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvSetFields {
    pub name: String,
    pub description: String,
    /// Key names, in order. No values.
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvSetRow {
    pub id: String,
    pub fields: EnvSetFields,
    /// Per key in `fields.keys`, in the same order: whether the keychain held a value when it was last checked. `None` is a
    /// row from before migration 72, or one whose check did not answer.
    pub is_set: Vec<Option<bool>>,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}

const WORKFLOW_COLUMNS: &str = "id, name, description, command, params_json, env_set_ids_json, source, acked_hash, revision, created_at, updated_at";
const PROMPT_COLUMNS: &str =
    "id, name, description, body, params_json, source, revision, created_at, updated_at";

fn workflow_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRow> {
    Ok(WorkflowRow {
        id: row.get("id")?,
        fields: WorkflowFields {
            name: row.get("name")?,
            description: row.get("description")?,
            command: row.get("command")?,
            params: json_column(row, "params_json")?,
            env_set_ids: json_column(row, "env_set_ids_json")?,
        },
        source: row.get("source")?,
        acked_hash: row.get("acked_hash")?,
        revision: revision_of(row)?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn prompt_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PromptRow> {
    Ok(PromptRow {
        id: row.get("id")?,
        fields: PromptFields {
            name: row.get("name")?,
            description: row.get("description")?,
            body: row.get("body")?,
            params: json_column(row, "params_json")?,
        },
        source: row.get("source")?,
        revision: revision_of(row)?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn count(tx: &Transaction<'_>, table: &str) -> Result<usize> {
    let n: i64 = tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })?;
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

fn name_used(tx: &Transaction<'_>, table: &str, key: &str, except: &str) -> Result<bool> {
    Ok(tx.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE name_key = ?1 AND id <> ?2)"),
        params![key, except],
        |row| row.get(0),
    )?)
}

fn env_set_keys(tx: &rusqlite::Connection, id: &str) -> Result<(Vec<String>, Vec<Option<bool>>)> {
    let mut statement =
        tx.prepare("SELECT key, is_set FROM library_env_keys WHERE set_id = ?1 ORDER BY rowid")?;
    let rows = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut keys = Vec::new();
    let mut is_set = Vec::new();
    for row in rows {
        let (key, flag) = row?;
        keys.push(key);
        is_set.push(flag);
    }
    Ok((keys, is_set))
}

fn load_env_set(tx: &rusqlite::Connection, id: &str) -> Result<EnvSetRow> {
    let (name, description, revision, created_at, updated_at) = tx
        .query_row(
            "SELECT name, description, revision, created_at, updated_at FROM library_env_sets WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("environment set".into()))?;
    let (keys, is_set) = env_set_keys(tx, id)?;
    Ok(EnvSetRow {
        id: id.to_owned(),
        fields: EnvSetFields {
            name,
            description,
            keys,
        },
        is_set,
        revision: u32::try_from(revision).unwrap_or(u32::MAX),
        created_at,
        updated_at,
    })
}

/// A workflow may list only sets that exist, each once.
fn check_sets_exist(tx: &Transaction<'_>, ids: &[String]) -> Result<()> {
    for id in ids {
        let found: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM library_env_sets WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        if !found {
            return Err(CoreError::Invalid(
                "env_set_ids: name only sets that exist.".into(),
            ));
        }
    }
    Ok(())
}

fn insert_keys(tx: &Transaction<'_>, set_id: &str, keys: &[String], stamp: &str) -> Result<()> {
    for key in keys {
        tx.execute(
            "INSERT OR IGNORE INTO library_env_keys (set_id, key, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![set_id, key, stamp],
        )?;
    }
    Ok(())
}

/// Records what a keychain check found for one key. It changes neither the set nor the key's timestamps.
fn mark_key(tx: &Transaction<'_>, set_id: &str, key: &str, is_set: Option<bool>) -> Result<()> {
    tx.execute(
        "UPDATE library_env_keys SET is_set = ?3 WHERE set_id = ?1 AND key = ?2",
        params![set_id, key, is_set.map(i64::from)],
    )?;
    Ok(())
}

impl Store {
    /// How many workflows, prompts and environment sets the library holds.
    pub fn library_counts(&self) -> Result<(usize, usize, usize)> {
        let reader = self.reader();
        let one = |table: &str| -> Result<usize> {
            let n: i64 =
                reader.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
            Ok(usize::try_from(n).unwrap_or(usize::MAX))
        };
        Ok((
            one("library_workflows")?,
            one("library_prompts")?,
            one("library_env_sets")?,
        ))
    }

    /// A Person write that touches no row, such as a workflow run or an environment value, still leaves its audit row.
    pub fn library_audit(&self, action: &str, target: &str, detail: Value) -> Result<()> {
        self.commit(|tx, _| audit(tx, action, target, detail))
    }

    // ---- workflows ----

    pub fn library_workflows(&self) -> Result<Vec<WorkflowRow>> {
        let reader = self.reader();
        let mut statement = reader.prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM library_workflows ORDER BY created_at, id"
        ))?;
        let rows = statement.query_map([], workflow_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn library_workflow(&self, id: &str) -> Result<WorkflowRow> {
        self.reader()
            .query_row(
                &format!("SELECT {WORKFLOW_COLUMNS} FROM library_workflows WHERE id = ?1"),
                [id],
                workflow_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound("workflow".into()))
    }

    pub fn create_library_workflow(&self, fields: &WorkflowFields) -> Result<WorkflowRow> {
        self.create_library_workflow_from(fields, "local")
    }

    /// `source` is `local`, `import`, `repo` or `shared`.
    pub fn create_library_workflow_from(
        &self,
        fields: &WorkflowFields,
        source: &str,
    ) -> Result<WorkflowRow> {
        self.commit(|tx, _| {
            if count(tx, "library_workflows")? >= MAX_WORKFLOWS {
                return Err(limit("workflows", MAX_WORKFLOWS));
            }
            let key = name_key(&fields.name);
            if name_used(tx, "library_workflows", &key, "")? {
                return Err(taken("workflow"));
            }
            check_sets_exist(tx, &fields.env_set_ids)?;
            let (id, stamp) = (new_id(), now());
            audit(
                tx,
                "library.workflow.created",
                &id,
                json!({ "id": id, "name": fields.name, "source": source }),
            )?;
            tx.execute(
                "INSERT INTO library_workflows
                    (id, name, name_key, description, command, params_json, env_set_ids_json, source, revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?9, 1, ?8, ?8)",
                params![
                    id,
                    fields.name,
                    key,
                    fields.description,
                    fields.command,
                    serde_json::to_string(&fields.params)?,
                    serde_json::to_string(&fields.env_set_ids)?,
                    stamp,
                    source
                ],
            )?;
            Ok(tx.query_row(
                &format!("SELECT {WORKFLOW_COLUMNS} FROM library_workflows WHERE id = ?1"),
                [&id],
                workflow_row,
            )?)
        })
    }

    pub fn update_library_workflow(
        &self,
        id: &str,
        seen: u32,
        fields: &WorkflowFields,
    ) -> Result<WorkflowRow> {
        self.commit(|tx, _| {
            let current = tx
                .query_row(
                    &format!("SELECT {WORKFLOW_COLUMNS} FROM library_workflows WHERE id = ?1"),
                    [id],
                    workflow_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("workflow".into()))?;
            if current.revision != seen {
                return Err(conflict("workflow", current.revision));
            }
            let key = name_key(&fields.name);
            if name_used(tx, "library_workflows", &key, id)? {
                return Err(taken("workflow"));
            }
            check_sets_exist(tx, &fields.env_set_ids)?;
            let revision = current.revision.saturating_add(1);
            audit(
                tx,
                "library.workflow.changed",
                id,
                json!({ "id": id, "name": fields.name, "revision": revision }),
            )?;
            tx.execute(
                "UPDATE library_workflows SET name = ?2, name_key = ?3, description = ?4, command = ?5, params_json = ?6,
                     env_set_ids_json = ?7, revision = ?8, updated_at = ?9 WHERE id = ?1",
                params![
                    id,
                    fields.name,
                    key,
                    fields.description,
                    fields.command,
                    serde_json::to_string(&fields.params)?,
                    serde_json::to_string(&fields.env_set_ids)?,
                    i64::from(revision),
                    now()
                ],
            )?;
            Ok(tx.query_row(
                &format!("SELECT {WORKFLOW_COLUMNS} FROM library_workflows WHERE id = ?1"),
                [id],
                workflow_row,
            )?)
        })
    }

    /// Records that the person acknowledged this command and these parameters (by hash). It is not a change to the entry.
    pub fn ack_library_workflow(&self, id: &str, hash: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE library_workflows SET acked_hash = ?2, acked_at = ?3 WHERE id = ?1",
                params![id, hash, now()],
            )?;
            Ok(())
        })
    }

    pub fn delete_library_workflow(&self, id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let name: String = tx
                .query_row(
                    "SELECT name FROM library_workflows WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("workflow".into()))?;
            audit(
                tx,
                "library.workflow.deleted",
                id,
                json!({ "id": id, "name": name }),
            )?;
            tx.execute("DELETE FROM library_workflows WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    // ---- prompts ----

    pub fn library_prompts(&self) -> Result<Vec<PromptRow>> {
        let reader = self.reader();
        let mut statement = reader.prepare(&format!(
            "SELECT {PROMPT_COLUMNS} FROM library_prompts ORDER BY created_at, id"
        ))?;
        let rows = statement.query_map([], prompt_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn library_prompt(&self, id: &str) -> Result<PromptRow> {
        self.reader()
            .query_row(
                &format!("SELECT {PROMPT_COLUMNS} FROM library_prompts WHERE id = ?1"),
                [id],
                prompt_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound("prompt".into()))
    }

    pub fn create_library_prompt(&self, fields: &PromptFields) -> Result<PromptRow> {
        self.create_library_prompt_from(fields, "local")
    }

    pub fn create_library_prompt_from(
        &self,
        fields: &PromptFields,
        source: &str,
    ) -> Result<PromptRow> {
        self.commit(|tx, _| {
            if count(tx, "library_prompts")? >= MAX_PROMPTS {
                return Err(limit("prompts", MAX_PROMPTS));
            }
            let key = name_key(&fields.name);
            if name_used(tx, "library_prompts", &key, "")? {
                return Err(taken("prompt"));
            }
            let (id, stamp) = (new_id(), now());
            audit(
                tx,
                "library.prompt.created",
                &id,
                json!({ "id": id, "name": fields.name }),
            )?;
            tx.execute(
                "INSERT INTO library_prompts (id, name, name_key, description, body, params_json, source, revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?8, 1, ?7, ?7)",
                params![
                    id,
                    fields.name,
                    key,
                    fields.description,
                    fields.body,
                    serde_json::to_string(&fields.params)?,
                    stamp,
                    source
                ],
            )?;
            Ok(tx.query_row(
                &format!("SELECT {PROMPT_COLUMNS} FROM library_prompts WHERE id = ?1"),
                [&id],
                prompt_row,
            )?)
        })
    }

    pub fn update_library_prompt(
        &self,
        id: &str,
        seen: u32,
        fields: &PromptFields,
    ) -> Result<PromptRow> {
        self.commit(|tx, _| {
            let current = tx
                .query_row(
                    &format!("SELECT {PROMPT_COLUMNS} FROM library_prompts WHERE id = ?1"),
                    [id],
                    prompt_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("prompt".into()))?;
            if current.revision != seen {
                return Err(conflict("prompt", current.revision));
            }
            let key = name_key(&fields.name);
            if name_used(tx, "library_prompts", &key, id)? {
                return Err(taken("prompt"));
            }
            let revision = current.revision.saturating_add(1);
            audit(
                tx,
                "library.prompt.changed",
                id,
                json!({ "id": id, "name": fields.name, "revision": revision }),
            )?;
            tx.execute(
                "UPDATE library_prompts SET name = ?2, name_key = ?3, description = ?4, body = ?5, params_json = ?6,
                     revision = ?7, updated_at = ?8 WHERE id = ?1",
                params![
                    id,
                    fields.name,
                    key,
                    fields.description,
                    fields.body,
                    serde_json::to_string(&fields.params)?,
                    i64::from(revision),
                    now()
                ],
            )?;
            Ok(tx.query_row(
                &format!("SELECT {PROMPT_COLUMNS} FROM library_prompts WHERE id = ?1"),
                [id],
                prompt_row,
            )?)
        })
    }

    pub fn delete_library_prompt(&self, id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let name: String = tx
                .query_row(
                    "SELECT name FROM library_prompts WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("prompt".into()))?;
            audit(
                tx,
                "library.prompt.deleted",
                id,
                json!({ "id": id, "name": name }),
            )?;
            tx.execute("DELETE FROM library_prompts WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    // ---- environment sets: names only ----

    pub fn library_env_sets(&self) -> Result<Vec<EnvSetRow>> {
        let reader = self.reader();
        // One snapshot, so a set and its keys come from the same moment.
        let tx = reader.unchecked_transaction()?;
        let ids: Vec<String> = {
            let mut statement =
                tx.prepare("SELECT id FROM library_env_sets ORDER BY created_at, id")?;
            let ids = statement.query_map([], |row| row.get(0))?;
            ids.collect::<std::result::Result<_, _>>()?
        };
        ids.iter().map(|id| load_env_set(&tx, id)).collect()
    }

    pub fn library_env_set(&self, id: &str) -> Result<EnvSetRow> {
        load_env_set(&self.reader(), id)
    }

    pub fn create_library_env_set(&self, fields: &EnvSetFields) -> Result<EnvSetRow> {
        self.commit(|tx, _| {
            if count(tx, "library_env_sets")? >= MAX_ENV_SETS {
                return Err(limit("environment sets", MAX_ENV_SETS));
            }
            let key = name_key(&fields.name);
            if name_used(tx, "library_env_sets", &key, "")? {
                return Err(taken("environment set"));
            }
            let (id, stamp) = (new_id(), now());
            audit(
                tx,
                "library.env_set.created",
                &id,
                json!({ "id": id, "name": fields.name, "keys": fields.keys }),
            )?;
            tx.execute(
                "INSERT INTO library_env_sets (id, name, name_key, description, revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
                params![id, fields.name, key, fields.description, stamp],
            )?;
            insert_keys(tx, &id, &fields.keys, &stamp)?;
            load_env_set(tx, &id)
        })
    }

    /// Replaces the name, the description and the key names when `seen` is the current revision. A name left out is dropped.
    /// The caller has already removed the keychain items of the names that go (rows are changed after the items).
    pub fn update_library_env_set(
        &self,
        id: &str,
        seen: u32,
        fields: &EnvSetFields,
    ) -> Result<EnvSetRow> {
        self.commit(|tx, _| {
            let current = load_env_set(tx, id)?;
            if current.revision != seen {
                return Err(conflict("environment set", current.revision));
            }
            let key = name_key(&fields.name);
            if name_used(tx, "library_env_sets", &key, id)? {
                return Err(taken("environment set"));
            }
            let revision = current.revision.saturating_add(1);
            let removed: Vec<&String> = current
                .fields
                .keys
                .iter()
                .filter(|name| !fields.keys.contains(name))
                .collect();
            audit(
                tx,
                "library.env_set.changed",
                id,
                json!({ "id": id, "name": fields.name, "revision": revision, "keys": fields.keys, "removed_keys": removed }),
            )?;
            let stamp = now();
            tx.execute(
                "UPDATE library_env_sets SET name = ?2, name_key = ?3, description = ?4, revision = ?5, updated_at = ?6 WHERE id = ?1",
                params![id, fields.name, key, fields.description, i64::from(revision), stamp],
            )?;
            for name in removed {
                tx.execute(
                    "DELETE FROM library_env_keys WHERE set_id = ?1 AND key = ?2",
                    params![id, name],
                )?;
            }
            insert_keys(tx, id, &fields.keys, &stamp)?;
            load_env_set(tx, id)
        })
    }

    /// Removes the set and its names, after the caller removed every keychain item. A workflow that listed it forgets it.
    pub fn delete_library_env_set(&self, id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let current = load_env_set(tx, id)?;
            audit(
                tx,
                "library.env_set.deleted",
                id,
                json!({ "id": id, "name": current.fields.name, "keys": current.fields.keys }),
            )?;
            let workflows: Vec<(String, String)> = {
                let mut statement =
                    tx.prepare("SELECT id, env_set_ids_json FROM library_workflows")?;
                let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            for (workflow, listed) in workflows {
                let mut ids: Vec<String> = serde_json::from_str(&listed).unwrap_or_default();
                if ids.iter().any(|one| one == id) {
                    ids.retain(|one| one != id);
                    tx.execute(
                        "UPDATE library_workflows SET env_set_ids_json = ?2, revision = revision + 1, updated_at = ?3 WHERE id = ?1",
                        params![workflow, serde_json::to_string(&ids)?, now()],
                    )?;
                }
            }
            tx.execute("DELETE FROM library_env_sets WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// Records whether the keychain holds a value for one key. It is not a change to the set. `None` forgets what was known.
    pub fn library_mark_key(&self, set_id: &str, key: &str, is_set: Option<bool>) -> Result<()> {
        self.commit(|tx, _| mark_key(tx, set_id, key, is_set))
    }

    /// Records the answers of several keychain checks in one transaction.
    pub fn library_mark_keys(&self, marks: &[(String, String, bool)]) -> Result<()> {
        if marks.is_empty() {
            return Ok(());
        }
        self.commit(|tx, _| {
            for (set_id, key, is_set) in marks {
                mark_key(tx, set_id, key, Some(*is_set))?;
            }
            Ok(())
        })
    }

    /// The audit row of a value being set or a key's value removed, written before the keychain is touched. It names the set
    /// and the key and checks the key is in the set. It never carries a value.
    pub fn library_key_event(&self, action: &str, set_id: &str, key: &str) -> Result<()> {
        self.commit(|tx, _| {
            let set = load_env_set(tx, set_id)?;
            if !set.fields.keys.iter().any(|name| name == key) {
                return Err(CoreError::NotFound("environment key".into()));
            }
            audit(
                tx,
                action,
                set_id,
                json!({ "id": set_id, "name": set.fields.name, "key": key }),
            )?;
            tx.execute(
                "UPDATE library_env_keys SET updated_at = ?3 WHERE set_id = ?1 AND key = ?2",
                params![set_id, key, now()],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
impl Store {
    pub(crate) fn reader_for_test(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.reader()
    }

    pub(crate) fn writer_for_test(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.writer()
    }

    pub(crate) fn untrust_project_for_test(&self, id: &str) {
        self.writer()
            .execute("UPDATE projects SET trusted = 0 WHERE id = ?1", [id])
            .unwrap();
    }
}
