-- Migration 85: Crew skill proposals (KC9 part 1). A fixed-template skill a person may approve when three or more active
-- lessons of one repository share a group key: the source, the failing check kinds and the path area. The lessons' own text
-- is the proposal's text; no model is called and nothing leaves the device. Device only, never synced or exported, and never
-- a rule: a proposal changes no router, verifier, answer key, lease, budget or check, and it blocks nothing.
--
-- `group_key` is the source and the two grouping columns joined with the unit separator; one row per (project, group key)
-- ever, so a dismissed or approved pattern is never proposed again. `lessons` is ids only, never content. The state moves
-- open -> approved or open -> dismissed, with `rev` refusing a write made from a screen that changed. A proposal row is kept
-- after its source lessons are disabled, hidden or purged.
ALTER TABLE crew_lessons ADD COLUMN check_kind TEXT;
ALTER TABLE crew_lessons ADD COLUMN path_class TEXT;
CREATE TABLE crew_skill_proposals (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    group_key TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','approved','dismissed')),
    text TEXT NOT NULL CHECK(length(text) BETWEEN 1 AND 600),
    lessons TEXT NOT NULL CHECK(json_valid(lessons)),
    skill_name TEXT,
    rev INTEGER NOT NULL DEFAULT 1 CHECK(rev >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    approved_at TEXT,
    UNIQUE(project_id, group_key)
);
