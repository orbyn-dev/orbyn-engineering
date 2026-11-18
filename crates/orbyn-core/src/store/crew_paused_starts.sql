-- Migration 54: additive. A Low-priority start that Crew began itself and review capacity refused is kept as one bounded
-- row, so a person can see what is held. One row per trigger: the same trigger refused again updates it. Ids, counts and
-- times only: no task text and no engine output. The writer keeps at most 200 rows per project, oldest dropped.
CREATE TABLE crew_paused_starts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('comment_fix', 'review_fix', 'escape_check', 'background', 'unknown')),
    key TEXT NOT NULL CHECK(length(key) BETWEEN 1 AND 128),
    state TEXT NOT NULL CHECK(state IN ('held', 'started')),
    waiting INTEGER NOT NULL CHECK(waiting >= 0),
    limit_n INTEGER NOT NULL CHECK(limit_n >= 1),
    first_seen_unix INTEGER NOT NULL,
    last_seen_unix INTEGER NOT NULL,
    started_unix INTEGER,
    UNIQUE (project_id, kind, key)
);
CREATE INDEX crew_paused_starts_by_project ON crew_paused_starts (project_id, last_seen_unix DESC);
