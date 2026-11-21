-- Migration 66: the repositories a multi-repository thread works in (docs/multi-repo.md). One worktree per
-- repository on the thread's branch; `position` 0 is the primary repository.
CREATE TABLE thread_repos (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL,
    branch TEXT NOT NULL,
    worktree_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (thread_id, repo_id)
);
