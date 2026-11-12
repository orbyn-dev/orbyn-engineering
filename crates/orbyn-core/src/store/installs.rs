//! The kept outcome of each engine install, update and removal: newest first, a bounded
//! number per engine. Text arrives already redacted; nothing here reads a setting.

use orbyn_contracts::{InstallKind, InstallOutcome};
use rusqlite::{Row, params};

use super::Store;
use crate::error::Result;

/// How many outcomes an engine keeps. The oldest go first.
pub const INSTALLS_KEPT: usize = 20;

fn kind_text(kind: InstallKind) -> &'static str {
    match kind {
        InstallKind::Install => "install",
        InstallKind::Update => "update",
        InstallKind::Remove => "remove",
    }
}

fn kind_of(text: &str) -> InstallKind {
    match text {
        "install" => InstallKind::Install,
        "remove" => InstallKind::Remove,
        _ => InstallKind::Update,
    }
}

fn outcome_row(row: &Row<'_>) -> rusqlite::Result<InstallOutcome> {
    let output: String = row.get("output")?;
    Ok(InstallOutcome {
        id: row.get("id")?,
        engine: row.get("engine")?,
        kind: kind_of(&row.get::<_, String>("kind")?),
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        ok: row.get("ok")?,
        reason: row.get("reason")?,
        command: row.get("command")?,
        exit_status: row.get("exit_status")?,
        from_version: row.get("from_version")?,
        to_version: row.get("to_version")?,
        output: output
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect(),
    })
}

impl Store {
    /// Keeps one outcome and drops the engine's oldest beyond `INSTALLS_KEPT`, together.
    pub fn record_install(&self, outcome: &InstallOutcome) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO engine_installs (id, engine, kind, started_at, finished_at, ok,
                    reason, command, exit_status, from_version, to_version, output)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    outcome.id,
                    outcome.engine,
                    kind_text(outcome.kind),
                    outcome.started_at,
                    outcome.finished_at,
                    outcome.ok,
                    outcome.reason,
                    outcome.command,
                    outcome.exit_status,
                    outcome.from_version,
                    outcome.to_version,
                    outcome.output.join("\n"),
                ],
            )?;
            tx.execute(
                "DELETE FROM engine_installs WHERE engine = ?1 AND rowid NOT IN (
                    SELECT rowid FROM engine_installs WHERE engine = ?1
                    ORDER BY finished_at DESC, rowid DESC LIMIT ?2)",
                params![outcome.engine, INSTALLS_KEPT as i64],
            )?;
            Ok(())
        })
    }

    /// An engine's kept outcomes, newest first.
    pub fn engine_installs(&self, engine: &str) -> Result<Vec<InstallOutcome>> {
        let reader = self.reader();
        let mut statement = reader.prepare(
            "SELECT * FROM engine_installs WHERE engine = ?1
             ORDER BY finished_at DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![engine, INSTALLS_KEPT as i64], outcome_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn open(dir: &TempDir) -> Store {
        Store::open(&dir.path().join("test.sqlite3"), "test").unwrap()
    }

    fn outcome(engine: &str, n: usize) -> InstallOutcome {
        InstallOutcome {
            id: format!("{engine}-{n}"),
            engine: engine.to_owned(),
            kind: InstallKind::Update,
            started_at: format!("2026-11-01T00:00:{n:02}Z"),
            finished_at: format!("2026-11-01T00:01:{n:02}Z"),
            ok: n.is_multiple_of(2),
            reason: (!n.is_multiple_of(2))
                .then(|| "The installer stopped with an error.".to_owned()),
            command: Some("npm install -g x@latest".into()),
            exit_status: Some(i32::try_from(n).unwrap()),
            from_version: Some("1.0.0".into()),
            to_version: None,
            output: vec!["one".into(), "two".into()],
        }
    }

    #[test]
    fn an_engine_keeps_its_newest_twenty_and_no_other_engine_loses_any() {
        let dir = TempDir::new();
        let store = open(&dir);
        for n in 0..25 {
            store.record_install(&outcome("codex", n)).unwrap();
        }
        store.record_install(&outcome("claude", 1)).unwrap();
        let kept = store.engine_installs("codex").unwrap();
        assert_eq!(kept.len(), INSTALLS_KEPT);
        assert_eq!(kept[0].id, "codex-24");
        assert_eq!(kept[INSTALLS_KEPT - 1].id, "codex-5");
        assert_eq!(store.engine_installs("claude").unwrap().len(), 1);
        assert!(store.engine_installs("other").unwrap().is_empty());
    }

    #[test]
    fn an_outcome_reads_back_as_written() {
        let dir = TempDir::new();
        let store = open(&dir);
        let written = outcome("codex", 3);
        store.record_install(&written).unwrap();
        assert_eq!(store.engine_installs("codex").unwrap(), vec![written]);
    }
}
