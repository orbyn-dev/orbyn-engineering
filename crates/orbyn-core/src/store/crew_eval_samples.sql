-- Migration 88: EvalHistory samples (KC5 slice 4). One row per crew task, recording what the task's brief kind was,
-- whether it carried lessons and which ids, and how it ended. This is the measurable half of the lessons design:
-- recording only. Nothing reads a sample to change routing, `score`, `EvalHistory`, the brief, a check or a lease.
--
-- Device only, never synced or exported. The settings sync is an allowlist of preference keys and does not name this
-- table. The row holds ids and enums only, never lesson text or any other content. The outcome is written once, when
-- the task first reaches a sampled state, and a later move never overwrites it.
CREATE TABLE crew_eval_samples (
    task_id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    project_id TEXT,
    kind TEXT NOT NULL,
    had_lessons INTEGER NOT NULL CHECK(had_lessons IN (0,1)),
    lesson_ids TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(lesson_ids)),
    left_out_ids TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(left_out_ids)),
    outcome TEXT CHECK(outcome IS NULL OR outcome IN ('passed','failed','stopped','cancelled')),
    stop_reason TEXT,
    engine TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);
CREATE INDEX crew_eval_samples_by_project ON crew_eval_samples(project_id, created_at);
