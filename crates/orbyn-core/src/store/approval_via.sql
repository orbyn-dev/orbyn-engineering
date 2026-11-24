-- Migration 68: the Code Mode run and call an approval came from (docs/code-mode.md, "Approvals from a script").
-- Display only: a JSON {code_run, call, script_sha256}, NULL for every other approval.
ALTER TABLE approvals ADD COLUMN via TEXT;
