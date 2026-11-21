-- Migration 64: the repositories of a project beyond its own folder (docs/multi-repo.md). Trust is per
-- repository. `clone_state` is `cloning` or `failed` while a clone runs or when it failed, else NULL.
CREATE TABLE project_repos (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    trusted INTEGER NOT NULL DEFAULT 0,
    is_git INTEGER NOT NULL DEFAULT 1,
    managed INTEGER NOT NULL DEFAULT 0,
    clone_state TEXT,
    clone_message TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX project_repos_by_project ON project_repos(project_id, created_at);
