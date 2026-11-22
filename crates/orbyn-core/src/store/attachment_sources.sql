-- Migration 73: the source kept beside an attachment (docs/snapshots.md). One row per attachment, written by
-- `PUT /api/attachments/{id}/source`; `source` is the wire JSON, `created_at` when it was last set. It can hold
-- the app name, the window title and the accessible text, so nothing here is logged. It goes with its attachment.
CREATE TABLE attachment_sources (
    attachment_id TEXT PRIMARY KEY REFERENCES attachments(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    created_at TEXT NOT NULL
);
