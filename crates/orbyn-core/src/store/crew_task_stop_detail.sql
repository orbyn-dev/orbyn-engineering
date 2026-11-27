-- Migration 86: KC8 follow-up, a typed detail for a stop whose reason stays `engine_error`. A worker that
-- stops after its one resume after a restart keeps `stop_reason = 'engine_error'`, so every old reader sees
-- the stop it always saw, and gains `stop_detail = 'worker_stopped_twice'` so the needs-you field can name
-- the second death. This is an added column and not a rebuild of `crew_tasks`: 26 tables have a foreign key
-- to it, and SQLite would have to drop and copy all of them to change the existing stop-reason CHECK. Only
-- the typed word is allowed; NULL means the stop has no detail (a reviewer lane, a task that could not be
-- resumed, or a resume brief that could not be sent).
ALTER TABLE crew_tasks ADD COLUMN stop_detail TEXT CHECK(stop_detail IS NULL OR stop_detail IN ('worker_stopped_twice'));
