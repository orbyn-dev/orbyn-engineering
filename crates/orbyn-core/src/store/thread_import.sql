-- Migration 68: a thread made from a transcript file says so (docs/attachments-transcripts.md). `source` is `orbyn-transcript` and
-- `imported_at` is when; both stay NULL on a thread made here. No engine session id is ever stored from a file.
ALTER TABLE threads ADD COLUMN source TEXT CHECK (source IS NULL OR source IN ('orbyn-transcript'));
ALTER TABLE threads ADD COLUMN imported_at TEXT;
