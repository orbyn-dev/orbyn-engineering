-- The rows a later release left in an old database: one project, one thread with a finished run,
-- and one audit row. H4's builder supplies the schema at a chosen version from the real migration
-- list, so only these rows are a fixture; `{{PROJECT_PATH}}` is replaced with a real folder by the test.
INSERT INTO projects (id, name, path, trusted, is_git, created_at)
VALUES ('p-mid', 'Kept project', '{{PROJECT_PATH}}', 1, 0, '2026-01-01T00:00:00Z');
INSERT INTO threads (id, project_id, title, engine, model, cwd, next_seq, created_at, updated_at)
VALUES ('t-mid', 'p-mid', 'A thread from an earlier release', 'codex', NULL, '{{PROJECT_PATH}}', 1,
        '2026-01-01T00:00:01Z', '2026-01-01T00:00:02Z');
INSERT INTO runs (id, thread_id, engine, state, owner_epoch, error, started_at, ended_at)
VALUES ('r-mid', 't-mid', 'codex', 'completed', 'old-epoch', NULL, '2026-01-01T00:00:01Z', '2026-01-01T00:00:02Z');
INSERT INTO audit_events (ts, actor, action, target, decision, source, thread_id, run_id, detail)
VALUES ('2026-01-01T00:00:02Z', 'user', 'thread.create', 't-mid', NULL, 'user', 't-mid', NULL, '{}');
