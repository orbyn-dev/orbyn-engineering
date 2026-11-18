CREATE TABLE crew_forge_corrections (
    parent_task TEXT PRIMARY KEY REFERENCES crew_merges(task_id) ON DELETE CASCADE,
    task_id TEXT NOT NULL UNIQUE,
    body TEXT NOT NULL CHECK(json_valid(body)),
    completed_commit TEXT
);
