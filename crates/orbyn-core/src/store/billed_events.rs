//! The billed events an engine's account reports, at rest. The engine host keeps no files, so the
//! account's own billing history belongs here rather than in a caller's memory.
//!
//! Nothing produces these events yet: the engine host answers `usage` with limits only and lists
//! Cursor's `accountUsage.ts` as not added, so this store and the read window below stay empty in
//! the app today. It is the contract that host read lands on, and its shape is T3's
//! (`accountUsage.ts`, MIT, commit 9168ca73d): `GetFilteredUsageEvents` answers
//! `usageEventsDisplay`, whose event holds `timestamp`, `model`, `conversationId` and `tokenUsage`
//! with `inputTokens` (the uncached input), `cacheReadTokens`, `cacheWriteTokens`, `outputTokens`
//! and `totalCents`.
//!
//! **One report is one complete read of the account's window.** `record_billed_events` replaces the
//! rows of the days that report names and leaves every other day alone, so the caller must hand
//! over a whole window (every page of it, as T3 reconciles against `totalUsageEventsCount`). That
//! is what makes the two directions of counting right at once: re-reading a window writes the same
//! rows again instead of adding more, and a window that holds two events is stored as two.
//!
//! **The window that was read is its own fact** (`billed_reads`, migration 91), because a read that
//! found no spend writes no event: the rows cannot say whether a day was looked at and came back
//! empty. `record_account_report` records the events and the window in one transaction, so a window
//! is never claimed without what the read reported. A day the window covers whole is what that day
//! held and its rows are replaced, including with nothing; a day it covers in part keeps its other
//! rows, because a part says nothing about the rest of that day. Windows that meet or overlap are
//! one window, since nothing between them was left unread; a window with a gap between it and the
//! stored one claims only itself. Which window to read next is the reader's question
//! (`crate::agents::cursor_usage::next_window`), not this store's.
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

use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::Store;
use crate::error::{CoreError, Result};
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

/// What the store knows about one engine's account read: the window it covers and when it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountRead {
    /// The oldest instant the read covered, inclusive.
    pub since_ms: u64,
    /// The newest instant it covered, inclusive.
    pub until_ms: u64,
    /// When the core read it, so a window the store still answers is not read again.
    pub fetched_at_ms: u64,
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

/// Milliseconds in one day, for the day arithmetic a read window needs.
const DAY_MS: u64 = 86_400_000;

/// The days a window covers whole, as its first and last day, or nothing when it holds no whole
/// day. A day the window covers in part is not here: a part says nothing about the rest of the day.
fn covered_days(since_ms: u64, until_ms: u64) -> Option<(String, String)> {
    // The index of the first day whose first millisecond the window holds.
    let first = since_ms.div_ceil(DAY_MS);
    // How many days have ended by `until_ms`, which is how many can be covered whole.
    let whole = until_ms.saturating_add(1) / DAY_MS;
    if first >= whole {
        return None;
    }
    Some((
        day_of_ms(first.saturating_mul(DAY_MS))?,
        day_of_ms(whole.saturating_sub(1).saturating_mul(DAY_MS))?,
    ))
}

/// The rows one engine already holds on the days from `first_day` to `last_day`, by event id, with
/// the billed amount each reported.
fn known_rows(
    tx: &Transaction<'_>,
    engine: &str,
    first_day: &str,
    last_day: &str,
) -> Result<HashMap<String, Option<i64>>> {
    let mut stored = tx.prepare_cached(
        "SELECT event_id, reported_cents FROM billed_events
         WHERE engine = ?1 AND day >= ?2 AND day <= ?3",
    )?;
    let rows = stored.query_map(params![engine, first_day, last_day], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
    })?;
    let mut known = HashMap::new();
    for row in rows {
        let (event_id, cents) = row?;
        known.insert(event_id, cents);
    }
    Ok(known)
}

/// Writes one report's events. The row of an event already stored is replaced, so an amount that
/// settled updates it rather than adding a second one.
fn upsert_events<'a>(
    tx: &Transaction<'_>,
    events: impl IntoIterator<Item = &'a BilledEvent>,
    recorded_at: &str,
) -> Result<()> {
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
    }
    Ok(())
}

/// How many of a report's events the store did not have, and how many it had with an amount that
/// has since settled.
fn count_recorded<'a>(
    known: &HashMap<String, Option<i64>>,
    events: impl IntoIterator<Item = &'a BilledEvent>,
) -> Recorded {
    let mut recorded = Recorded::default();
    for event in events {
        match known.get(&event.event_id) {
            None => recorded.new += 1,
            Some(previous) if *previous != event.reported_cents => recorded.settled += 1,
            Some(_) => {}
        }
    }
    recorded
}

/// Keeps the window the store says it has read. Two windows that meet or overlap are one window,
/// because between them nothing was left unread. A window with a gap between it and the stored one
/// claims only itself: the gap was never read, so it is not claimed.
fn merge_read(
    tx: &Transaction<'_>,
    engine: &str,
    since_ms: u64,
    until_ms: u64,
    fetched_at_ms: u64,
) -> Result<()> {
    let previous = tx
        .query_row(
            "SELECT since_ms, until_ms FROM billed_reads WHERE engine = ?1",
            [engine],
            |row| Ok((count_of(row.get(0)?), count_of(row.get(1)?))),
        )
        .optional()?;
    let (since_ms, until_ms) = match previous {
        Some((stored_since, stored_until))
            if since_ms <= stored_until.saturating_add(1)
                && stored_since <= until_ms.saturating_add(1) =>
        {
            (stored_since.min(since_ms), stored_until.max(until_ms))
        }
        _ => (since_ms, until_ms),
    };
    tx.execute(
        "INSERT INTO billed_reads (engine, since_ms, until_ms, fetched_at_ms)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (engine) DO UPDATE SET since_ms = excluded.since_ms,
             until_ms = excluded.until_ms, fetched_at_ms = excluded.fetched_at_ms",
        params![
            engine,
            sql_count(since_ms),
            sql_count(until_ms),
            sql_count(fetched_at_ms)
        ],
    )?;
    Ok(())
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
            let mut known: HashMap<String, Option<i64>> = HashMap::new();
            for engine in days
                .iter()
                .map(|(engine, _)| *engine)
                .collect::<BTreeSet<_>>()
            {
                let of_engine: Vec<&str> = days
                    .iter()
                    .filter(|(name, _)| *name == engine)
                    .map(|(_, day)| *day)
                    .collect();
                let (Some(first_day), Some(last_day)) = (of_engine.first(), of_engine.last())
                else {
                    continue;
                };
                for (event_id, cents) in known_rows(tx, engine, first_day, last_day)? {
                    known.entry(event_id).or_insert(cents);
                }
            }
            {
                let mut clear =
                    tx.prepare_cached("DELETE FROM billed_events WHERE engine = ?1 AND day = ?2")?;
                for (engine, day) in &days {
                    clear.execute(params![engine, day])?;
                }
            }
            upsert_events(tx, events, &recorded_at)?;
            Ok(count_recorded(&known, events))
        })
    }

    /// Records one read of the account's billed history over the window it covered, with the time
    /// it was read. This is one transaction, so the window is never claimed without the events the
    /// read reported.
    ///
    /// A day the window covers whole is what that day held, so its rows are replaced by what the
    /// read found — including nothing, which is how a day the account has stopped billing loses its
    /// rows. A day the window covers in part cannot say anything about the rest of that day, so it
    /// keeps its other rows and only what the read reported is written over. An amount that settled
    /// on an event already stored is counted as `settled`.
    ///
    /// `events` are this engine's; another engine's are not written here. A window that ends before
    /// it starts is refused rather than read as empty.
    pub fn record_account_report(
        &self,
        engine: &str,
        since_ms: u64,
        until_ms: u64,
        events: &[BilledEvent],
        fetched_at_ms: u64,
    ) -> Result<Recorded> {
        if since_ms > until_ms {
            return Err(CoreError::Invalid(
                "a read window ends before it starts".to_owned(),
            ));
        }
        let recorded_at = now();
        let ours: Vec<&BilledEvent> = events
            .iter()
            .filter(|event| event.engine == engine)
            .collect();
        let covered = covered_days(since_ms, until_ms);
        self.commit(|tx, _| {
            let mut known: HashMap<String, Option<i64>> = HashMap::new();
            if let Some((first_day, last_day)) = &covered {
                for (event_id, cents) in known_rows(tx, engine, first_day, last_day)? {
                    known.entry(event_id).or_insert(cents);
                }
                let mut stored = tx.prepare_cached(
                    "SELECT DISTINCT day FROM billed_events WHERE engine = ?1 AND day >= ?2 AND day <= ?3",
                )?;
                let days: Vec<String> = stored
                    .query_map(params![engine, first_day, last_day], |row| row.get(0))?
                    .collect::<std::result::Result<_, _>>()?;
                let mut clear =
                    tx.prepare_cached("DELETE FROM billed_events WHERE engine = ?1 AND day = ?2")?;
                for day in &days {
                    clear.execute(params![engine, day])?;
                }
            }
            if let (Some(first_day), Some(last_day)) = (
                ours.iter().map(|event| event.day.as_str()).min(),
                ours.iter().map(|event| event.day.as_str()).max(),
            ) {
                for (event_id, cents) in known_rows(tx, engine, first_day, last_day)? {
                    known.entry(event_id).or_insert(cents);
                }
            }
            upsert_events(tx, ours.iter().copied(), &recorded_at)?;
            merge_read(tx, engine, since_ms, until_ms, fetched_at_ms)?;
            Ok(count_recorded(&known, ours))
        })
    }

    /// The window of this engine's own billed history the core last read, and when it read it. A
    /// store that was never read answers `None`, which is a different state from a read that found
    /// nothing: the events alone cannot tell those apart.
    pub fn account_read(&self, engine: &str) -> Result<Option<AccountRead>> {
        let reader = self.reader();
        Ok(reader
            .query_row(
                "SELECT since_ms, until_ms, fetched_at_ms FROM billed_reads WHERE engine = ?1",
                [engine],
                |row| {
                    Ok(AccountRead {
                        since_ms: count_of(row.get(0)?),
                        until_ms: count_of(row.get(1)?),
                        fetched_at_ms: count_of(row.get(2)?),
                    })
                },
            )
            .optional()?)
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
