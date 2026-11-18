-- Migration 54: the positive person signal for merge provenance (slice 4l). `crew_person_commits` lists the full
-- commit ids the app itself made on a person's action, written in the same transaction as the audit row. Nothing is
-- inferred from an author name or email. `crew_hunks` gains the author kind `unknown` (a line nobody confirmed:
-- coverage counts it with the agent's lines), which a CHECK change needs a rebuild for; existing rows are copied as they
-- are, so old merges read the same as before. Ids, ranges and counts only: no file content, no prompt.
CREATE TABLE crew_hunks_next (
    merge_id INTEGER NOT NULL REFERENCES crew_merges(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    start_line INTEGER NOT NULL CHECK(start_line >= 1),
    line_count INTEGER NOT NULL CHECK(line_count >= 1),
    author_kind TEXT NOT NULL CHECK(author_kind IN ('agent', 'person', 'unknown')),
    engine TEXT,
    model TEXT,
    note_ref TEXT,
    PRIMARY KEY (merge_id, path, start_line)
);
INSERT INTO crew_hunks_next (merge_id, path, start_line, line_count, author_kind, engine, model, note_ref)
    SELECT merge_id, path, start_line, line_count, author_kind, engine, model, note_ref FROM crew_hunks;
DROP TABLE crew_hunks;
ALTER TABLE crew_hunks_next RENAME TO crew_hunks;

CREATE TABLE crew_person_commits (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    commit_id TEXT NOT NULL CHECK(length(commit_id) IN (40, 64) AND commit_id NOT GLOB '*[^0-9a-f]*'),
    at_unix INTEGER NOT NULL,
    action TEXT NOT NULL CHECK(length(action) BETWEEN 1 AND 64),
    PRIMARY KEY (project_id, commit_id)
);
