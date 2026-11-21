-- Migration 60: issue trackers (docs/issues.md). A connection keeps the tracker's host and scopes and never a
-- token (tokens live in the OS keychain). A link keeps a snapshot of the issue beside the thread, so a chip
-- draws with the tracker offline.
CREATE TABLE issue_trackers (
    tracker TEXT PRIMARY KEY,
    host TEXT,
    scopes TEXT NOT NULL DEFAULT '[]',
    connected_at TEXT NOT NULL
);

CREATE TABLE issue_links (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    tracker TEXT NOT NULL,
    scope TEXT NOT NULL,
    key TEXT NOT NULL,
    snapshot TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (thread_id, tracker, scope, key)
);
CREATE INDEX issue_links_by_issue ON issue_links(tracker, scope, key);
