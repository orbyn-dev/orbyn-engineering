-- Migration 89: the one issue tracker scope a project's issues come from (docs/issues.md, "Create, comment, react,
-- templates and project binding"). It stores two names and no secret; a removed project takes its binding with it.
CREATE TABLE project_issue_trackers (
    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    tracker TEXT NOT NULL,
    scope TEXT NOT NULL,
    created_at TEXT NOT NULL
);
