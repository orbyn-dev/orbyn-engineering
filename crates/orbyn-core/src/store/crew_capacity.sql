-- Migration 47: additive review capacity. A task carries its priority and whether Crew began it itself; a
-- project keeps a waiting-merge limit and a sampling choice; a sampled batch keeps what was chosen and the seed.
-- Existing tasks become `medium` and person-started, so none of them is ever paused.
ALTER TABLE crew_tasks ADD COLUMN priority TEXT NOT NULL DEFAULT 'medium' CHECK(priority IN ('high', 'medium', 'low'));
ALTER TABLE crew_tasks ADD COLUMN crew_started INTEGER NOT NULL DEFAULT 0 CHECK(crew_started IN (0, 1));

CREATE TABLE crew_capacity_settings (
    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    limit_n INTEGER NOT NULL CHECK(limit_n >= 1),
    sampling TEXT NOT NULL CHECK(sampling IN ('off', 'one_in_four'))
);

CREATE TABLE crew_review_samples (
    batch_id TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    chosen INTEGER NOT NULL CHECK(chosen IN (0, 1)),
    seed INTEGER NOT NULL,
    chosen_unix INTEGER NOT NULL,
    PRIMARY KEY (batch_id, task_id)
);
