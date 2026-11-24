-- Migration 83: the Crew "needs you" inbox's notified items (KC10). A window that wants to show a desktop notification for an
-- item claims it first; the row remembers the item's `since`, so one wait is announced once however many windows are open or
-- restarts happen, and a later wait of the same kind and id (a new `since`) is announced again. The table holds ids and times
-- only, never task content, and nothing here touches a task, a merge or a background run.
CREATE TABLE crew_needs_you_notified (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    since TEXT NOT NULL,
    notified_unix INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
);
