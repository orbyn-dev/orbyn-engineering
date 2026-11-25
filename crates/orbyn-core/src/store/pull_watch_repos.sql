-- Migration 90: a pull request watch belongs to one repository of the thread (docs/multi-repo.md, "Pull requests").
-- Two repositories can both have a #12. Every existing watch moves to the thread's own project, which is its
-- primary repository, so a thread with one repository is unchanged.
CREATE TABLE pull_watches_by_repo (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL,
    number INTEGER NOT NULL,
    snapshot TEXT,
    comment_wakes INTEGER NOT NULL DEFAULT 0,
    failures INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    PRIMARY KEY (thread_id, repo_id, number)
);
INSERT INTO pull_watches_by_repo (thread_id, repo_id, number, snapshot, comment_wakes, failures, created_at)
    SELECT w.thread_id, t.project_id, w.number, w.snapshot, w.comment_wakes, w.failures, w.created_at
    FROM pull_watches w JOIN threads t ON t.id = w.thread_id;
DROP TABLE pull_watches;
ALTER TABLE pull_watches_by_repo RENAME TO pull_watches;
