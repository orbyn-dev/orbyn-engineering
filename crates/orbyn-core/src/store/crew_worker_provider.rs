//! A project's stored "Worker provider" choice for Crew routing. It lives in the existing `project_preferences` table (no
//! migration) under a key the settings catalog does not read, as the remembered default branch does. Absent, damaged or
//! unknown text reads as the default, `same_as_lead`, so an older or newer build never blocks a start; only a person's save
//! writes it.

use orbyn_contracts::CrewWorkerProvider;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use super::{AuditRecord, Store, insert_audit};
use crate::error::Result;

const KEY: &str = "crew_worker_provider";

/// The stored text is the choice's wire name (`same_as_lead`, `any`); anything else is not a choice this build knows.
fn parse(text: &str) -> Option<CrewWorkerProvider> {
    serde_json::from_value(Value::String(text.to_owned())).ok()
}

fn name(provider: CrewWorkerProvider) -> &'static str {
    match provider {
        CrewWorkerProvider::SameAsLead => "same_as_lead",
        CrewWorkerProvider::Any => "any",
    }
}

impl Store {
    /// The project's choice, `same_as_lead` when nothing valid is stored. A storage failure is an error: it must not quietly
    /// widen routing to every provider, and the default it would fall back to is the narrower one anyway.
    pub fn crew_worker_provider(&self, project_id: &str) -> Result<CrewWorkerProvider> {
        let text: Option<String> = self
            .reader()
            .query_row(
                "SELECT value FROM project_preferences WHERE project_id = ?1 AND key = ?2",
                params![project_id, KEY],
                |row| row.get(0),
            )
            .optional()?;
        Ok(text.as_deref().and_then(parse).unwrap_or_default())
    }

    /// A person's save for one project, with its audit row in the same transaction. Saving the default clears the row.
    pub fn set_crew_worker_provider(
        &self,
        project_id: &str,
        provider: CrewWorkerProvider,
        actor: &str,
    ) -> Result<()> {
        self.commit(|tx, _| {
            insert_audit(
                tx,
                &AuditRecord {
                    actor: actor.to_owned(),
                    action: "crew.worker_provider.set".to_owned(),
                    target: project_id.to_owned(),
                    decision: None,
                    source: "user".to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({ "worker_provider": name(provider) }),
                },
            )?;
            if provider == CrewWorkerProvider::default() {
                tx.execute(
                    "DELETE FROM project_preferences WHERE project_id = ?1 AND key = ?2",
                    params![project_id, KEY],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO project_preferences (project_id, key, value) VALUES (?1, ?2, ?3)
                     ON CONFLICT (project_id, key) DO UPDATE SET value = excluded.value",
                    params![project_id, KEY, name(provider)],
                )?;
            }
            Ok(())
        })
    }
}
