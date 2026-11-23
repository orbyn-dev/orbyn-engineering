-- Migration 77: additive. A task resumes once after a restart. `resume_count` is 0 until
-- that resume, `resumed_at` records when it happened, and `downtime_secs` is the time the app
-- was off before that resume, so the task's budget does not pay for it. Only ids, counts and
-- times are stored here: no task text and no engine output.
ALTER TABLE crew_runs ADD COLUMN resumed_at TEXT;
ALTER TABLE crew_runs ADD COLUMN resume_count INTEGER NOT NULL DEFAULT 0 CHECK(resume_count >= 0);
ALTER TABLE crew_runs ADD COLUMN downtime_secs INTEGER NOT NULL DEFAULT 0 CHECK(downtime_secs >= 0);
