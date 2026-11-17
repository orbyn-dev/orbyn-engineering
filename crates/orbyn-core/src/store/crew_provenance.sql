-- Migration 48: additive per-hunk provenance. One row per merged hunk (new-file line range) with who wrote it,
-- and one row per repository for the first time provenance was recorded, set once and never moved. Ids, ranges and
-- counts only: no file content, no prompt. A merge that is removed takes its hunks with it.
CREATE TABLE crew_hunks (
    merge_id INTEGER NOT NULL REFERENCES crew_merges(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    start_line INTEGER NOT NULL CHECK(start_line >= 1),
    line_count INTEGER NOT NULL CHECK(line_count >= 1),
    author_kind TEXT NOT NULL CHECK(author_kind IN ('agent', 'person')),
    engine TEXT,
    model TEXT,
    note_ref TEXT,
    PRIMARY KEY (merge_id, path, start_line)
);

CREATE TABLE crew_provenance_start (
    repo TEXT PRIMARY KEY,
    began_unix INTEGER NOT NULL
);
