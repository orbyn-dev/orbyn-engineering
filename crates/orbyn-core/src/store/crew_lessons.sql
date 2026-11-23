-- Migration 80: Crew lessons (KC5 slice 1). One short sentence the crew keeps about one repository (or all of them),
-- with where it came from. Device only and never synced: nothing here is exported, and the settings sync is an allowlist that
-- does not name this table. It is data in a brief, never a rule: no row here changes the router, the verifier, an answer key, a
-- lease, a budget or a check.
--
-- The scope is a repository (its project id) or global. The source is exactly one of the three the design note rules: a repeated
-- failure fingerprint, a confirmed escape (a revert is its own source), or a person's correction. Text is 1 to 300 characters.
-- `fingerprint` is the dedupe key: a named hash of what the lesson is about, unique when present, so the same failure or the
-- same escape never writes a second row. `evidence` is ids only, never content, as a JSON array of {kind, id}. At a cap (50 per
-- repository, 20 global) the oldest, least-used row becomes `hidden_by_cap` and stays readable; nothing is auto-deleted.
CREATE TABLE crew_lessons (
    id TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 96),
    scope TEXT NOT NULL CHECK(scope IN ('repo', 'global')),
    project_id TEXT,
    source TEXT NOT NULL CHECK(source IN ('fingerprint', 'escape', 'revert', 'person')),
    text TEXT NOT NULL CHECK(length(text) BETWEEN 1 AND 300),
    fingerprint TEXT CHECK(fingerprint IS NULL OR length(fingerprint) = 64),
    evidence TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(evidence)),
    state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active', 'disabled')),
    hidden_by_cap INTEGER NOT NULL DEFAULT 0 CHECK(hidden_by_cap IN (0, 1)),
    edited_by_person INTEGER NOT NULL DEFAULT 0 CHECK(edited_by_person IN (0, 1)),
    names_path INTEGER NOT NULL DEFAULT 0 CHECK(names_path IN (0, 1)),
    uses INTEGER NOT NULL DEFAULT 0 CHECK(uses >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_used_at TEXT,
    CHECK((scope = 'repo' AND project_id IS NOT NULL) OR (scope = 'global' AND project_id IS NULL))
);
CREATE UNIQUE INDEX crew_lessons_by_fingerprint ON crew_lessons(fingerprint) WHERE fingerprint IS NOT NULL;
CREATE INDEX crew_lessons_by_scope ON crew_lessons(scope, project_id, created_at DESC);
