-- Migration 65: the personal library (docs/execution-profiles-and-library.md, Part 2): workflows, prompts and environment
-- sets, all on this device. An environment set is a name and a list of key NAMES. There is no value column anywhere in
-- these tables, on purpose: values live in the OS keychain only (hard rule 1), and a test fails if a column could hold one.
CREATE TABLE library_workflows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    command TEXT NOT NULL,
    params_json TEXT NOT NULL,
    env_set_ids_json TEXT NOT NULL DEFAULT '[]',
    -- Where the entry came from. Only `local` (written on this device) runs without a first acknowledgement; for any other
    -- source the person's acknowledgement is kept as the hash of the command and parameters it covered.
    source TEXT NOT NULL DEFAULT 'local' CHECK (source IN ('local','import','repo','shared')),
    acked_hash TEXT,
    acked_at TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE library_prompts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL,
    params_json TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'local' CHECK (source IN ('local','import','repo','shared')),
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE library_env_sets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE library_env_keys (
    set_id TEXT NOT NULL REFERENCES library_env_sets(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (set_id, key)
);
