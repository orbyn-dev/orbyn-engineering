-- Migration 90: a multi-repository thread's run is snapshotted in each of its worktrees (docs/multi-repo.md). The
-- task folder is not a repository, so a run there used to record no checkpoint at all and every turn or thread diff
-- answered `404 not_found`. The run-level `checkpoints` row keeps the first repository's ends, so one run is one
-- row for the turn picker; a repository's own ends live here, because a commit only means something in the
-- repository whose Git objects hold it. Like `checkpoints`, a row goes when its thread does.
CREATE TABLE checkpoint_repositories (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    start_sha TEXT NOT NULL,
    end_sha TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (run_id, repo_id)
);
CREATE INDEX checkpoint_repositories_by_thread ON checkpoint_repositories(thread_id, repo_id, created_at);
