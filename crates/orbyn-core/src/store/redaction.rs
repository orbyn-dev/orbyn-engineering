//! The one read-change-write of the redaction settings, with its audit row in the same
//! transaction, so two edits cannot lose each other and a widening is never unrecorded.

use rusqlite::OptionalExtension;

use super::{AuditRecord, Store, insert_audit};
use crate::error::Result;

/// The settings key. It is not a `pref:` key and not `server_settings`, so the account's
/// settings sync (an allowlist of preference keys) never reads or writes it.
pub const REDACTION_KEY: &str = "redaction";

impl Store {
    pub fn redaction_json(&self) -> Result<Option<String>> {
        self.setting(REDACTION_KEY)
    }

    /// Reads the stored document, lets `change` return the new one, the audit row to keep (a
    /// change that widens what leaves) and a result, and writes all in one transaction.
    pub(crate) fn change_redaction<T>(
        &self,
        change: impl FnOnce(Option<String>) -> Result<(String, Option<AuditRecord>, T)>,
    ) -> Result<T> {
        self.commit(|tx, _| {
            let old: Option<String> = tx
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [REDACTION_KEY],
                    |row| row.get(0),
                )
                .optional()?;
            let (new, audit, out) = change(old)?;
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                rusqlite::params![REDACTION_KEY, new],
            )?;
            if let Some(record) = audit {
                insert_audit(tx, &record)?;
            }
            Ok(out)
        })
    }
}
