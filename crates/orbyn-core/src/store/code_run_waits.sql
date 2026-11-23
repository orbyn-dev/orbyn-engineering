-- Migration 74: the time a Code Mode run waited for a person (docs/code-mode.md, "Run record").
-- A run starts at 0; an approval records its creation to the millisecond so a decision or expiry adds the
-- exact wait to its run. Rows stored before this migration stay NULL: no data, no number.
ALTER TABLE code_runs ADD COLUMN waited_ms INTEGER;
ALTER TABLE approvals ADD COLUMN created_at_ms INTEGER;
