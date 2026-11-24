//! The billed events an engine's account reports, at rest. The engine host keeps no files, so the
//! account's own billing history belongs here rather than in a caller's memory.
//!
//! Nothing produces these events yet: the engine host answers `usage` with limits only and lists
//! Cursor's `accountUsage.ts` as not added, so this store stays empty in the app today. It is the
//! contract that host read lands on, and its shape is T3's (`accountUsage.ts`, MIT, commit
//! 9168ca73d): `GetFilteredUsageEvents` answers `usageEventsDisplay`, whose event holds
//! `timestamp`, `model`, `conversationId` and `tokenUsage` with `inputTokens` (the uncached input),
//! `cacheReadTokens`, `cacheWriteTokens`, `outputTokens` and `totalCents`.
//!
//! **One report is one complete read of the account's window.** `record_billed_events` replaces the
//! rows of the days that report names and leaves every other day alone, so the caller must hand
//! over a whole window (every page of it, as T3 reconciles against `totalUsageEventsCount`). That
//! is what makes the two directions of counting right at once: re-reading a window writes the same
//! rows again instead of adding more, and a window that holds two events is stored as two.
//!
//! Cursor sends no event id. A row is keyed by a digest of the fields that identify the event —
//! the engine, the conversation, the timestamp, the model and the four counters — plus how many
//! identical events came before it in the same report. The digest means a session identifier never
//! reaches the disk in cleartext. The key holds nothing that can settle later: an amount is not
//! part of it, so a report with the billed cents settled updates that row rather than adding a
//! second one.
//!
//! Cursor's counters are distinct rather than inclusive, so nothing is subtracted from anything.
//! Reasoning is not a column: Cursor bills it inside `outputTokens` and reports no separate
//! counter (T3 hardcodes zero), so a `reasoningTokens` field, if one ever appears, is not added on
//! top of the output it is already inside. A record that cannot be read as a billed event (no
//! usage block, no timestamp or model that names a day, a negative counter) is dropped rather than
//! counted as zero, and a store with no rows reads as empty.

use std::collections::{BTreeSet, HashMap};

use rusqlite::params;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::Store;
use crate::error::Result;
use crate::util::now;

/// One billed event, already read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BilledEvent {
    pub engine: String,
    /// The digest of what identifies this event, with its place among identical events.
    pub event_id: String,
    pub day: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// The account's billed amount in cents, when it reports one.
    pub reported_cents: Option<i64>,
}

/// One day and model's billed totals, summed over the events in the period.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BilledTotal {
    pub day: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// The account's billed amount in cents, when any event reported one.
    pub reported_cents: Option<i64>,
}

/// What recording one report did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Recorded {
    /// Events the store did not have.
    pub new: usize,
    /// Events it had, whose billed amount the account has since settled.
    pub settled: usize,
}

/// A counter the account reported: a value that is not a non-negative number makes the record
/// malformed, and an absent one counts as none.
fn counter(usage: &Value, key: &str) -> Option<u64> {
    match usage.get(key) {
        None | Some(Value::Null) => Some(0),
        Some(Value::Number(number)) => {
            let value = number.as_f64()?;
            (value.is_finite() && value >= 0.0).then(|| value.trunc() as u64)
        }
        // A wrong type is not a billed event, rather than a zero.
        Some(_) => None,
    }
}

/// Cursor sends milliseconds, as a number or as a string of them.
fn timestamp_ms(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let value = text.parse::<f64>().ok()?;
            (value.is_finite() && value >= 0.0).then(|| value.trunc() as u64)
        }
        _ => None,
    }
}

/// A timestamp that names no day is dropped rather than dated by guess.
fn day_of_ms(ms: u64) -> Option<String> {
    let nanos = i128::from(ms).checked_mul(1_000_000)?;
    let at = time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?;
    Some(at.date().to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The digest of what identifies one event. Every text field is length-prefixed so that no two
/// different events hash the same, and no field that can settle later is included.
fn identity(engine: &str, stamp: u64, model: &str, session: &str, counters: [u64; 4]) -> String {
    let mut hasher = Sha256::new();
    for field in [engine, model, session] {
        hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    for value in std::iter::once(stamp).chain(counters) {
        hasher.update(value.to_le_bytes());
    }
    hex(hasher.finalize().as_slice())
}

/// The events in one usage report, in the order they arrived. The report's `usageEventsDisplay` is
/// the only shape read; any other report holds no billed event and answers empty. Events identical
/// in every identifying field are kept apart by their place among the identical ones, which is
/// stable because the report is read whole.
pub fn parse_billed_events(engine: &str, report: &Value) -> Vec<BilledEvent> {
    let Some(events) = report.get("usageEventsDisplay").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut identical: HashMap<String, u64> = HashMap::new();
    let mut parsed = Vec::new();
    for raw in events {
        let Some(event) = raw.as_object() else {
            continue;
        };
        // No usage block is a non-token event: it proves no spend and is not a billed event.
        let Some(usage) = event.get("tokenUsage").filter(|usage| !usage.is_null()) else {
            continue;
        };
        let Some(stamp) = timestamp_ms(event.get("timestamp")) else {
            continue;
        };
        let Some(day) = day_of_ms(stamp) else {
            continue;
        };
        let Some(model) = event
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
        else {
            continue;
        };
        let (Some(input), Some(output), Some(cache_read), Some(cache_write)) = (
            counter(usage, "inputTokens"),
            counter(usage, "outputTokens"),
            counter(usage, "cacheReadTokens"),
            counter(usage, "cacheWriteTokens"),
        ) else {
            continue;
        };
        let reported_cents = match usage.get("totalCents") {
            None | Some(Value::Null) => None,
            Some(Value::Number(number)) => match number.as_f64() {
                Some(cents) if cents.is_finite() && cents >= 0.0 => Some(cents.round() as i64),
                _ => continue,
            },
            Some(_) => continue,
        };
        // An event with no tokens and no billed amount proves no spend.
        let billed = reported_cents.is_some_and(|cents| cents > 0);
        if input + output + cache_read + cache_write == 0 && !billed {
            continue;
        }
        let session = event
            .get("conversationId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let digest = identity(
            engine,
            stamp,
            model,
            session,
            [input, output, cache_read, cache_write],
        );
        let ordinal = identical.entry(digest.clone()).or_insert(0);
        let event_id = format!("{digest}:{ordinal}");
        *ordinal += 1;
        parsed.push(BilledEvent {
            engine: engine.to_owned(),
            event_id,
            day,
            model: model.to_owned(),
            input,
            output,
            cache_read,
            cache_write,
            reported_cents,
        });
    }
    parsed
}

/// SQLite integers are signed, so a counter is written at most at `i64::MAX`.
fn sql_count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// A count read back is never negative.
fn count_of(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

impl Store {
    /// Records one report of the account's billed events. The rows of the days the report names are
    /// replaced by it, so a window read twice holds the same rows, and a window that holds two
    /// identical events holds two rows. Days the report does not mention are left as they were.
    /// A billed amount that settled on an event already stored is counted as `settled`.
    pub fn record_billed_events(&self, events: &[BilledEvent]) -> Result<Recorded> {
        if events.is_empty() {
            return Ok(Recorded::default());
        }
        let recorded_at = now();
        let days: BTreeSet<(&str, &str)> = events
            .iter()
            .map(|event| (event.engine.as_str(), event.day.as_str()))
            .collect();
        self.commit(|tx, _| {
            let mut recorded = Recorded::default();
            {
                let mut stored = tx.prepare_cached(
                    "SELECT event_id, reported_cents FROM billed_events
                     WHERE engine = ?1 AND day = ?2",
                )?;
                let mut known: HashMap<String, Option<i64>> = HashMap::new();
                for (engine, day) in &days {
                    let rows = stored.query_map(params![engine, day], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
                    })?;
                    for row in rows {
                        let (event_id, cents) = row?;
                        known.insert(event_id, cents);
                    }
                }
                {
                    let mut clear = tx.prepare_cached(
                        "DELETE FROM billed_events WHERE engine = ?1 AND day = ?2",
                    )?;
                    for (engine, day) in &days {
                        clear.execute(params![engine, day])?;
                    }
                }
                let mut insert = tx.prepare_cached(
                    "INSERT OR REPLACE INTO billed_events (
                        engine, event_id, day, model, input_tokens, output_tokens,
                        cache_read_tokens, cache_write_tokens, reported_cents, recorded_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                for event in events {
                    insert.execute(params![
                        event.engine,
                        event.event_id,
                        event.day,
                        event.model,
                        sql_count(event.input),
                        sql_count(event.output),
                        sql_count(event.cache_read),
                        sql_count(event.cache_write),
                        event.reported_cents,
                        recorded_at,
                    ])?;
                    match known.get(&event.event_id) {
                        None => recorded.new += 1,
                        Some(previous) if *previous != event.reported_cents => {
                            recorded.settled += 1
                        }
                        Some(_) => {}
                    }
                }
            }
            Ok(recorded)
        })
    }

    /// The billed totals per day and model for one engine, from `since` on. A store with no rows
    /// answers empty, which is a history of nothing rather than an error.
    pub fn billed_totals(&self, engine: &str, since: Option<&str>) -> Result<Vec<BilledTotal>> {
        let reader = self.reader();
        let mut statement = reader.prepare_cached(
            "SELECT day, model, SUM(input_tokens), SUM(output_tokens),
                    SUM(cache_read_tokens), SUM(cache_write_tokens), SUM(reported_cents)
             FROM billed_events
             WHERE engine = ?1 AND (?2 IS NULL OR day >= ?2)
             GROUP BY day, model
             ORDER BY day, model",
        )?;
        let rows = statement.query_map(params![engine, since], |row| {
            Ok(BilledTotal {
                day: row.get(0)?,
                model: row.get(1)?,
                input: count_of(row.get(2)?),
                output: count_of(row.get(3)?),
                cache_read: count_of(row.get(4)?),
                cache_write: count_of(row.get(5)?),
                reported_cents: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// How many events are stored for one engine, which tells a scanned history from no history.
    pub fn billed_event_count(&self, engine: &str) -> Result<u64> {
        let reader = self.reader();
        let count: i64 = reader.query_row(
            "SELECT COUNT(*) FROM billed_events WHERE engine = ?1",
            [engine],
            |row| row.get(0),
        )?;
        Ok(count_of(count))
    }
}
