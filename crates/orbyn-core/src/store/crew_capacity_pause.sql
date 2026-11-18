-- Migration 50: additive. Whether a project pauses new low-priority Crew starts at all. Off by default: until a person
-- turns it on, the capacity check lets every start go. Existing rows (and projects with none) stay off.
ALTER TABLE crew_capacity_settings ADD COLUMN pause_enabled INTEGER NOT NULL DEFAULT 0 CHECK(pause_enabled IN (0, 1));
