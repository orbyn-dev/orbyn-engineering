//! The billed events an engine's account reports, at rest. The engine host keeps no files, so
//! the account's own billing history is durable here: one row per event, keyed by a content
//! identity because Cursor sends no event id. A read that returns an event again writes nothing,
//! so one inference reported twice counts once. A record that cannot be read as a billed event
//! (no usage block, no timestamp or model that names a day, a negative counter) is dropped
//! rather than counted as zero, and a store with no rows reads as empty.
//!
//! The event shape follows T3's `packages/provider-cursor/src/server/accountUsage.ts` (MIT,
//! commit 9168ca73d): `GetFilteredUsageEvents` answers `usageEventsDisplay`, whose event holds
//! `timestamp`, `model`, `conversationId` and `tokenUsage` with `inputTokens` (uncached),
//! `cacheReadTokens`, `cacheWriteTokens`, `outputTokens` and `totalCents`. Its counters are
//! distinct rather than inclusive, so nothing here subtracts one from another.

use std::collections::HashMap;

use rusqlite::params;
use serde_json::Value;

use super::Store;
use crate::error::Result;
use crate::util::now;

/// One billed event, already read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BilledEvent {
    pub engine: String,
    /// The content identity, with an occurrence index for identical events.
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

/// A counter the account reported: a value that is not a non-negative number makes the record
/// malformed, and absent or empty counts as none.
fn counter(usage: &Value, key: &str) -> Option<u64> {
    match usage.get(key) {
        None | Some(Value::Null) => Some(0),
        Some(Value::Number(number)) => {
            let value = number.as_f64()?;
            (value.is_finite() && value >= 0.0).then(|| value.trunc() as u64)
        }
        // `tokens()` reads any non-number as no count; a wrong type is not a billed event.
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

/// The events in one usage report, in the order they arrived. The report's `usageEventsDisplay`
/// is the only shape read; any other report holds no billed event and answers empty.
pub fn parse_billed_events(engine: &str, report: &Value) -> Vec<BilledEvent> {
    let Some(events) = report.get("usageEventsDisplay").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut occurrences: HashMap<String, u64> = HashMap::new();
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
        // Cursor sends no event id, so two identical events are told apart by how many came
        // before. Re-reading the same page yields the same identities, which is what dedupes it.
        let identity = format!(
            "{stamp}|{model}|{session}|{input}|{output}|{cache_read}|{cache_write}|{reported_cents:?}"
        );
        let occurrence = occurrences.entry(identity.clone()).or_insert(0);
        let event_id = format!("{identity}|{occurrence}");
        *occurrence += 1;
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
    /// Writes the events that are new, in one transaction. An event already stored is skipped,
    /// and the count is what was new.
    pub fn record_billed_events(&self, events: &[BilledEvent]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let recorded_at = now();
        self.commit(|tx, _| {
            let mut written = 0;
            {
                let mut insert = tx.prepare_cached(
                    "INSERT OR IGNORE INTO billed_events (
                        engine, event_id, day, model, input_tokens, output_tokens,
                        cache_read_tokens, cache_write_tokens, reported_cents, recorded_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                for event in events {
                    written += insert.execute(params![
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
                }
            }
            Ok(written)
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
