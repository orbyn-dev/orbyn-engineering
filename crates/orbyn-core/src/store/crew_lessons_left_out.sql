-- Migration 81: KC5 slice 2, the brief's lesson bookkeeping. A lesson that fails a task's answer-key check on the way
-- into that task's brief is left out of that one brief and the task still starts; this counter is the contract's
-- "left_out_of" ("Left out of 1 brief"). A use is counted on the lesson when a brief carries it, in the same transaction
-- that stores the brief. Neither counter changes what a lesson says, and nothing here is synced or exported.
ALTER TABLE crew_lessons ADD COLUMN left_out_of INTEGER NOT NULL DEFAULT 0 CHECK(left_out_of >= 0);
