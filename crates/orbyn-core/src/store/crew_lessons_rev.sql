-- Migration 84: KC5 slice 3, a person's view of one lesson. `rev` is the integer a person's write sends back so a change
-- made since they loaded it is refused instead of overwritten; it grows on every change a person can see (an edit, the
-- on/off switch, promote and demote, undo, and a cap hide), never on a use count or the soft delete itself. `deleted_at`
-- is the 10 s Undo: a deleted lesson is hidden from every read but this one until its window ends. A lesson purged after
-- that window leaves its fingerprint here, so the same failure, escape or revert is never written again. Device local,
-- never synced or exported; nothing here changes the router, the verifier, an answer key, a lease, a budget or a check.
ALTER TABLE crew_lessons ADD COLUMN rev INTEGER NOT NULL DEFAULT 1 CHECK(rev >= 1);
ALTER TABLE crew_lessons ADD COLUMN deleted_at TEXT;
CREATE TABLE crew_lessons_forgotten (
    fingerprint TEXT PRIMARY KEY CHECK(length(fingerprint)=64),
    forgotten_at TEXT NOT NULL
);
