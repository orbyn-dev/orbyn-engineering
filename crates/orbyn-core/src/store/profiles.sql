-- Execution profiles (docs/execution-profiles-and-library.md, Part 1). Built-in profiles live in code, not here. An
-- application row records what a thread or crew last got, copied at apply time. It has no foreign key to `profiles` on
-- purpose: deleting or editing a profile never changes what was applied. A thread's row is removed with the thread.
CREATE TABLE profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    engine TEXT,
    model_tier TEXT NOT NULL CHECK (model_tier IN ('lead','builder','scout')),
    permission_mode TEXT NOT NULL CHECK (permission_mode IN ('supervised','auto_edits','auto','full_access')),
    autopilot_mode TEXT NOT NULL CHECK (autopilot_mode IN ('off','on','full')),
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE profile_applications (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('thread','crew')),
    scope_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    name_at_apply TEXT NOT NULL,
    revision_at_apply INTEGER NOT NULL,
    customized INTEGER NOT NULL DEFAULT 0,
    applied_json TEXT NOT NULL DEFAULT '{}',
    applied_at TEXT NOT NULL,
    PRIMARY KEY (scope_kind, scope_id)
);
