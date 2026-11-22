-- Migration 74: the check a person makes (issue #794). A `manual` check of an answer key is not something code can run: it waits for a
-- person's decision. `crew_manual_checks` holds that wait: one row per (task, check), `awaiting` until a person passes or denies it, then
-- `passed` or `failed` with the person's note and when they decided. The table holds no command, no output and no key text, and a task's
-- row goes when the task does.
CREATE TABLE crew_manual_checks (
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    check_id TEXT NOT NULL CHECK(length(check_id) BETWEEN 1 AND 64),
    state TEXT NOT NULL DEFAULT 'awaiting' CHECK(state IN ('awaiting', 'passed', 'failed')),
    note TEXT CHECK(note IS NULL OR length(note) BETWEEN 1 AND 200),
    waiting_unix INTEGER NOT NULL,
    decided_unix INTEGER,
    PRIMARY KEY (task_id, check_id)
);
CREATE INDEX crew_manual_checks_waiting ON crew_manual_checks(state, waiting_unix);
