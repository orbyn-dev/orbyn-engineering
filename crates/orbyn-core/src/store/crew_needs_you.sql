-- Migration 77: the Crew "needs you" inbox's dismissals (KC3 PR1). A person may dismiss a stop or a task that could not start on
-- this device; the row remembers the item's `since`, so a later wait of the same kind and id shows again. Nothing here touches a
-- task, a merge or a background run, and the table holds ids and times only, never task content.
CREATE TABLE crew_needs_you_dismissed (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    since TEXT NOT NULL,
    dismissed_unix INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
);
