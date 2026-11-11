//! Durable state of the account's settings sync: the local copy of each zone's record
//! with its stamps, the outbox of fields not yet uploaded, the zone cursor and the
//! conflicts to show. Non-secret by rule: no key, token or password is stored here.
//! The SQLite database stays the source of truth for settings; a preference write
//! appends its outbox rows in the same transaction (`change_settings`), so a crash
//! loses neither.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

use super::Store;
use crate::account::merge::{Field, Hlc, Record, Stamp, merge_records};
use crate::account::settings_record::{MAX_RECORD_BYTES, Staged};
use crate::error::{CoreError, Result};

/// A lost offline edit: the setting was changed on two devices while both were
/// offline. The newer change won; the older value is kept here to offer back.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncConflict {
    pub field: String,
    pub kept: Value,
    pub lost: Value,
    pub at_ms: i64,
}

/// What one merge of a remote record did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub conflicts: usize,
    pub waiting: usize,
}

fn text(value: &Value) -> String {
    value.to_string()
}

fn field_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, Field)> {
    let value: String = row.get(1)?;
    Ok((
        row.get(0)?,
        Field {
            value: serde_json::from_str(&value).unwrap_or(Value::Null),
            stamp: Stamp {
                wall_ms: row.get::<_, i64>(2)?,
                counter: u32::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                device: row.get(4)?,
            },
        },
    ))
}

pub(super) fn load_record(conn: &Connection, zone: &str) -> Result<Record> {
    let rows = conn
        .prepare("SELECT field, value, wall, counter, device FROM account_fields WHERE zone = ?1")?
        .query_map([zone], field_row)?
        .collect::<std::result::Result<Record, _>>()?;
    Ok(rows)
}

/// The zone's device id and clock, when capture is on.
fn clock(conn: &Connection, zone: &str) -> Result<Option<(String, Hlc, bool)>> {
    Ok(conn
        .query_row(
            "SELECT device, hlc_wall, hlc_counter, capture FROM account_sync WHERE zone = ?1",
            [zone],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    Hlc::new(
                        row.get::<_, i64>(1)?,
                        u32::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    ),
                    row.get::<_, i64>(3)? != 0,
                ))
            },
        )
        .optional()?)
}

fn save_clock(conn: &Connection, zone: &str, hlc: Hlc) -> Result<()> {
    conn.execute(
        "UPDATE account_sync SET hlc_wall = ?2, hlc_counter = ?3 WHERE zone = ?1",
        params![zone, hlc.wall_ms, i64::from(hlc.counter)],
    )?;
    Ok(())
}

/// Queues `field`. `base` is the stamp it replaced; a field edited again before it
/// uploads keeps the first one, since that is what the other devices may have seen.
fn queue_field(
    tx: &Connection,
    zone: &str,
    name: &str,
    field: &Field,
    at_ms: i64,
    base: Option<&Stamp>,
) -> Result<()> {
    let wall = field.stamp.wall_ms;
    tx.execute(
        "INSERT INTO account_outbox (zone, field, value, wall, counter, device, edited_at, base_wall, base_counter, base_device)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (zone, field) DO UPDATE SET value = excluded.value, wall = excluded.wall,
            counter = excluded.counter, device = excluded.device, edited_at = excluded.edited_at",
        params![
            zone, name, text(&field.value), wall, i64::from(field.stamp.counter), field.stamp.device, at_ms,
            base.map(|base| base.wall_ms),
            base.map(|base| i64::from(base.counter)),
            base.map(|base| base.device.clone()),
        ],
    )?;
    Ok(())
}

/// Stores a field of the local record. A local edit is already in the settings, so it
/// counts as applied; a field that came from the account is not until it is applied.
fn put_field(tx: &Connection, zone: &str, name: &str, field: &Field, applied: bool) -> Result<()> {
    tx.execute(
        "INSERT INTO account_fields (zone, field, value, wall, counter, device, applied)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (zone, field) DO UPDATE SET value = excluded.value, wall = excluded.wall,
            counter = excluded.counter, device = excluded.device, applied = excluded.applied",
        params![
            zone,
            name,
            text(&field.value),
            field.stamp.wall_ms,
            i64::from(field.stamp.counter),
            field.stamp.device,
            applied
        ],
    )?;
    Ok(())
}

/// The fields from the account this device has not applied yet and may: a stamp later
/// than `max_wall` is held back. Keybinding fields wait for their file.
pub(super) fn load_unapplied(conn: &Connection, zone: &str, max_wall: i64) -> Result<Record> {
    let rows = conn
        .prepare(
            "SELECT field, value, wall, counter, device FROM account_fields
             WHERE zone = ?1 AND applied = 0 AND wall <= ?2",
        )?
        .query_map(params![zone, max_wall], field_row)?
        .collect::<std::result::Result<Record, _>>()?;
    Ok(rows)
}

/// Marks applied every unapplied field that is not a keybinding and is not held back.
pub(super) fn mark_applied(tx: &Connection, zone: &str, max_wall: i64) -> Result<()> {
    tx.execute(
        "UPDATE account_fields SET applied = 1
         WHERE zone = ?1 AND applied = 0 AND wall <= ?2 AND field NOT GLOB 'keybindings:*'",
        params![zone, max_wall],
    )?;
    Ok(())
}

/// A removal marker of a set element older than the horizon.
fn is_old_tombstone(name: &str, field: &Field, now_ms: i64) -> bool {
    crate::account::settings_record::set_key_of(name).is_some()
        && field.value.get("removed") == Some(&Value::Bool(true))
        && field.stamp.wall_ms < now_ms - crate::account::settings_record::TOMBSTONE_HORIZON_MS
}

fn skipped_names(conn: &Connection, zone: &str) -> Result<std::collections::BTreeSet<String>> {
    let names = conn
        .prepare("SELECT field FROM account_skipped WHERE zone = ?1")?
        .query_map([zone], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(names)
}

fn field_size(name: &str, field: &Field) -> usize {
    name.len() + field.value.to_string().len()
}

/// Stages `staged` as local edits made at `now_ms`, when capture is on for the zone.
/// Returns how many rows changed, so the caller can wake the worker.
pub(super) fn capture(
    tx: &Transaction<'_>,
    zone: &str,
    staged: &[Staged],
    now_ms: i64,
    seed: bool,
    seed_wins: bool,
) -> Result<usize> {
    let Some((device, mut hlc, true)) = clock(tx, zone)? else {
        return Ok(0);
    };
    let local = load_record(tx, zone)?;
    let mut size: usize = local
        .iter()
        .map(|(name, field)| field_size(name, field))
        .sum();
    let mut touched = 0;
    for item in staged {
        match item {
            Staged::Field { name, value } => {
                tx.execute(
                    "DELETE FROM account_skipped WHERE zone = ?1 AND field = ?2",
                    params![zone, name],
                )?;
                if local.get(name).is_some_and(|known| &known.value == value) {
                    continue;
                }
                // What the person had before the account was on is older than anything
                // the account holds, so a first sync never beats a newer account value.
                // The one exception is "Use this device's settings": then they are as new as an
                // edit made now, and win everywhere.
                let (wall_ms, counter) = if seed && !seed_wins {
                    (0, 0)
                } else {
                    hlc.tick_at(now_ms)
                };
                let field = Field {
                    value: value.clone(),
                    stamp: Stamp {
                        wall_ms,
                        counter,
                        device: device.clone(),
                    },
                };
                // A change that would take the record over its limit stays local.
                let freed = local.get(name).map_or(0, |known| field_size(name, known));
                if size.saturating_sub(freed) + field_size(name, &field) > MAX_RECORD_BYTES {
                    tx.execute(
                        "INSERT OR IGNORE INTO account_skipped (zone, field) VALUES (?1, ?2)",
                        params![zone, name],
                    )?;
                    touched += 1;
                    continue;
                }
                size = size.saturating_sub(freed) + field_size(name, &field);
                // A first-sync value combines with the account's rather than conflicts with
                // it: its base has no device, which the merge reads as "not concurrent".
                let first_sync = Stamp {
                    wall_ms: 0,
                    counter: 0,
                    device: String::new(),
                };
                let base = if seed {
                    Some(&first_sync)
                } else {
                    local.get(name).map(|known| &known.stamp)
                };
                put_field(tx, zone, name, &field, true)?;
                queue_field(tx, zone, name, &field, now_ms, base)?;
                touched += 1;
            }
            Staged::Skipped { name } => {
                tx.execute(
                    "INSERT OR IGNORE INTO account_skipped (zone, field) VALUES (?1, ?2)",
                    params![zone, name],
                )?;
                touched += 1;
            }
        }
    }
    save_clock(tx, zone, hlc)?;
    Ok(touched)
}

impl Store {
    /// Wakes the zone's worker, as a queued change does.
    pub(crate) fn account_poke(&self) {
        self.account_changes.send_modify(|count| *count += 1);
    }

    /// Deletes the changes that have not synced, when the person signs out and asks for it.
    pub(crate) fn account_discard_outbox(&self, zone: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("DELETE FROM account_outbox WHERE zone = ?1", [zone])?;
            Ok(())
        })
    }

    /// Wakes on any local change that queued a field.
    pub fn subscribe_account_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.account_changes.subscribe()
    }

    /// Turns capture on for `zone`. The first time, `seed` stages what the person has
    /// set so far, so a first sync combines it with what the account holds.
    pub(crate) fn account_enable(
        &self,
        zone: &str,
        seed: &[Staged],
        seed_wins: bool,
    ) -> Result<()> {
        let touched = self.commit(|tx, _| {
            let known = clock(tx, zone)?;
            if known.is_some() {
                tx.execute(
                    "UPDATE account_sync SET capture = 1 WHERE zone = ?1",
                    [zone],
                )?;
                return Ok(0);
            }
            tx.execute(
                "INSERT INTO account_sync (zone, capture, device) VALUES (?1, 1, ?2)",
                params![zone, crate::util::new_id()],
            )?;
            capture(tx, zone, seed, crate::util::unix_now_ms(), true, seed_wins)
        })?;
        if touched > 0 {
            self.account_changes.send_modify(|count| *count += 1);
        }
        Ok(())
    }

    /// Whether this data folder has joined the zone before, so a sign-in does not ask again.
    pub(crate) fn account_known(&self, zone: &str) -> Result<bool> {
        Ok(clock(&self.reader(), zone)?.is_some())
    }

    /// How many settings the zone's local record holds.
    pub(crate) fn account_field_count(&self, zone: &str) -> Result<usize> {
        Ok(load_record(&self.reader(), zone)?.len())
    }

    /// Queues fields outside a settings write, such as a keybinding saved to its file.
    pub(crate) fn account_stage(&self, zone: &str, staged: &[Staged]) -> Result<()> {
        let touched = self
            .commit(|tx, _| capture(tx, zone, staged, crate::util::unix_now_ms(), false, false))?;
        if touched > 0 {
            self.account_changes.send_modify(|count| *count += 1);
        }
        Ok(())
    }

    /// The local copy of the zone's record: every field, known or not.
    pub(crate) fn account_record(&self, zone: &str) -> Result<Record> {
        load_record(&self.reader(), zone)
    }

    /// Marks keybinding fields applied, once their file has taken them.
    pub(crate) fn account_mark_applied(&self, zone: &str, names: &[String]) -> Result<()> {
        self.commit(|tx, _| {
            for name in names {
                tx.execute(
                    "UPDATE account_fields SET applied = 1 WHERE zone = ?1 AND field = ?2",
                    params![zone, name],
                )?;
            }
            Ok(())
        })
    }

    /// Fields from the account stamped later than the drift limit: kept and passed
    /// on, never applied.
    pub(crate) fn account_held_back_len(&self, zone: &str) -> Result<usize> {
        let cutoff = crate::util::unix_now_ms() + crate::account::merge::MAX_DRIFT_MS;
        let n: i64 = self.reader().query_row(
            "SELECT COUNT(*) FROM account_fields WHERE zone = ?1 AND applied = 0 AND wall > ?2",
            params![zone, cutoff],
            |row| row.get(0),
        )?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Takes a field out of the record and the outbox and keeps its setting local: the
    /// upload would be over the record limit.
    pub(crate) fn account_keep_local(&self, zone: &str, name: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM account_fields WHERE zone = ?1 AND field = ?2",
                params![zone, name],
            )?;
            tx.execute(
                "DELETE FROM account_outbox WHERE zone = ?1 AND field = ?2",
                params![zone, name],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO account_skipped (zone, field) VALUES (?1, ?2)",
                params![zone, name],
            )?;
            Ok(())
        })
    }

    /// Drops removal markers of set elements older than the horizon, which are not
    /// waiting to upload, so a record does not grow for ever.
    pub(crate) fn account_compact(&self, zone: &str, now_ms: i64) -> Result<usize> {
        self.commit(|tx, _| {
            let queued = pending_edits(tx, zone)?;
            let old: Vec<String> = load_record(tx, zone)?
                .into_iter()
                .filter(|(name, field)| {
                    is_old_tombstone(name, field, now_ms) && !queued.contains_key(name)
                })
                .map(|(name, _)| name)
                .collect();
            for name in &old {
                tx.execute(
                    "DELETE FROM account_fields WHERE zone = ?1 AND field = ?2",
                    params![zone, name],
                )?;
            }
            Ok(old.len())
        })
    }

    /// The fields waiting to upload.
    pub(crate) fn account_outbox_names(&self, zone: &str) -> Result<Vec<String>> {
        Ok(pending_edits(&self.reader(), zone)?.into_keys().collect())
    }

    pub(crate) fn account_outbox_len(&self, zone: &str) -> Result<usize> {
        count(&self.reader(), "account_outbox", zone)
    }

    pub(crate) fn account_skipped_len(&self, zone: &str) -> Result<usize> {
        count(&self.reader(), "account_skipped", zone)
    }

    pub(crate) fn account_version(&self, zone: &str) -> Result<Option<u64>> {
        let version: Option<i64> = self
            .reader()
            .query_row(
                "SELECT version FROM account_sync WHERE zone = ?1",
                [zone],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(version.and_then(|version| u64::try_from(version).ok()))
    }

    /// Merges the zone's record as the server holds it into the local copy, in one
    /// transaction. Afterwards the outbox holds exactly the fields the server lacks or
    /// holds an older stamp for. A queued edit meets a conflict when another device
    /// changed the same field since the stamp the edit replaced: both changed while the
    /// devices were apart, and the older one lost.
    pub(crate) fn account_merge(
        &self,
        zone: &str,
        remote: &Record,
        version: u64,
    ) -> Result<MergeReport> {
        self.commit(|tx, _| {
            let (device, mut hlc, _) = clock(tx, zone)?.ok_or_else(|| CoreError::Internal("account sync is not set up".into()))?;
            let now_ms = crate::util::unix_now_ms();
            let local = load_record(tx, zone)?;
            let pending = pending_edits(tx, zone)?;
            // A stamp far ahead of this clock never beats an edit waiting here, or that
            // edit would be thrown away at every merge: the edit uploads instead.
            let cutoff = now_ms + crate::account::merge::MAX_DRIFT_MS;
            let mut remote = remote.clone();
            remote.retain(|name, field| !(field.stamp.wall_ms > cutoff && pending.contains_key(name)));
            let remote = &remote;
            let merged = merge_records(&local, remote);
            let skipped = skipped_names(tx, zone)?;
            for field in remote.values() {
                hlc.observe(field.stamp.wall_ms, field.stamp.counter, now_ms);
            }
            for name in &merged.remote_won {
                // A setting kept local (its upload would not fit) is never overwritten by
                // the older value the account still holds: that value is kept to pass on,
                // not applied.
                put_field(tx, zone, name, &merged.record[name], skipped.contains(name))?;
            }
            let mut conflicts = 0;
            for (name, (queued, base)) in &pending {
                let Some(theirs) = remote.get(name) else { continue };
                let concurrent = theirs.stamp.device != device
                    && base.as_ref().is_none_or(|base| !base.device.is_empty())
                    && base.as_ref() != Some(&theirs.stamp)
                    && theirs.value != *queued;
                if concurrent {
                    let (kept, lost) = if merged.remote_won.contains(name) {
                        (&theirs.value, queued)
                    } else {
                        (queued, &theirs.value)
                    };
                    tx.execute(
                        "INSERT INTO account_conflicts (zone, field, kept, lost, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![zone, name, text(kept), text(lost), now_ms],
                    )?;
                    conflicts += 1;
                }
            }
            for name in pending.keys().filter(|name| !merged.local_ahead.contains(*name)) {
                tx.execute("DELETE FROM account_outbox WHERE zone = ?1 AND field = ?2", params![zone, name])?;
            }
            for name in merged.local_ahead.iter().filter(|name| !pending.contains_key(*name)) {
                let field = &merged.record[name];
                if is_old_tombstone(name, field, now_ms) {
                    // Another device compacted this marker away; dropping it here is how
                    // the horizon holds across devices, rather than uploading it again.
                    tx.execute("DELETE FROM account_fields WHERE zone = ?1 AND field = ?2", params![zone, name])?;
                } else if !skipped.contains(name) {
                    queue_field(tx, zone, name, field, now_ms, None)?;
                }
            }
            save_clock(tx, zone, hlc)?;
            tx.execute(
                "UPDATE account_sync SET version = ?2 WHERE zone = ?1",
                params![zone, i64::try_from(version).unwrap_or(i64::MAX)],
            )?;
            Ok(MergeReport { conflicts, waiting: count(tx, "account_outbox", zone)? })
        })
    }

    /// Records an accepted upload: the outbox rows for exactly the stamps sent are
    /// done, and a field edited again while it was in flight stays queued.
    pub(crate) fn account_pushed(&self, zone: &str, sent: &Record, version: u64) -> Result<usize> {
        self.commit(|tx, _| {
            for (name, field) in sent {
                tx.execute(
                    "DELETE FROM account_outbox WHERE zone = ?1 AND field = ?2 AND wall = ?3 AND counter = ?4 AND device = ?5",
                    params![zone, name, field.stamp.wall_ms, i64::from(field.stamp.counter), field.stamp.device],
                )?;
            }
            tx.execute(
                "UPDATE account_sync SET version = ?2 WHERE zone = ?1",
                params![zone, i64::try_from(version).unwrap_or(i64::MAX)],
            )?;
            count(tx, "account_outbox", zone)
        })
    }

    pub(crate) fn account_conflicts(&self, zone: &str) -> Result<Vec<SyncConflict>> {
        let rows = self
            .reader()
            .prepare("SELECT field, kept, lost, at_ms FROM account_conflicts WHERE zone = ?1 ORDER BY id")?
            .query_map([zone], |row| {
                let value = |index: usize| -> rusqlite::Result<Value> {
                    Ok(serde_json::from_str(&row.get::<_, String>(index)?).unwrap_or(Value::Null))
                };
                Ok(SyncConflict { field: row.get(0)?, kept: value(1)?, lost: value(2)?, at_ms: row.get(3)? })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn account_clear_conflicts(&self, zone: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("DELETE FROM account_conflicts WHERE zone = ?1", [zone])?;
            Ok(())
        })
    }
}

/// The queued edits: each field's value and the stamp it replaced.
fn pending_edits(
    conn: &Connection,
    zone: &str,
) -> Result<BTreeMap<String, (Value, Option<Stamp>)>> {
    let rows = conn
        .prepare("SELECT field, value, base_wall, base_counter, base_device FROM account_outbox WHERE zone = ?1")?
        .query_map([zone], |row| {
            let base = match (row.get::<_, Option<i64>>(2)?, row.get::<_, Option<i64>>(3)?, row.get::<_, Option<String>>(4)?) {
                (Some(wall), Some(counter), Some(device)) => Some(Stamp {
                    wall_ms: wall,
                    counter: u32::try_from(counter).unwrap_or(0),
                    device,
                }),
                _ => None,
            };
            let value: String = row.get(1)?;
            Ok((row.get::<_, String>(0)?, (serde_json::from_str(&value).unwrap_or(Value::Null), base)))
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

fn count(conn: &Connection, table: &str, zone: &str) -> Result<usize> {
    let table = match table {
        "account_outbox" | "account_skipped" | "account_conflicts" => table,
        _ => return Err(CoreError::Internal("unknown account table".into())),
    };
    let n: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE zone = ?1"),
        [zone],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(0))
}
