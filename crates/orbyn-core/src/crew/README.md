# Crew

Crew is Preview: the engine-agnostic crew of agents of [docs/crew.md](../../../../docs/crew.md),
behind `CoreConfig::crew_preview`, which ships `false` (`ORBYN_CREW_PREVIEW=1` turns it on for the service). With it off, no Crew tool is listed or
callable and nothing here runs. The protocol's objects are in `orbyn-contracts` and are stored
by `store::crew` ([Crew Protocol](../../../../docs/crew/protocol/SPEC.md)).

## What it owns

| Module | Owns |
| --- | --- |
| `router` | Router v0: filters, scores from seeded defaults, an explicit engine honoured and still checked, and the `RouteDecision`. Pure: no model, no process, no file |
| `filters` | The hard filters, in order, each rejection with its reason. `class_screen` runs first |
| `classes` | Data classes: `.orbyn/crew.json` read at the pinned base commit and at the default branch, the stricter applies (strict, fail closed), the last accepted default-branch ref and commit remembered per project with a person's confirm or reset, the glob, a brief's files against the classes, the `route_class` ledger row and the three read-only views |
| `score` | Pass rate for the brief's kind (seeded defaults until `EvalHistory` has a measured rate) and quota headroom, in permille |
| `tiers` | The tier a role needs; native model-family words and explicit exact ACP engine/model admission |
| `tier_settings` | Person-owned ACP assignments: revision saves plus reviewed-default preconditions; no tier guessing |
| `view` | A plain copy of one engine's status, models and usage |
| `gather` | Builds the views from the engine registry and model list the engine picker reads, and limits from the usage cache only |
| `stamp` | A brief draft becomes a brief: the core adds the envelope, a draft that carries one is refused |
| `draft` | The JSON Schema `crew_start` lists for its drafts, a valid example, and every problem in a bad draft at once |
| `guard` | The secret scan (H2) and the answer-key leak check (H1) |
| `persist` | Storing a decision with its task, after both checks |
| `tool` | The tool list and `crew_route`, the dry-run tool |
| `start` | `crew_start`: validates the brief and key, routes, grants the leases and runs the worker through `delegate` |
| `report` | `crew_report`: the worker's result, stamped by the core, then the verifier |
| `lifecycle` | The task states and transitions of SPEC.md section 6, and the loop guard. Pure |
| `leases` | Grants the brief's files to its worker; refuses protected paths, files outside the brief and a reviewer's write; reads expiry on use |
| `checks` | The check runners: command kinds, `file_scope`, `diff_size`. Any other kind is `skipped` |
| `verify` | The verifier: runs the key in the task's checkout, writes Evidence and the Gate, builds the reason a worker may read |
| `workers` | The allow list of Orbyn tools a Crew worker may call; every other `orbyn_*` tool, including any added later, is refused |
| `protect` | Records the key's protected files (type, executable bit, SHA-256 of the exact bytes, links not followed) before the worker exists, and compares at verify time. Git is not asked, so nothing the worker can write in `.git` hides a change |
| `recover` | After a restart: clears dead verification claims, verifies a task left `verifying`, stops one whose worker is gone, sets the budget timers again |
| [`bus`](bus/README.md) | The Crew bus: `crew_post`, `crew_ask`, the board, steering delivery and the guards for hops, posts, sizes and the end of a run |
| [`autopilot`](autopilot/README.md) | Autopilot: the always-denied list, the approval chain, two keys, trip wires and the approval ledger |
| [`review`](review/README.md) | Review stages: reviewer lanes, typed findings, the finding check, the fix loop and repo rules that add lanes |
| `capacity` | Review capacity: the count of waiting High and Medium merges, whether a new Low-priority start that Crew began itself pauses over the limit (8), and the seeded random sample of a batch (off by default, one in four). The decisions are pure. `provenance` is the one part that runs git: it reads the merged range's new-file hunks (byte-exact `-U0` diff; binary, deleted and unsafe paths skipped), gives each line an author through `git blame` of the merge commit (the task's engine and model, or a person only for a line whose last-changing commit is in the positive `person_commits` list the caller passes; anything not positively a person's is the agent's, and an author email is never read; the merge hook must pass only positively identified person commits, never a guess) and writes a local-only note on the merge commit under `refs/notes/orbyn-agent-trace` (ids, ranges and counts, never content; a merge with no hunk has no note and no `note_ref`, so it reads as Not measured). `crew_provenance_start` is first-write-wins: an older merge recorded late reads as Not measured. Provenance and the coverage built on it are merge-level, not line-level review. The start hook (`capacity/hooks.rs`, flag-gated) runs before `crew_start` grants a worker and refuses a Low-priority start that Crew began itself over the limit with `not_authorized`, naming the waiting count and the limit. Pausing is off until a person turns on `pause_enabled` (migration 50). A launch with `Links::crew_started` (review fixes, merge-comment fixes) is stored Low and Crew-started; every other start is Medium and person-started. `capacity/coverage.rs` and `capacity/api.rs` build the coverage map and the person-facing state. The merge hook (same file, flag-gated) runs after a merge row is stored, in a detached task with an EMPTY person-commit list: every line reads as the agent until a later change passes positively identified person commits. A failure is logged and written to the approval ledger on the merge's task as `provenance_failed`, and never fails the merge. Sampling is not wired (its boundary would be one `crew_process_merges` pass). Settings, a task's priority and the recorded samples are kept by `store/crew_capacity.rs` (migration 47), hunks and the first provenance time by `store/crew_provenance.rs` (migration 48) |
| `backtest` | Reviewer backtest, pure part (slice 4n PR 1): which pull requests are eligible (merged within 180 days, at least 30 days old), the `EscapeSource` trait for outcomes (the escape log is not merged, so the only source reports Not measured), and each lane's recall and precision from 5 escapes and 10 findings, matched by file only. `replay` (PR 2) is the one part that runs: `run` replays each requested review lane or check on each eligible pull request through a `LaneRunner` (fake in tests, no live engine yet), keeps a budget of tokens per engine that is never summed or pooled (a lane stops when its own engine's budget is spent; lanes of one engine share it), records a broken lane as failed without stopping the others, stores each finding fingerprint with `caught`, `unmatched` or `no_escape`, and scores with `score`. When the escape source says Not measured the run still finishes and shows finding counts only. A request is refused if it has a builder lane, a money-like field or name, or an engine without a budget. A person starts it: no timer, startup hook or scheduler calls it, and a test fails if any other non-test code reaches it. `api` (PR 3) is the person's way in, behind the Preview flag: `crew_backtest_estimate` and `crew_backtest_report` only read; `crew_backtest_start` records the run and replays it in the background, one run at a time (a second start is refused in the same transaction that would insert it), and is refused until a live `LaneRunner` exists (none yet: tests install a fake); `crew_backtest_stop` only asks, and the run ends as stopped before its next replay. Outcomes are Not measured until the escape log (4k) is merged. History is the project's own Crew merges, numbered by merge record. `crew_backtest_recover` runs once when the core opens and marks a run a crash left `running` as failed, "interrupted". Scoring normalises paths (`./`, `..`, separators) and counts identical findings once. Stored by `store/crew_backtest.rs` (migration 49, ids, counts and fingerprints only) |
| `backtest` | Reviewer backtest, pure part (slice 4n PR 1): which pull requests are eligible (merged within 180 days, at least 30 days old), the `EscapeSource` trait for outcomes (the escape log is not merged, so the only source reports Not measured), and each lane's recall and precision from 5 escapes and 10 findings, matched by file only. `replay` (PR 2) is the one part that runs: `run` replays each requested review lane or check on each eligible pull request through a `LaneRunner` (fake in tests, no live engine yet), keeps a budget of tokens per engine that is never summed or pooled (a lane stops when its own engine's budget is spent; lanes of one engine share it), records a broken lane as failed without stopping the others, stores each finding fingerprint with `caught`, `unmatched` or `no_escape`, and scores with `score`. When the escape source says Not measured the run still finishes and shows finding counts only. A request is refused if it has a builder lane, a money-like field or name, or an engine without a budget. A person starts it: no timer, startup hook or scheduler calls it, and a test fails if any other non-test code does. Stored by `store/crew_backtest.rs` (migration 49, ids, counts and fingerprints only). The routes are PR 3 |
| `escapes` | The escape log, read-only (slice 4k, PR 1): merges that got past review and checks and were later found defective. A revert is a **possible** escape at once, written by the schema (migration 51 back-fills it and a trigger on `crew_merges` adds it with the revert); confirmed only when a person confirms (PR 2). Possible, confirmed and dismissed are always separate counts. Display only: the breaker, Router and gates never read it. Views for `GET /api/crew/escapes`, `/escapes/{id}` and `/escapes/thresholds`; thresholds (60 d, 14 d, 10 merges, 2x) are device config in `settings`. Stored by `store/crew_escapes.rs`, whose read functions are the stable API for the backtest's `EscapeSource`. See [docs/crew/escapes.md](../../../../docs/crew/escapes.md) |
| `background` | Background crews' state: definition, strict validation, redaction, runs with spend against a budget set at creation, and the stop for the budget that queues a review. Stored in the data folder, audited, no trigger and no engine |
| `triggers` | `.orbyn/crew.json` `background` triggers and templates, parsed and checked: kinds, budgets, engines, schedules, hidden text and name collisions. Pure policy: nothing starts, writes or calls a model |
| `settle` | Applies a judgement: pass, or retry, change of approach or stop; the result for the lead; a worker's run ending |

## Rules to keep

- The router's inputs are the brief and this machine's own configuration. A brief has no
  field for the answer key, and no function here takes one except the leak check on storing.
- An engine or model the machine has not configured is never routed to. An explicit engine
  that fails a filter is refused with its reason, never swapped for another.
- Effort is `medium`, never higher. A review brief, the finding check and a second key are lead tier, and a named model below it is refused. A review lane is worker tier unless it is the `security` lane, a repo rule locks it, it is the merge-owed `correctness` lane or only the author's vendor can review it; a pinned model wins otherwise.
- Every rejection carries a reason, and a score carries its basis (`used_defaults`, or the
  measured rate with its sample size).
- No learning, trust score or counterfactual fork here: those are later slices, and
  `EvalHistory` is the one place they will feed in.
- Only the lead starts a task or routes (H3). A worker's thread is found by `crew_runs`, and its
  tools refuse once the run ends (H7); the core's own verification still adds Evidence and a Gate.
- The checks run when the worker's turn has ended, never during it, so the worker cannot change files
  while they run. Before any check, a change to a file in the key's protected paths fails the attempt
  with a reason that names no file (N7, H9, H11). The files are recorded at `crew_start` and compared
  byte for byte, never through git, so `.gitignore`, `update-index` flags, `.git/config`, replace refs, loose objects
  and alternates cannot hide an edit. Exactly one verification runs per task: it is claimed in the transaction.
- The verifier is code. A worker is told why a check failed (redacted, with the key's text
  withheld, then proven free of both) and never the check, its id or any other part of the key (H1).
- `failed` is never at rest: a failed verification goes through it to the state the loop guard
  names in one transaction. `final` is true only for `passed`, `stopped` and `cancelled`.
- The guard compares fingerprints: the failing checks, how each ended and the names of what
  failed (tests, or files outside the lease), never the output, which changes between runs. The same failure twice gives one change of approach; the same
  failure after that stops the run (`repeated_failure`).
- Messages and facts pass the secret scan and the answer-key check on the way in and out; a worker never sees the lead's
  thread or messages to others (see the [bus](bus/README.md)).
- A lease is read for expiry when it is used, never by a timer; a lapsed one is recorded `expired`
  once and blocks nobody. `crew_start` takes one lock, so two briefs cannot both be granted a path.

## Known limits of Preview

- Background processes a worker starts outlive its turn. They can edit protected files after a check has read
  them, or read a check command from `ps`. Fixing it needs the worker's own worktree and killing its process
  group (#436).
- A worker runs as the same OS user, so its shell can read the Orbyn SQLite store (the answer-key table and the
  lead's thread) or call the `orbyn` CLI. No tool refusal can stop that; it needs a sandbox that blocks reads of
  the app data folder (#441).
- An attachment tool takes a thread id; a worker may name only its own. `computer_*` and every `orbyn_*` tool
  off the allow list are refused to a worker.
- A key with protected paths and no record of them from the start fails closed (`needs_person`).

## Follow-ups

- Known gap: Evidence keeps the output's hash and a 4000-character redacted excerpt, not the full log.
- Evidence names HEAD as its commit; when the checkout had uncommitted changes, `does_not_prove` says so.
- `budget.minutes` ends a task even in the middle of a turn, through a timer that holds the core weakly and
  is set again after a restart. A verification gets what is left of the budget and stops once the task has
  been stopped. Turn limits are #438.
- Windows: the leak test (`a_failing_check_that_prints_its_own_command_leaks_nothing_to_the_worker`) is
  Unix-only. On Windows a command's echo can differ in quotes and line endings, so the withholding needs its
  own test there, and a check's deadline must kill every process the check started (the process owner does;
  no Windows test shows it yet).
- The worker runs in the lead's checkout, as `delegate` does. A writing worker's own worktree
  (docs/crew.md, "Isolation") is not created yet, so the diff checks see the lead's changes too.
- The check kinds `schema`, `http_probe`, `preview`, `visual_compare`, `repo_required`, `manual`,
  `screenshot_only`, `accessibility_audit`, `performance_budget`, `secret_scan` and
  `licence_scan` report `skipped` and are listed as not verified.
- Tier words match whole words in a model's id or label; they are defaults to check against real ids.

## How it is tested

Fake engines (`tests/fake.rs`) stand in for the registry, so each filter, each score, the
explicit override and the tool are checked with no process started. `tests/core.rs` runs the
tool through a real `Core` with a scripted engine driver. `tests/harness.rs` is a real `Core`
with a fake worker that writes files in a git checkout and calls `crew_report` over MCP: `runs.rs`
takes a task from its first turn to the end (fail, retry with the reason, the same failure twice,
one change of approach, stop; pass; no report; engine error), `starts.rs` covers what a start
refuses and the lease rules, `lifecycle.rs` checks the table against the spec's own, and
`verify.rs` the reason a worker may read and the check runners. `review.rs` runs the review stages (see the [review README](review/README.md)). `bus.rs` and `bus_guards.rs` run two fake engines (one
takes input mid-turn, one does not) through the [bus](bus/README.md).

The crate's [group table](../../README.md#groups) lists every group.
