-- Migration 71: a pull request link belongs to one repository of the thread (docs/multi-repo.md, "Pull requests").
-- Two repositories can both have a #12. Every existing link moves to the thread's own project, which is its
-- primary repository, so a thread with one repository is unchanged.
CREATE TABLE thread_pull_requests_by_repo (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL,
    number INTEGER NOT NULL,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    state TEXT NOT NULL,
    head TEXT NOT NULL,
    base TEXT NOT NULL,
    author TEXT,
    PRIMARY KEY (thread_id, repo_id, number)
);
INSERT INTO thread_pull_requests_by_repo (thread_id, repo_id, number, url, title, state, head, base, author)
    SELECT p.thread_id, t.project_id, p.number, p.url, p.title, p.state, p.head, p.base, p.author
    FROM thread_pull_requests p JOIN threads t ON t.id = p.thread_id;
DROP TABLE thread_pull_requests;
ALTER TABLE thread_pull_requests_by_repo RENAME TO thread_pull_requests;
CREATE INDEX thread_pull_requests_by_number ON thread_pull_requests(repo_id, number);
