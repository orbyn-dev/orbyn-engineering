-- Migration 73: additive (issue #911). The billed events an engine's account reports for its own
-- spend, kept so a history outlives one read. One row per billed event, keyed by a content
-- identity the engine's event carries no id for: an event re-reported by a later read counts once.
-- Counters are the account's own and are not derived here: `input_tokens` is the uncached input,
-- and cached input and cache writes are their own columns, as Cursor reports them.
-- `reported_cents` is the account's billed amount in cents, NULL when it reports none; a row with
-- neither tokens nor a billed amount is never written.
CREATE TABLE billed_events (
    engine TEXT NOT NULL,
    event_id TEXT NOT NULL,
    day TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    reported_cents INTEGER,
    recorded_at TEXT NOT NULL,
    PRIMARY KEY (engine, event_id)
) WITHOUT ROWID;

CREATE INDEX billed_events_by_day ON billed_events (engine, day);
