CREATE TABLE crew_forge_corrections (
    parent_task TEXT PRIMARY KEY REFERENCES crew_merges(task_id) ON DELETE CASCADE,
    task_id TEXT NOT NULL UNIQUE,
    body TEXT NOT NULL CHECK(json_valid(body)),
    completed_commit TEXT,
    -- Set with the person's Revert answer. Nothing may queue or publish a correction without it.
    person_requested_unix INTEGER,
    -- Set once the checkout and object reference were removed.
    retired_unix INTEGER
);
