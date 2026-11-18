//! The commits the app made on a person's action: the only source of the positive `person_commits` list that merge
//! provenance reads (`docs/crew/provenance-person-signal.md`). A row is written in the same transaction as its audit
//! row. A commit the person makes outside the app is never listed, and nothing is inferred from an author or email.
//!
//! No app path commits on a person's action today (the optional README "Initial commit" at project creation is not
//! recorded), so nothing calls `record_person_commit` yet and the list is empty: every line reads `agent` or
//! `unknown`. A future path that does must refuse to record a commit that contains a worker-written change (workers
//! write into the person's own checkout): that check belongs to the path, which knows what it staged.

use rusqlite::params;

use super::{AuditRecord, Store};
use crate::error::{CoreError, Result};

fn full_commit(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The action names a row may carry: short, lowercase, no spaces, so a row never holds free text.
fn plain_action(action: &str) -> bool {
    (1..=64).contains(&action.len())
        && action
            .bytes()
            .all(|b| b.is_ascii_lowercase() || matches!(b, b'_' | b'.'))
}

impl Store {
    /// Records `commit` (a full lowercase id) as one the app made for `project_id` on the person's `action`, and the
    /// audit row `crew.person_commit`, in one transaction. A commit already recorded changes nothing and adds no
    /// second audit row. Refuses a short or uppercase id and an action that is not a plain name.
    pub fn record_person_commit(
        &self,
        project_id: &str,
        commit: &str,
        action: &str,
        at_unix: i64,
    ) -> Result<()> {
        if !full_commit(commit) {
            return Err(CoreError::Invalid(
                "a person commit needs a full lowercase commit id".into(),
            ));
        }
        if !plain_action(action) {
            return Err(CoreError::Invalid(
                "a person commit needs a plain action name".into(),
            ));
        }
        self.commit(|tx, _| {
            let added = tx.execute(
                "INSERT OR IGNORE INTO crew_person_commits (project_id, commit_id, at_unix, action)
                 VALUES (?1, ?2, ?3, ?4)",
                params![project_id, commit, at_unix, action],
            )?;
            if added > 0 {
                super::insert_audit(
                    tx,
                    &AuditRecord {
                        actor: "local_caller".into(),
                        action: "crew.person_commit".into(),
                        target: commit.to_owned(),
                        decision: None,
                        source: "local".into(),
                        thread_id: None,
                        run_id: None,
                        detail: serde_json::json!({ "project": project_id, "action": action }),
                    },
                )?;
            }
            Ok(())
        })
    }

    /// The project's recorded person commits, oldest first.
    pub fn crew_person_commits(&self, project_id: &str) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT commit_id FROM crew_person_commits WHERE project_id = ?1 ORDER BY at_unix, commit_id",
        )?;
        Ok(statement
            .query_map([project_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?)
    }

    /// The person commits of the project that led `task_id`: the list a merge of that task is read against. Empty when
    /// the task or its lead thread is unknown, which reads every line as agent or unknown, never as a person's.
    pub fn crew_person_commits_for_task(&self, task_id: &str) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT c.commit_id FROM crew_person_commits c
             JOIN threads t ON t.project_id = c.project_id
             JOIN crew_runs r ON r.lead_thread_id = t.id
             WHERE r.task_id = ?1 ORDER BY c.at_unix, c.commit_id",
        )?;
        Ok(statement
            .query_map([task_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?)
    }
}
