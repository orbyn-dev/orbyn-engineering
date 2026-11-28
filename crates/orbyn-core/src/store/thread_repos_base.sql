-- Migration 90: the branch each repository's worktree of a multi-repository thread was cut from
-- (docs/multi-repo.md). The start records the base the person chose for each repository, and a repository added to
-- an existing thread records the branch that repository had checked out. A row written before this column keeps
-- NULL: the base was never stored, so it cannot be recovered, and null is never read as a default.
ALTER TABLE thread_repos ADD COLUMN base_branch TEXT;
