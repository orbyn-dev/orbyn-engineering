# Who wrote a line: a positive person signal for merge provenance (4l)

Design note for issue #711. Status: built (slice 4l, migration 55), except the person path that records `crew_person_commits` rows, which does not exist yet: until one does, no line is `person`, and coverage stays merge-level.

## The problem

Merge provenance (`crew/capacity/provenance.rs`) labels each added line through `git blame` of the merge commit. A line is a person's only when its last-changing commit is in the positive `person_commits` list the caller passes; the merge hook passes an empty list. The list cannot be computed from authorship today:

- Workers run in the lead's checkout, as the person, and commit with the person's own git identity. An author match would label agent commits as the person's and overstate review coverage.
- No known person identity exists in config, and the person also commits by hand, in an editor or a terminal, where the app sees nothing.

A wrong "person" label is worse than a missing one: coverage would count an unreviewed agent line as written by a human. So the rule stays fail-closed: a commit is the person's only on positive evidence, and everything else counts as agent for coverage.

## Recommendation

Mark agent commits positively, mark person commits positively only where the app itself can see them, and keep a third value for the rest.

1. **A distinct worker identity.** Every process a Crew worker runs gets `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME` and `GIT_COMMITTER_EMAIL` set to `Orbyn Crew worker (<engine>/<model>) <worker+<task id>@orbyn.invalid>`. Engines are spawned through `for_engine` (`host/process.rs`) in `agents/driver/pipes.rs`, and per-run variables are added after it, so a value set there wins. Today `RunSpec` carries no task id (only `confine.is_some()` marks a worker), Claude passes a per-run environment, and the OpenCode (ACP) driver passes only its fixed per-engine environment and Codex passes none, so neither has a per-run one: the build adds a task id to `RunSpec` and a per-run environment to those two drivers. The environment overrides the person's git config for that process only; the person's own config is not touched. The task id in the address ties a commit to its worker task.

   Matching uses only the exact `worker+<task id>@orbyn.invalid` address, where the id has the app's id format and exists in the crew tasks table. The `<engine>` and `<model>` values in the display name are cleaned (anything outside `[A-Za-z0-9._:/-]` becomes `_`, at most 64 characters) and never used for matching. Codex workers cannot run today (`CODEX_WORKERS_ALLOWED = false`) and workers run on macOS only; the Codex part waits for that.
2. **A trailer as a second mark, best effort.** The worker brief asks for `Orbyn-Task: <task id>` in commit messages. The app cannot enforce it (the person owns the repository's hooks), so the identity is the mark and the trailer only corroborates it.
3. **Person commits only for commits the app makes on a person action. Future: no such path exists today.** The only commit made on a person's action is the optional README "Initial commit" at project creation (`project/projects.rs`), which is not recorded; every other production commit uses an app identity (`Orbyn Crew`, and `Orbyn <orbyn@localhost>` for workspace provisioning and snapshots). When a path exists, the app records the full commit id in a local table (`crew_person_commits`: project, commit, time, action) in the same transaction as the audit row, and `person_commits` is read from that table. A commit the person makes outside the app is not listed. A commit is listed only if it contains no worker-written change: workers write into the person's own checkout, so while any worker run has touched the staged files since the last commit, the commit is refused or recorded as `unknown`.
4. **Three values, two counts.** A line is `agent` (the author or the committer is a worker address, or the task trailer matches), `person` (commit id is in the table) or `unknown` (anything else: the person's manual commits, history before this change, bots). Coverage counts `unknown` as agent: it is never credited as human-written. The service returns all three values; the coverage screens fold `unknown` into the agent count, labelled "Agent or unconfirmed". This reverses today's rule that author emails are never read (`provenance.rs`): reading author and committer emails can only split `agent` from `unknown` and can never produce `person`.
5. **A person may attest.** Later, a person-key route can mark a merge "I wrote this" (audit first, `seen`). It adds those commit ids to the table. Nothing is inferred from an email match.

## What it does not do

- The identity is a label, not a boundary. A worker can override the variables in its own command (`GIT_AUTHOR_NAME=... git commit`, `--author`, `env -u`), an engine's shell may rebuild its environment, and the person's shell profile may export `GIT_AUTHOR_*` (Claude's Bash tool loads it). It is enough for coverage (a worker that hides itself shows up as `unknown`, which counts as agent) and is never used to authorise anything.
- It does not tell a person's manual commit from a bot's: both are `unknown`.

## What changes for existing setups

- **History.** Existing commits carry no worker identity and are not in the table: all `unknown`, so coverage for old merges does not change.
- **Existing checkouts.** Workers still run in the lead's checkout. The environment override works there without a worktree. When worker worktrees are built (planned), the same environment applies, and each worktree can also set the identity in its own `config.worktree`.
- **Engines.** Engines that pass their own author flag or add `Co-Authored-By` trailers keep doing so; their commits are `unknown` unless the environment wins. Each engine needs a probe: commit once in a scratch repository, under the person's real shell profile, and read both author and committer.
- **Sandbox.** The macOS worker profile does not touch environment variables or git. The escape-check proof environment is cleared to a short list and never commits, so it is unaffected. The proof's own commits do not exist.
- **The app's own commits** (merge commits, notes under `refs/notes/orbyn-agent-trace`, `Orbyn Crew <crew@orbyn.invalid>`) are not labelled as a person's. Conflict-resolution lines are blamed to the merge commit, which is authored by `Orbyn Crew`: the service returns them as `unknown`, so they count as "Agent or unconfirmed".
- **Cost.** One small table and a migration (next free number at merge), a task id on `RunSpec` with a per-run environment for the OpenCode and Codex drivers, and a trailer line in the brief.

## Decisions

- v1 does not credit a person's manual commits: `unknown` counts as agent until a person attests.
- The coverage screens fold `unknown` into the agent count, labelled "Agent or unconfirmed"; the service still returns all three values.
- One commit probe each for Claude, OpenCode and Codex (Codex once workers are allowed). Each records whether the variables win for author and committer; the build lists the result per engine. Engines where the environment does not win stay `unknown`.

## Probe results

One commit per engine, in a scratch repository, with the four variables set in the engine process's environment the way the app sets them, and
the commit made by the engine's own shell tool. Read back with `git log --format='%an <%ae> | %cn <%ce>'`. Date of the probes: 2026-11-19.

| Engine | Result |
| --- | --- |
| OpenCode (v2.2.5) | The environment won for author and committer, name and email, with the person's real shell profile (it exports no `GIT_*` variable). Run as `opencode run --standalone --auto` with model `opencode/beam-free`; that is not the `opencode acp` process the app launches, and without `--standalone` OpenCode v2 hands the command to a background service that does not inherit the caller's environment, so this probe covers the shell tool only. A second commit with a profile that exports `GIT_AUTHOR_EMAIL` and `GIT_COMMITTER_EMAIL` (through `ZDOTDIR`) took the profile's emails and kept the environment's names: such a commit reads `unknown`, as the note says. |
| Claude (2.1.327) | Not probed: the CLI in this environment reports `loggedIn: false` ("OAuth session expired and could not be refreshed"), so no run could start. Its labels are unverified until it is probed. |
| Codex (0.166.0) | Not probed: `CODEX_WORKERS_ALLOWED` is false, so no Crew worker runs on it, and this change does not loosen that. The per-run environment is in the driver and is unit-tested only as far as its construction. |

Engines where a probe shows the environment does not win stay `unknown`. Re-run the Claude probe where the CLI is signed in before relying on its labels.

## As built: worker identity, trailer and the person table

Slice 4l is built (migration 55); `docs/crew.md`, "As built (slice 4l, person signal, migration 55)", records the merge hook's side.
No route was added.

- **Worker identity.** `RunSpec.crew_task` (`crates/orbyn-core/src/agents/driver/mod.rs:117`) is set when the run's thread works a
  Crew task (`conversation/runs/turn.rs:137-215`), and the Claude, OpenCode (ACP) and Codex drivers append the four
  `GIT_AUTHOR_*` / `GIT_COMMITTER_*` variables to that process's environment after `for_engine` (`agents/driver/pipes.rs:142-146`;
  `claude/mod.rs:449`, `acp/mod.rs:218`, `codex/mod.rs:384`, all through `agents/driver/worker_identity.rs`). A warm process
  started ahead of the turn is not reused for a worker, because it missed the identity (`turn.rs:141-146`). Only the exact
  `worker+<task id>@orbyn.invalid` address of an app-format id naming an existing crew task matches; case, a look-alike or a
  `.mailmap` cannot make a line a person's (`worker_identity.rs`, `crew/capacity/provenance.rs:494-570`). Codex workers still
  cannot run (`host/confine/mod.rs:41`), so its per-run environment is unit-tested only.
- **Trailer.** The worker brief asks for `Orbyn-Task: <task id>` in the commit message (`crew/start.rs:738`). Provenance reads it and
  marks a line `agent` only when the named task exists (`provenance.rs:445-492`); a trailer can never make a line a person's.
- **Three values.** `Author::Agent` / `Person` / `Unknown` (`provenance.rs:41-59`): `agent` on a worker author or committer, or a
  trailer naming an existing task; `person` only when the blamed commit is in the project's `crew_person_commits`; otherwise
  `unknown` (a manual commit, history before the identities, a bot, the app's own `crew@orbyn.invalid` merge, so a conflict
  resolution is `unknown`). Migration 55 rebuilds `crew_hunks` so `author_kind` allows `unknown` and copies old rows as they were.
  Coverage counts `unknown` with the agent's lines (`crew/capacity/coverage.rs:109-111`); the wire shape did not change, so
  `agent_lines` is labelled "Agent or unconfirmed" (`crates/orbyn-contracts/src/crew_capacity.rs:166`,
  `web/src/features/crew/capacity/live/LiveCoverage.tsx`), not the three fields the design's item 4 described.
- **The table, and what the person path still lacks.** `crew_person_commits` (project, full commit id, time, action) is written only
  by `Store::record_person_commit`, in the same transaction as its `crew.person_commit` audit row
  (`store/crew_person_commits.rs:31-71`); the merge hook passes the project's list through `crew_person_commits_for_task`
  (`crew/capacity/hooks.rs:88-116`). Nothing calls the writer outside tests: no app path records a person commit today (the only
  commit made on a person's action, the optional README "Initial commit" at project creation, is not recorded), and no route
  records one. So the list stays empty, no line is `person`, and coverage can only count agent and unconfirmed lines. The
  design's person attestation (item 5) is not built, and nothing infers `person` from an email.
