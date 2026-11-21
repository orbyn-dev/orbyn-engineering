//! Issue tracker connections and the links between threads and issues (docs/issues.md). A connection never
//! holds a token.

use orbyn_contracts::{IssueKey, IssueRef, TrackerKind};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeMap;

use super::Store;
use crate::error::Result;

/// A connection as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackerConnection {
    pub tracker: TrackerKind,
    pub host: Option<String>,
    pub scopes: Vec<String>,
}

pub(crate) fn tracker_text(kind: TrackerKind) -> &'static str {
    match kind {
        TrackerKind::Github => "github",
        TrackerKind::Gitlab => "gitlab",
        TrackerKind::AzureDevops => "azure_devops",
        TrackerKind::Forgejo => "forgejo",
        TrackerKind::Linear => "linear",
    }
}

pub(crate) fn tracker_of(text: &str) -> Option<TrackerKind> {
    Some(match text {
        "github" => TrackerKind::Github,
        "gitlab" => TrackerKind::Gitlab,
        "azure_devops" => TrackerKind::AzureDevops,
        "forgejo" => TrackerKind::Forgejo,
        "linear" => TrackerKind::Linear,
        _ => return None,
    })
}

impl Store {
    pub fn tracker_connections(&self) -> Result<Vec<TrackerConnection>> {
        let reader = self.reader();
        let mut statement =
            reader.prepare("SELECT tracker, host, scopes FROM issue_trackers ORDER BY tracker")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (tracker, host, scopes) = row?;
            if let Some(tracker) = tracker_of(&tracker) {
                let scopes = serde_json::from_str(&scopes).unwrap_or_default();
                out.push(TrackerConnection {
                    tracker,
                    host,
                    scopes,
                });
            }
        }
        Ok(out)
    }

    pub fn put_tracker(&self, connection: &TrackerConnection, now: &str) -> Result<()> {
        let scopes = serde_json::to_string(&connection.scopes)?;
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO issue_trackers (tracker, host, scopes, connected_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(tracker) DO UPDATE SET host = ?2, scopes = ?3",
                params![tracker_text(connection.tracker), connection.host, scopes, now],
            )?;
            Ok(())
        })
    }

    pub fn delete_tracker(&self, tracker: TrackerKind) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM issue_trackers WHERE tracker = ?1",
                params![tracker_text(tracker)],
            )?;
            Ok(())
        })
    }

    /// Adds a link, or refreshes its snapshot when it exists. The thread must exist.
    pub fn put_issue_link(&self, thread_id: &str, issue: &IssueRef, now: &str) -> Result<()> {
        let snapshot = serde_json::to_string(issue)?;
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO issue_links (thread_id, tracker, scope, key, snapshot, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(thread_id, tracker, scope, key) DO UPDATE SET snapshot = ?5",
                params![
                    thread_id,
                    tracker_text(issue.tracker),
                    issue.scope,
                    issue.key,
                    snapshot,
                    now
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_issue_link(&self, thread_id: &str, issue: &IssueKey) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM issue_links WHERE thread_id = ?1 AND tracker = ?2 AND scope = ?3 AND key = ?4",
                params![thread_id, tracker_text(issue.tracker), issue.scope, issue.key],
            )?;
            Ok(())
        })
    }

    /// Every thread's linked issues, oldest link first.
    pub fn issue_links(&self) -> Result<BTreeMap<String, Vec<IssueRef>>> {
        let reader = self.reader();
        let mut statement = reader
            .prepare("SELECT thread_id, snapshot FROM issue_links ORDER BY created_at, rowid")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out: BTreeMap<String, Vec<IssueRef>> = BTreeMap::new();
        for row in rows {
            let (thread, snapshot) = row?;
            if let Ok(issue) = serde_json::from_str::<IssueRef>(&snapshot) {
                out.entry(thread).or_default().push(issue);
            }
        }
        Ok(out)
    }

    /// The threads that link `issue`.
    pub fn threads_linking(&self, issue: &IssueKey) -> Result<Vec<String>> {
        let reader = self.reader();
        let mut statement = reader.prepare(
            "SELECT thread_id FROM issue_links WHERE tracker = ?1 AND scope = ?2 AND key = ?3
             ORDER BY created_at, rowid",
        )?;
        let rows = statement.query_map(
            params![tracker_text(issue.tracker), issue.scope, issue.key],
            |row| row.get(0),
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    #[allow(dead_code)]
    fn has_tracker(&self, tracker: TrackerKind) -> Result<bool> {
        let reader = self.reader();
        Ok(reader
            .query_row(
                "SELECT 1 FROM issue_trackers WHERE tracker = ?1",
                params![tracker_text(tracker)],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn a_connection_keeps_host_and_scopes_and_is_replaced_in_place() {
        let dir = TempDir::new();
        let store = Store::open(&dir.path().join("t.sqlite3"), "test").unwrap();
        let conn = TrackerConnection {
            tracker: TrackerKind::Gitlab,
            host: Some("https://gitlab.com".into()),
            scopes: vec!["a/b".into()],
        };
        store.put_tracker(&conn, "2026-11-21T00:00:00Z").unwrap();
        store
            .put_tracker(
                &TrackerConnection {
                    scopes: vec!["a/b".into(), "c/d".into()],
                    ..conn.clone()
                },
                "2026-11-21T00:01:00Z",
            )
            .unwrap();
        let all = store.tracker_connections().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].scopes, ["a/b", "c/d"]);
        store.delete_tracker(TrackerKind::Gitlab).unwrap();
        assert!(store.tracker_connections().unwrap().is_empty());
    }
}
