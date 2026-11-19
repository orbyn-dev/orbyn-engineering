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
| `capacity` | Review capacity: the count of waiting High and Medium merges, whether a new Low-priority start that Crew began itself pauses over the limit (8), and the seeded random sample of a batch (off by default, one in four). The decisions are pure. `provenance` is the one part that runs git: it reads the merged range's new-file hunks (byte-exact `-U0` diff; binary, deleted and unsafe paths skipped), gives each line an author through `git blame` of the merge commit (the task's engine and model, or a person only for a line whose last-changing commit is in the positive `person_commits` list the caller passes; anything not positively a person's is the agent's, and an author email is never read; the merge hook must pass only positively identified person commits, never a guess) and writes a local-only note on the merge commit under `refs/notes/orbyn-agent-trace` (ids, ranges and counts, never content; a merge with no hunk has no note and no `note_ref`, so it reads as Not measured). `crew_provenance_start` is first-write-wins: an older merge recorded late reads as Not measured. Provenance and the coverage built on it are merge-level, not line-level review. The start hook (`capacity/hooks.rs`, flag-gated) runs before `crew_start` grants a worker and refuses a Low-priority start that Crew began itself over the limit with `not_authorized`, naming the waiting count and the limit. That refusal is never retried by a timer or a loop and is shown plainly as waiting: a merge-comment fix keeps its Comment stored and the merge's `needs_person` note says the fix waits for review capacity (answering again starts it, which clears the note); a review fix tells the lead the same, and since a confirmed finding is decided once, a new review of the work is its next trigger. Pausing is off until a person turns on `pause_enabled` (migration 50). A launch with `Links::crew_started` (review fixes, merge-comment fixes) is stored Low and Crew-started; every other start is Medium and person-started. `capacity/coverage.rs` and `capacity/api.rs` build the coverage map and the person-facing state. The merge hook (same file, flag-gated) runs after a merge row is stored, in a detached task with an EMPTY person-commit list: every line reads as the agent until a later change passes positively identified person commits. A failure is logged and written to the approval ledger on the merge's task as `provenance_failed` (stage, a fixed code-owned reason that never holds git output or a path, `coverage: Not measured`, `resolved: false`; the ledger write itself is tried three times), and never fails the merge. A failed merge has no note, so coverage reads it as Not measured. The next merge's hook first retries up to 10 unresolved failures, oldest first, recording each as of its own merge time (hunks are replaced, so a retry is safe); a success writes a `resolved: true` row on the same task. A repository whose provenance already began later than the failed merge still reads it as Not measured. Writes of `refs/notes/orbyn-agent-trace` are taken one at a time in this process, keyed by the shared git directory (so linked worktrees share the lock), and each git call is bounded at 30 s; a ref lock clash with another process is only tried again up to 5 times, with no cross-process guarantee. The `resolved: true` row keeps kind `provenance_failed` and has no `why` or `coverage`, so a generic ledger view lists it like a failure; read `resolved`. A retry pass takes the 10 oldest failures not yet retried 3 times; each failed retry writes a `retry_failed` row with `attempt`, and after the third the merge stays Not measured and is not retried again, so permanent failures cannot hold back newer ones. Sampling runs in the same detached task (`capacity/batch.rs`), only when a person set sampling to `one_in_four` (off by default; off reads the merge's project and its setting and writes nothing): a Low merge joins its project's open batch, the Low merges that landed since the last recorded batch, and the oldest four close first and again while four remain, so a merge is never skipped whatever order the hooks finish in (there is no digest in the app yet, so a short batch stays open). Low merges that landed before sampling was turned on are never sampled or recorded: the boundary is the time of the latest settings audit row that turned sampling on (`crew_sampling_since`, no migration), so turning it off and on again resets it, and saving other settings with sampling still on does not move it. One in four, rounded up, is picked at random with a seed from the system, not risk-weighted; High and Medium merges are never in a batch. Every task of the batch is recorded once in `crew_review_samples` with whether it was picked and the seed; the choice is never given to the worker and the merge row, review state and `needs_person` are not touched. `GET /api/crew/capacity` lists the picked tasks whose merge still waits as `picked`. A refused start is also noted as one bounded `crew_paused_starts` row (migration 54, `store/crew_paused_starts.rs`; ids, counts and times only, 200 per project, a repeat updates its row, the same trigger starting marks it `started`) and listed as `held` by `GET /api/crew/capacity`. Settings, a task's priority and the recorded samples are kept by `store/crew_capacity.rs` (migration 47), hunks and the first provenance time by `store/crew_provenance.rs` (migration 48) |
| `backtest` | Reviewer backtest, pure part (slice 4n PR 1): which pull requests are eligible (merged within 180 days, at least 30 days old), the `EscapeSource` trait for outcomes (the real source, `escape_log.rs`, reads the escape log: see below), and each lane's recall and precision from 5 escapes and 10 findings, matched by file only. `replay` (PR 2) is the one part that runs: `run` replays each requested review lane or check on each eligible pull request through a `LaneRunner` (fake in tests, no live engine yet), keeps a budget of tokens per engine that is never summed or pooled (a lane stops when its own engine's budget is spent; lanes of one engine share it), records a broken lane as failed without stopping the others, stores each finding fingerprint with `caught`, `unmatched` or `no_escape`, and scores with `score`. When the escape source says Not measured the run still finishes and shows finding counts only. A request is refused if it has a builder lane, a money-like field or name, or an engine without a budget. A person starts it: no timer, startup hook or scheduler calls it, and a test fails if any other non-test code reaches it. `api` (PR 3) is the person's way in, behind the Preview flag: `crew_backtest_estimate` and `crew_backtest_report` only read; `crew_backtest_start` records the run and replays it in the background, one run at a time (a second start is refused in the same transaction that would insert it), and is refused until a live `LaneRunner` exists (none yet: tests install a fake); `crew_backtest_stop` only asks, and the run ends as stopped before its next replay. Outcomes come from the escape log through `EscapeLog` (read-only): confirmed escapes only, merged in the 30 to 180 day window and found within 30 days of their merge; the pull request is the merge's record id and the files are those the escape's fix or revert commit changed, read from git on the merge's checkout; an escape whose files cannot be read (a post-merge check or a person's mark names no commit, no checkout, git failed) is counted as unresolved beside the lane's score and never scored; a log that cannot be read whole (over its 5,000 cap, one damaged row) or that holds no escape at all is Not measured, never partial. History is the project's own Crew merges, numbered by merge record. `crew_backtest_recover` runs once when the core opens and marks a run a crash left `running` as failed, "interrupted". Scoring normalises paths (`./`, `..`, separators) and counts identical findings once. Stored by `store/crew_backtest.rs` (migration 49, ids, counts and fingerprints only) |
| `escapes` | The escape log (slice 4k): reads (PR 1), a person's writes (PR 2, `person.rs`: mark, confirm, dismiss, gap; `Person` routes, audit first, `409 stale`, idempotent), a person's created check (PR 4, `check.rs`: propose, accept, approve, retry, withdraw; `Person` routes; only accept starts a worker, through the router; the worker may add exactly one test file (`check_paths.rs`); the proof (`check_proof.rs`) runs on a `git archive` tree, with output discarded and a time limit, confined on macOS and otherwise only after a person approves the exact change; it must fail on the escaped merge and pass on the fix; every task an accept created is recorded for good, and the merge queue's entries and `merge_blocker` refuse it unless the escape's current check names it, is `proven` and was proven on the commit being queued, through one shared guard; the tick unwinds stuck and withdrawn checks; migration 52) and the possible-escape scan (PR 3, `scan.rs`: three signals from read-only git on the project checkout, writes `possible` only, bounded, idempotent, run by the service's own tick; `possible.rs` reads `GET /escapes/possible`, which never writes). PR 1: merges that got past review and checks and were later found defective. A revert is a **possible** escape at once, written by the schema (migration 51 back-fills it and a trigger on `crew_merges` adds it with the revert); confirmed only when a person confirms or marks it (PR 2). Possible, confirmed and dismissed are always separate counts. Display only: the breaker, Router and gates never read it. Views for `GET /api/crew/escapes`, `/escapes/{id}` and `/escapes/thresholds`; thresholds (60 d, 14 d, 10 merges, 2x) are device config in `settings`. Stored by `store/crew_escapes.rs`, whose read functions are the stable API for the backtest's `EscapeSource`. See [docs/crew/escapes.md](../../../../docs/crew/escapes.md) |
| `background` | Background crews' state: definition, strict validation, redaction, runs with spend against a budget set at creation, and the stop for the budget that queues a review. Stored in the data folder, audited, no trigger and no engine |
| `background_listeners` | The background events no person pressed (slice 4h, PR 3): one owned future, `run_crew_background_listeners`, the service opts in behind the Preview flag (an interval of 60s to 24h, `ORBYN_CREW_BACKGROUND_WATCH_INTERVAL_SECS`) and awaits on shutdown. Each pass fires what is due: a `schedule` crew once per `every_hours` bucket, and a `ci_failure` crew on a failed check run at its branch head (its setting, else the project default) narrowed by its optional workflow filter; `issue_label` and `dependency_update` crews have no listener. Every firing starts a run through the same seam as Run now (`crew_background_start`) with the event in the `PausedTrigger` key (`<crew>:schedule:<bucket>`, `<crew>:ci:<head>`), and claims a first-write-wins event row (migration 58) first, so a repeat of the same event starts nothing new and a refused firing (crew off, a run already running, no engine, capacity) is never retried on a timer. Event text from outside — a check's name, a head sha, a `.orbyn/crew.json` trigger's name — only selects what to start and reaches only the dedupe key, the trigger key and a diagnostic line; the brief stays code-owned (decision 6). A file trigger with no matching crew is ignored: nothing starts, nothing is adopted, at most the diagnostic line "found in .orbyn/crew.json, no matching crew", no UI |
| `background_start` | Run now (slice 4h, PR 1): a person starts one run of an enabled background crew by hand (the `manual` event, `Core::crew_background_run_now`). It makes the run's lead thread (the run's id is the thread's id, the `crew_id` of its tasks; no engine ever runs it and `tell_lead` skips it), pins that crew's Autopilot mode `off` so every merge of the run waits for a person, and starts the task through `launch_crew_task` as Crew-started and Low priority, so `capacity_start_check` can pause it (the refusal is a held `background` start with key `<crew>:manual`, and the run, thread and mode are taken back). The crew's turns and minutes are the brief's budget: minutes are the existing task timer, turns are the worker's runs, checked when a verification fails. A background worker's turn with no output fails the run (`NO_REPLY`, engine error) instead of being judged by what it leaves behind. A task's end ends the run: passed is `needs_review`; budget, repeated failure and engine error are `stopped` with that reason; anything else is `needs_review` with a plain sentence. The brief is code-owned per template (no event or repository text in it) and may write under the repository's top-level entries except `.orbyn/` and `.github/`; its checks are a file scope and a size cap, so a person reads every change. `issue_label` and `dependency_update` crews and crews that are off are refused. The listeners (PR 3) fire the same start with their own event key. No daily budget, no route yet |
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
