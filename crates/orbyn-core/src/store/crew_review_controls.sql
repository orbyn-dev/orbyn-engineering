-- Migration 46: additive person review controls and device-local ACP assignments.
CREATE TABLE crew_review_controls (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    lane TEXT NOT NULL,
    commit_id TEXT NOT NULL,
    round INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('same_engine', 'remove_lane')),
    state TEXT NOT NULL CHECK(state IN ('waiting', 'allowed', 'wait', 'consumed', 'removed')),
    engine TEXT,
    model TEXT,
    actor TEXT NOT NULL CHECK(actor IN ('core', 'person')),
    reason TEXT NOT NULL,
    expires_unix INTEGER NOT NULL,
    created_unix INTEGER NOT NULL,
    UNIQUE(subject_id, lane, commit_id, round, kind)
);
CREATE INDEX crew_review_controls_subject ON crew_review_controls(subject_id);

CREATE TABLE crew_acp_tier_revision (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    revision INTEGER NOT NULL CHECK(revision >= 0)
);
INSERT INTO crew_acp_tier_revision (singleton, revision) VALUES (1, 0);
CREATE TABLE crew_acp_tiers (
    engine TEXT PRIMARY KEY,
    tier TEXT NOT NULL CHECK(tier IN ('lead', 'worker', 'scout')),
    model TEXT NOT NULL
);

CREATE TABLE crew_review_publications (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL REFERENCES crew_tasks(id),
    review_task_id TEXT NOT NULL REFERENCES crew_tasks(id),
    scope_digest TEXT NOT NULL,
    replaces_id TEXT UNIQUE REFERENCES crew_review_publications(id),
    active INTEGER NOT NULL CHECK(active IN (0,1)),
    ever_sending INTEGER NOT NULL CHECK(ever_sending IN (0,1)),
    revision INTEGER NOT NULL CHECK(revision >= 0),
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE UNIQUE INDEX crew_review_publication_active_scope ON crew_review_publications(scope_digest) WHERE active=1;
