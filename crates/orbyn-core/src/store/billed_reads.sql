-- Migration 91: the window of its own billed history an engine's account was last read over
-- (issue #911; docs/architecture.md, "Usage"). A read that found no spend writes no row to
-- `billed_events`, so the events alone cannot say whether a day was looked at and came back
-- empty. This row says which window the core read and when, so a window the store already
-- answers is not read again, and history older than the window is topped up rather than guessed.
-- Milliseconds are the account's own clock, both bounds inclusive. No credential, account id or
-- token is stored here: one row per engine, as with the events.
CREATE TABLE billed_reads (
    engine TEXT PRIMARY KEY,
    since_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL,
    fetched_at_ms INTEGER NOT NULL
) WITHOUT ROWID;
