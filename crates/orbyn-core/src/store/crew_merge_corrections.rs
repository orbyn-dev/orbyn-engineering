//! Immutable parent-merge bindings for code-owned corrective tasks.
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::{Store, crew_runs::find_run};
use crate::error::{CoreError, Result};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForgeCorrection {
    pub parent_task: String,
    pub parent_commit: String,
    pub task_id: String,
    pub run_id: String,
    pub repo: String,
    pub remote: String,
    pub branch: String,
    pub base: String,
    pub candidate: String,
    pub tree: String,
    pub checkout: String,
}

impl Store {
    pub(crate) fn crew_forge_correction(&self, parent: &str) -> Result<Option<ForgeCorrection>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_forge_corrections WHERE parent_task=?1",
                [parent],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|body| serde_json::from_str(&body).map_err(CoreError::from))
            .transpose()
    }

    pub(crate) fn crew_correction_for_task(&self, task: &str) -> Result<Option<ForgeCorrection>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_forge_corrections WHERE task_id=?1",
                [task],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|body| serde_json::from_str(&body).map_err(CoreError::from))
            .transpose()
    }

    pub(crate) fn crew_pending_corrections(&self) -> Result<Vec<ForgeCorrection>> {
        let conn = self.reader();
        let mut statement = conn.prepare("SELECT body FROM crew_forge_corrections WHERE completed_commit IS NULL ORDER BY rowid LIMIT 129")?;
        let bodies = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if bodies.len() > 128 {
            return Err(CoreError::Invalid(
                "more than 128 corrective tasks need a person".into(),
            ));
        }
        bodies
            .into_iter()
            .map(|body| serde_json::from_str(&body).map_err(CoreError::from))
            .collect()
    }

    /// Completed corrections, newest first, so their checkout and reference can be retired once nothing reads them.
    pub(crate) fn crew_completed_corrections(&self) -> Result<Vec<ForgeCorrection>> {
        let conn = self.reader();
        let mut statement = conn.prepare("SELECT body FROM crew_forge_corrections WHERE completed_commit IS NOT NULL ORDER BY rowid DESC LIMIT 256")?;
        let bodies = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        bodies
            .into_iter()
            .map(|body| serde_json::from_str(&body).map_err(CoreError::from))
            .collect()
    }

    /// A review or fix of the correction still reads its checkout: another unfinished task runs in that folder.
    pub(crate) fn crew_correction_checkout_in_use(
        &self,
        correction: &ForgeCorrection,
    ) -> Result<bool> {
        let count: i64 = self.reader().query_row(
            "SELECT COUNT(*) FROM crew_runs r JOIN crew_tasks t ON t.id=r.task_id WHERE r.worktree=?1 AND r.task_id<>?2 AND t.state NOT IN ('passed','stopped','cancelled')",
            params![correction.checkout, correction.task_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// `actor` is who asked: `person` for a Revert answer, `watch` for the code's own verification of a broken main. The audit
    /// row is written first, in the same transaction as the binding, and only when the binding is new.
    pub(crate) fn add_crew_forge_correction(
        &self,
        correction: &ForgeCorrection,
        actor: &str,
    ) -> Result<()> {
        if correction.task_id == correction.parent_task
            || correction.run_id.is_empty()
            || !Path::new(&correction.repo).is_absolute()
            || !Path::new(&correction.checkout).is_absolute()
            || [
                &correction.parent_commit,
                &correction.base,
                &correction.candidate,
                &correction.tree,
            ]
            .iter()
            .any(|commit| !crate::source_control::forge::crew::is_commit(commit))
        {
            return Err(CoreError::Invalid("invalid corrective binding".into()));
        }
        let body = serde_json::to_string(correction)?;
        self.commit(|tx, _| {
            let original: String = tx.query_row(
                "SELECT merge_commit FROM crew_merges WHERE task_id=?1",
                [&correction.parent_task],
                |row| row.get(0),
            )?;
            if original != correction.parent_commit {
                return Err(CoreError::Invalid("the original merge changed".into()));
            }
            if let Some(prior) = tx
                .query_row(
                    "SELECT body FROM crew_forge_corrections WHERE parent_task=?1",
                    [&correction.parent_task],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            {
                if prior != body {
                    return Err(CoreError::Invalid(
                        "the corrective task identity is immutable".into(),
                    ));
                }
                return Ok(());
            }
            let crew: String = tx.query_row(
                "SELECT crew_id FROM crew_merges WHERE task_id=?1",
                [&correction.parent_task],
                |row| row.get(0),
            )?;
            let event = serde_json::json!({
                "actor": actor,
                "parent": correction.parent_task,
                "parent_commit": correction.parent_commit,
                "base": correction.base,
                "candidate": correction.candidate,
                "tree": correction.tree,
            });
            tx.execute(
                "INSERT INTO crew_autopilot_events(crew_id,task_id,kind,body,at) VALUES(?1,?2,'correction_requested',?3,?4)",
                params![crew, correction.parent_task, event.to_string(), crate::util::now()],
            )?;
            tx.execute(
                "INSERT INTO crew_forge_corrections(parent_task,task_id,body) VALUES(?1,?2,?3)",
                params![correction.parent_task, correction.task_id, body],
            )?;
            Ok(())
        })
    }

    /// Only this binding can admit a provider-free task checkout. It cannot attach a model worker.
    pub(crate) fn bind_crew_correction_checkout(&self, correction: &ForgeCorrection) -> Result<()> {
        self.commit(|tx, _| {
            let body: String = tx.query_row(
                "SELECT body FROM crew_forge_corrections WHERE task_id=?1",
                [&correction.task_id],
                |row| row.get(0),
            )?;
            if serde_json::from_str::<ForgeCorrection>(&body)? != *correction {
                return Err(CoreError::Invalid(
                    "the corrective checkout binding changed".into(),
                ));
            }
            let run = find_run(tx, &correction.task_id)?;
            if run.worker_thread_id.is_some() {
                return Err(CoreError::Invalid(
                    "a code-owned correction cannot have a model worker".into(),
                ));
            }
            tx.execute(
                "UPDATE crew_runs SET worktree=?2,base_commit=?3 WHERE task_id=?1",
                params![correction.task_id, correction.checkout, correction.base],
            )?;
            Ok(())
        })
    }

    /// Projection is receipt-based and idempotent, including a crash after recording the corrective merge.
    pub(crate) fn finish_crew_correction(&self, task: &str) -> Result<bool> {
        self.commit(|tx, _| {
            let stored: Option<(String,Option<String>)> = tx.query_row(
                "SELECT body,completed_commit FROM crew_forge_corrections WHERE task_id=?1", [task],
                |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
            let Some((body,None)) = stored else { return Ok(false); };
            let correction: ForgeCorrection = serde_json::from_str(&body)?;
            let receipt: Option<(String,String,String,Option<String>)> = tx.query_row(
                "SELECT merge_commit,before_commit,candidate,needs_person FROM crew_merges WHERE task_id=?1", [task],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let Some((commit,base,candidate,None)) = receipt else { return Ok(false); };
            if base != correction.base || candidate != correction.candidate {
                return Err(CoreError::Invalid("the corrective merge receipt differs".into()));
            }
            let changed = tx.execute("UPDATE crew_merges SET review_state='reverted',reverted_commit=?2,needs_person=NULL,reviewed_unix=?3 WHERE task_id=?1 AND merge_commit=?4",
                params![correction.parent_task,commit,crate::util::unix_now(),correction.parent_commit])?;
            if changed != 1 {
                // The original merge is gone or changed: completing would fail on every start. Ask a person once instead.
                tx.execute(
                    "UPDATE crew_merges SET needs_person=?2 WHERE task_id=?1",
                    params![correction.parent_task, "The corrective PR merged, but the original merge record changed. A person must reconcile it."],
                )?;
                return Ok(false);
            }
            Ok(true)
        })
    }

    /// Final completion follows the idempotent area effect. Ledger and receipt commit together.
    pub(crate) fn complete_crew_correction(&self, task: &str, commit: &str) -> Result<()> {
        self.commit(|tx,_| {
            let stored:Option<(String,Option<String>)>=tx.query_row("SELECT body,completed_commit FROM crew_forge_corrections WHERE task_id=?1",[task],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
            let Some((body,completed))=stored else {return Ok(());};
            if let Some(prior)=completed {
                return if prior==commit {Ok(())} else {Err(CoreError::Invalid("the completed corrective receipt cannot change".into()))};
            }
            let correction:ForgeCorrection=serde_json::from_str(&body)?;
            let (crew,original_revert):(String,Option<String>)=tx.query_row("SELECT crew_id,reverted_commit FROM crew_merges WHERE task_id=?1 AND review_state='reverted'",[&correction.parent_task],|row|Ok((row.get(0)?,row.get(1)?)))?;
            let observed:String=tx.query_row("SELECT merge_commit FROM crew_merges WHERE task_id=?1 AND needs_person IS NULL",[task],|row|row.get(0))?;
            if observed!=commit || original_revert.as_deref()!=Some(commit) {return Err(CoreError::Invalid("the corrective completion receipt differs".into()));}
            let event=serde_json::json!({"revert":commit,"corrective_task":task,"by":"forge_receipt","verified":true});
            tx.execute("INSERT INTO crew_autopilot_events(crew_id,task_id,kind,body,at) VALUES(?1,?2,'reverted',?3,?4)",params![crew,correction.parent_task,event.to_string(),crate::util::now()])?;
            tx.execute("UPDATE crew_forge_corrections SET completed_commit=?2 WHERE task_id=?1 AND completed_commit IS NULL",params![task,commit])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Store, migrations};
    use crate::testing::TempDir;
    use rusqlite::Connection;

    #[test]
    fn correction_migration_preserves_schema55_and_reopens_idempotently() {
        let dir = TempDir::new();
        let path = dir.path().join("correction.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
        )
        .unwrap();
        for (version, sql) in migrations::MIGRATIONS
            .iter()
            .filter(|(version, _)| *version <= 55)
        {
            conn.execute_batch(sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations VALUES(?1,'fixture')",
                [version],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO settings(key,value) VALUES('correction-migration','preserved')",
            [],
        )
        .unwrap();
        drop(conn);
        let store = Store::open(&path, "correction-migration").unwrap();
        assert_eq!(
            store
                .reader()
                .query_row(
                    "SELECT value FROM settings WHERE key='correction-migration'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "preserved"
        );
        assert_eq!(
            store
                .reader()
                .query_row("SELECT COUNT(*) FROM crew_forge_corrections", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        drop(store);
        let reopened = Store::open(&path, "correction-reopen").unwrap();
        assert_eq!(
            reopened
                .reader()
                .query_row(
                    "SELECT COUNT(*) FROM schema_migrations WHERE version=56",
                    [],
                    |r| r.get::<_, u32>(0)
                )
                .unwrap(),
            1
        );
    }
}
