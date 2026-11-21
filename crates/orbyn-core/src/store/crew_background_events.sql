-- Migration 59: additive. Numbered after the escape retention's 58. One row per background event
-- that has fired, keyed by the event's identity
-- (crew, and the head commit with the failing check's name for a CI failure, or the schedule bucket). The row is the
-- dedupe: a repeat of the same event starts nothing new, and a firing that was refused keeps its row, so nothing is
-- retried on a timer. Ids, keys and times only: no check output, no repository text beyond a check name, and none of it
-- ever reaches a prompt.
CREATE TABLE crew_background_events (
    identity TEXT PRIMARY KEY,
    claimed_unix INTEGER NOT NULL
);
