# Crew escape flywheel: backend design

Design note for the escape backend (beta item 4k, U2); no code. The screens exist as display-only Preview frames (`escape-*`,
Crew Designs v20, [crew.md](../crew.md) "slice 4f, UI part C"). Reverts are counted today (4j); escapes are not: `escapes` is `null`.

## What an escape is

A merge that got past review and checks and was later found defective. Four sources, from data the core already has. Every source
starts **possible**; **confirmed** only when a person confirms (the Settings frame counts a revert at once; the ruling overrides it):

| Source | Read from | Counts |
| --- | --- | --- |
| `revert` | `crew_merges.review_state = 'reverted'` (Crew's watch or the person's Revert) | **possible** at once, shown in the log and labelled so |
| `person_marked` | the person's "Mark as an escape" on a merge, or a review comment they promote | **possible**; the person's mark is the confirm, so it is confirmed at once |
| `post_merge_check` | `crew_merges.watch = 'broken'` on a merge that was **not** reverted (the forge watch sets this, `forge_watch.rs`) | only after a person confirms |
| `follow_up_fix` | a later commit on the target branch, see below | only after a person confirms |

A "reopened task" is not a source (`crew_tasks.state` has none); a person marks the merge instead.
## Possible-escape detection

Nothing here is confirmed until a person confirms; each signal fills the "Possible escape" card (`escape-possible`), one open candidate per merge.

1. **Revert of a merged crew task made outside the crew.** Reads `git log` of the target for a `Revert "..."` commit or `git revert`
   body naming a `merge_commit` we hold. False positives: a revert to bisect, or a re-land.
2. **Follow-up fix on the same files.** Reads the merge's file list (`before_commit..merge_commit`, from git; only `areas` is
   stored) and later commits within 14 days whose subject starts `fix` and that change an overlapping hunk.
   Highest false-positive risk of all: ordinary follow-up work, a typo, a rename. The card says why, as the design does.
3. **Post-merge check failure.** `watch = 'broken'` with `reverted_commit` null, from the forge watch's repository checks on
   the forge tip; the device's own checks only (forge CI status is Planned). False positives: a flaky check, a later merge's
   failure, a broken environment.

The scan runs after a merge, a revert and the watch's tick (at most 50 merges, 200 commits). It writes possible escapes only; a GET never writes; a dismissed commit is not offered again (unique merge, source, cause).

## Create a check

From a confirmed escape the person may ask Crew to propose a check. It is a **proposal**, never applied by itself:

- Crew suggests the gap: **test gap** (plain input and output) or **tooling gap** (a repo rule or tool step); the person can change it.
- **v1 only adds test files.** The brief is an ordinary crew task carrying the escape id, paths and what it asserts. It never
  edits `.orbyn/crew.json` or any policy file; the core refuses a diff that does. A tooling gap is logged with no proposal.
- **Proof before it is added** (the design's rule): it must fail on the escaped commit and pass on the revert or fix. If it
  passes on both, the row goes to `failed` and the person decides (`escape-check-invalid`).
- It merges through the **merge queue like any change**, with its own risk tier. **Authority is the lane rules' own:** the
  person confirms in the app (a `Person` route), workers hold no merge authority, N1 refuses the confirm path by name, lanes
  stay changeable only by `/review/{id}/remove-lane`, and Orbyn writes nothing into a repository itself (rule 4).

## Report

`GET /escapes/report` takes a project and 30, 60 or 90 days (default 60): merges, confirmed and possible escapes **counted separately** (reverted or not), gap
counts, checks added, being written and failed, and a per-week series, grouped by **engine and model**, **area**
(`risk::area_of`) and **lane**. A group under 10 merges returns `rate: null`, `low_sample: true`; a rate uses confirmed escapes only. An escape counts
against the engine, model and area copied from the merge **when logged**. A hard-deleted lead thread drops out, as for 4j.

## Screens to wire

All are `#/preview/crew/<frame>` in `web/src/features/crew/escape/`, on literal sample data with every button disabled;
`web/mock/` has no escape data. Frame ids: Crew Designs v20 in `orbyn-designs` (path in `docs/match-kit.md`).

| Frame | Needs |
| --- | --- |
| `escape-log`, `-empty`, `-low-sample`, `-error` | `GET /escapes`: totals, per-engine counts, rows, `as_of`, `low_sample`; a read error is 500 `unreadable` |
| `escape-possible` | `GET /escapes/possible`, `POST .../confirm`, `.../dismiss` |
| `escape-create-check`, `-tooling`, `escape-report` | the Review queue's merge row, `POST /escapes/mark`, `POST .../check` |
| `escape-detail`, `-detail-tooling`, `-check-invalid` | `GET /escapes/{id}`: events, the check and its proof results, counts against, area record |
| `escape-dashboard`, `escape-narrow-*` | `GET /escapes/report` by week, engine, area, lane (narrow: layout only) |
| `escape-route-counts`, `-route-shifted` | per-engine counts for the task's area (`GET /escapes/counts?project=&area=`); display only: the Router does not read them in v1 |
| `escape-settings` | `GET /escapes/thresholds` (the stored config below); "ask each time" stays fixed in v1 |

## Data model

**One additive migration**, numbered when it merges. Migration numbers are contiguous and checked by tests, so the first
migration PR to merge takes the next free number and any other renumbers at its merge (rebase, bump, rerun).

- `crew_escapes(id PK, project_id, merge_task_id UNIQUE REFERENCES crew_merges(task_id), source, state, gap, gap_by, engine,
  model, areas, lane, reason, found_unix, decided_unix, decided_by)`. `state`: `possible`, `confirmed`, `dismissed`.
  `source` and `gap` are checked enums. `reason` is at most 200 characters, redacted by the same scrub the ledger uses.
- `crew_escape_events` (append-only, UPDATE and DELETE refused by triggers, like `crew_area_events`): `detected`, `confirmed`,
  `dismissed`, `reclassified`, `check_proposed`, `check_failed`, `check_proven`, `check_merged`, `check_withdrawn`;
  unique `(escape_id, kind, cause)`. The audit row is written in the same transaction as the state change (rule 6).
- `crew_escape_checks(escape_id PK, path, task_id, state, proof)`; `proof` is JSON: commit ids and two booleans.

**Stored:** ids, commit ids, paths, enums, areas, engine and model names, one short reason. **Never stored:** diffs, test bodies,
command output, prompts, secrets or comment text. On this device only (rule 4). Limits: 5,000 escapes and 20,000 events per
project; oldest `dismissed` and `possible` rows go first, `confirmed` never. Unreadable fails closed (rule 7).

## HTTP and route classes

All under `/api/crew`, Preview-gated; GETs in `crew::changes::READS`, writes in `STATE_CHANGING` with a `GUARDS` entry. Classes
follow [worker-token-isolation.md](../worker-token-isolation.md), not built: `local_only` today, `Person` when the key lands.

| Route | Class |
| --- | --- |
| `GET /escapes`, `/escapes/{id}`, `/escapes/possible`, `/escapes/report`, `/escapes/counts`, `/escapes/thresholds` | Local |
| `PUT /escapes/thresholds` (the config) | Person |
| `POST /escapes/mark`, `/escapes/{id}/confirm`, `/dismiss`, `/gap` | Person |
| `POST /escapes/{id}/check` (propose), `/check/accept`, `/check/retry`, `/check/withdraw` | Person |

Each write takes a `seen` token (a mismatch is `409 stale`, as areas do) and answers with the escape as it now stands.
`check/accept` is the only route that starts a worker.

## PR plan

1. **Store and revert log.** Migration, tables, `revert` source (back-filled as possible), `GET /escapes`, `/{id}`, `/thresholds`.
   Tests: migration leaves older tables alone; a revert makes one possible escape, idempotent; immutable events; unreadable fails closed.
2. **Person acts.** `mark`, `confirm`, `dismiss`, `gap`, audit first. Tests: `409 stale`; refused without person transport;
   N1 refuses an agent; confirmed counts change only after confirm.
3. **Possible-escape scan.** Three signals, `GET /escapes/possible`. Tests: temporary git repos per signal and per false
   positive above; bounds; dismissed not re-offered; a GET writes nothing.
4. **Create a check.** Brief, proof, merge-queue path, `failed` state. Tests: passes-on-both refused; no lane removed; fake engine.
5. **Report and screens.** `/report`, `/counts`, web client, bindings, mock, the 17 frames on real data. Tests: week buckets,
   low-sample boundary, web tests per frame state. Counts into the 4j breaker or Router tie-break are **not** in this plan.

## As built: PR 1, store and revert log

Migration **51** (`store/crew_escapes.sql`), numbered when it opened; a later merge renumbers (rebase, bump, rerun). Read-only: no route writes (the person's writes are PR 2, below).

- **Tables.** `crew_escapes` as designed, less `crew_escape_checks` (PR 4). Table checks: `source`, `state`, `gap`, `gap_by`, `decided_by` are enums;
  `reason` is at most 200 characters; a possible row has no decision and a confirmed row is decided by `person`; one row per merge.
  `crew_escape_events` is append-only (UPDATE and DELETE refused by triggers, like `crew_area_events`), unique `(escape_id, kind, cause)`, no
  foreign key to its escape so the trail outlives a removed merge. `model` and `lane` are NULL until PR 2 and the scan record them.
- **The `revert` source** is written by the schema: the migration back-fills `crew_merges.review_state = 'reverted'` as **possible** (cause
  `revert:<commit>`), and a trigger on `crew_merges` does the same, in the revert's own transaction, for every later revert. Idempotent: the merge
  and the event are unique, so repeating a revert, or a dismissed escape, adds nothing. A merge whose lead thread is gone has no project and
  drops out, as for 4j. A failure to log never undoes a revert (`ON CONFLICT DO NOTHING`; any other refusal still errors).
- **Routes** (Preview-gated, class **Local**, in `READS`; writes are PR 2, as Person): `GET /api/crew/escapes?project=&state=&limit=&offset=`
  (counts for the report window by engine, possible, confirmed and dismissed apart, no rate, `low_sample`; one page of rows, newest found first),
  `GET /api/crew/escapes/{id}` (the escape and its trail, at most 100 rows) and `GET /api/crew/escapes/thresholds`. A damaged log is `500 unreadable`.
- **Thresholds** are one JSON document in the existing `settings` table under `crew_escape_thresholds` (not a `pref:` key, so the account sync never
  carries it): `report_days` 60, `follow_up_days` 14, `min_merges` 10, `rate_multiple` 2; `stored` says whether a person's values are in force.
  Absent means defaults; present but unparsable, unknown-field or out of range is `unreadable`, never the defaults.
- **Limits not built.** The 5,000-escape and 20,000-event pruning is still not built (PR 2's writer refuses to pass either limit instead); `DELETE` on events is refused by trigger, so pruning needs its own removal path. `crew_merges.areas` is `CHECK(json_valid)`, so the revert trigger cannot see invalid JSON; valid JSON that is not a list of strings makes reads of that row `unreadable`.
- **Display only.** Nothing reads the log for a decision: the breaker, the Router and every gate ignore it (decision 2). Test-covered for the breaker.

## As built: PR 2, person acts

No migration. `store/crew_escape_person.rs` (the writes), `crew/escapes/person.rs` (core API), four routes in `routes/crew_escapes.rs`.

| Route | Does |
| --- | --- |
| `POST /api/crew/escapes/{id}/confirm` | `possible` to `confirmed`. The only way an escape becomes confirmed besides a mark. |
| `POST /api/crew/escapes/{id}/dismiss` | `possible` to `dismissed`. Not offered again. |
| `POST /api/crew/escapes/mark` | The merge becomes a confirmed escape at once (`person_marked`). A merge with a possible escape is confirmed (source stays); confirmed is returned as it is; dismissed is refused. |
| `POST /api/crew/escapes/{id}/gap` | Sets `test_gap` or `tooling_gap` (`gap_by` person) on a possible or confirmed escape. Not on a dismissed one. |

- **Bodies.** `project` (an escape or merge of another project is `404`), `seen` (the state and gap the person saw; a mismatch is `409 stale`),
  and for confirm, dismiss and mark an optional `note` (at most 200 characters, redacted, kept in the audit row; a mark's note is also the new
  escape's reason). Each answers with the escape as it now stands (`EscapeRow`). The existing read shapes are unchanged.
- **Transitions.** `possible` to `confirmed` or `dismissed`; both are final. The same act on an escape already in that state answers with it
  unchanged and writes nothing (a retry after a lost answer, whatever `seen` was). The opposite act is `400` in plain words ("This escape is already
  dismissed, so it cannot be confirmed now."), after the stale check. There is no undo in v1; a wrong decision is for the owner to rule on.
- **Audit first, one transaction** (rule 6): the ledger's audit row (`crew.escape.confirm`, `.dismiss`, `.mark`, `.gap`; who `user`, when, the
  note), then the state change, then the `crew_escape_events` row (`confirmed`, `dismissed`, `reclassified` for a gap; a mark writes `detected` then
  `confirmed`) whose `cause` is `person:<audit id>`. The state is read and compared inside the immediate transaction and the `UPDATE` repeats the
  old state, so two acts racing leave one decision. A mark that would pass 5,000 escapes, or any act that would pass 20,000 events, is refused.
- **Authority.** `Person` and enforced (`routes/classes.rs`, `tests/routes.json`, `web/src/api/personKey.ts`): browser mode and the CLI get
  `403 desktop_required`, a remote viewer `403 local_only`, the flag off `404`. All four are in `crew::changes::STATE_CHANGING`, so an agent's
  request is refused by path (N1); no MCP tool, CLI command, timer or scan calls them. Nothing confirms by itself: detection, the revert trigger
  and every later PR write `possible` only. A confirm feeds no breaker, Router or gate (decision 2); a test checks the area mode is unchanged.
- **Choices where the note is silent:** state and action names are the note's (`possible`, `confirmed`, `dismissed`; `mark`, `confirm`, `dismiss`,
  `gap`); `seen` is the escape's state and gap; the gap is built here because the PR plan lists it; confirmed and dismissed are final.

### The stable read API (for the backtest's `EscapeSource`, slice 4n)

On `Store` (`store/crew_escapes.rs`), over `EscapeRecord` and `EscapeEventRecord`; all read-only and all-or-nothing (`CoreError::Unreadable`):

| Function | Returns |
| --- | --- |
| `crew_escapes_of_project(project_id, merged_since)` | every escape of a project whose merge reached the branch at or after `merged_since` (Unix s), oldest merge first, more than 5,000 is `Unreadable` (narrow `merged_since`), never a shortened list |
| `crew_escape_of_merge(task_id)` | the escape of one merge, if any |
| `crew_escape(id)`, `crew_escape_events(id, limit)` | one escape; its trail, oldest first |
| `crew_escape_page(project, state, limit, offset)` | one page, newest found first, and the total |
| `crew_escape_tallies(project, merged_since)` | per engine: merges, possible, confirmed, dismissed |

`EscapeRecord` carries `merge_task_id` and `merged_unix`, never a PR number or a file list (diffs are not stored): an `EscapeSource` resolves the
PR from the merge's forge record and the files from git. **Count `state = confirmed` as an escape and keep `possible` apart** (decision 1); a rate
needs at least `min_merges` merges (`crew::escapes::escape_thresholds`). Field and enum names are stable; new columns are additive.

## As built: PR 3, possible-escape scan

No migration (the PR 1 tables already hold `post_merge_check` and `follow_up_fix`). `crew/escapes/scan.rs` (the scan), `crew/escapes/possible.rs` (the read),
`store/crew_escape_scan.rs` (candidates and the one writer), one route in `routes/crew_escapes.rs`, one contract (`EscapePossible`).

- **`GET /api/crew/escapes/possible?project=&limit=&offset=`** (Preview-gated, class **Local**, in `READS`): `{ project, as_of, total, rows }`, the project's `possible`
  escapes, newest found first, the same `EscapeRow` as the log (`line` and `reason` say why). A confirmed or dismissed escape is not listed. It reads only; it
  never runs the scan, so a GET writes nothing. The existing read shapes are unchanged.
- **Writes `possible` only.** The scan writes `state = 'possible'` with decided by nobody and one `detected` event (`decided_by` core, `cause` the evidence: `revert:<commit>`,
  `follow_up_fix:<commit>`, `post_merge_check:<merge commit>`). It cannot write `person_marked` or a confirm; the store refuses it. One escape per merge: the first signal
  that fires keeps the row, in the order 1, 3, 2 below. `model` and `lane` stay empty (the merge does not record them).
- **Signals** (the design's three):
  1. *Revert outside the crew.* A later commit on the merge's branch whose message names the merge commit and is a revert (`Revert ` subject or `This reverts commit <merge>`).
     Not offered when that revert was itself reverted or reapplied (a re-land) or its message says `bisect`. The crew's own revert already has its row from the schema.
  2. *Follow-up fix.* A later non-merge commit within `follow_up_days` (the stored threshold, 14) of the merge whose subject starts with the word `fix`, `fixes` or `fixed`
     (not `fixture`), not a revert, on a file the merge changed, and that changes a **line the merge added**: `git blame` at the fix's parent names a commit the merge brought in.
     A fix next to those lines (a pure insertion adds no old line), on other lines, on another file, called something else, or after the window is not offered. The reason says it may be ordinary follow-up work.
  3. *Post-merge check.* `watch = broken`, not reverted: the device's own repository checks only (forge CI status stays Planned). No git.
- **Candidates.** Merges of a project with **no escape row of any state**, made within the report window (60 days, stored), newest first. A dismissed escape therefore is never a
  candidate and the writer's `INSERT OR IGNORE` on the unique merge would keep it dismissed anyway; a repeat scan adds nothing.
- **Git, read-only.** Hardened git on the merge's own checkout (`merge-base --is-ancestor`, `log`, `diff`, `rev-list`, `blame`): no worker, no engine, no network, no repository write,
  no filter or diff converter. A missing repository, a merge no longer on its branch, or an unreadable object is **skipped** (counted, never guessed).
- **When it runs.** The doc says after a merge, a revert and the watch's tick. Those hooks sit in the merge code, which this PR does not touch, so the scan is a **bounded
  background step of its own**: `Core::run_crew_escape_scan`, started by the service (Preview only) once at start and then on every Crew watch interval
  (`ORBYN_CREW_WATCH_INTERVAL_SECS`; none when that is off). No route, tool or CLI command runs it. A later PR may also call `crew_escape_scan` from the merge and revert paths.
- **Bounds.** Per scan: 50 merges, 200 later commits per merge, 300 git calls, 60 seconds. Each git call is cut at the scan's deadline and its output is capped (1 MiB for the log, name lists and blame, 256 KiB for a diff). A capped `diff-tree`, `diff` or `blame` skips only that fix or file; a capped `log`, name list or `rev-list`, or an unreadable repository, skips the merge: it is counted as skipped, never half read. stdout and stderr are read together, so a chatty stderr cannot stall a call. The log is NUL-separated and every parsed commit id is checked against `rev-list <merge>..refs/heads/<branch>`, so a commit message cannot forge an entry; paths are literal pathspecs; the branch is read as `refs/heads/<branch>` (a tag of the same name cannot stand in).
  **Rotation.** A merge with no signal never gets a row, so the scan keeps a cursor (the merge time and id of the last merge passed over) in `settings` under `crew_escape_scan_cursor`, device config like the thresholds; a damaged value reads as none and is overwritten. The next scan resumes after it and wraps to the newest at the end, so every merge in the window is reached over successive scans. The cursor is written after the escape rows. A batch the writer refuses (it would pass 5,000 escapes or 20,000 events) is treated like a project that was full before git ran: counted as `refused`, passed over, and retried when the scan wraps. A merge that alone cannot be read inside the scan's time (or whose log is always over its cap) is counted as skipped and passed over, so no one merge pins the scan. A merge skipped this way is not scanned for its signals until it can be read.
  A project at 5,000 escapes or 20,000 events is refused whole before any git runs, and a batch that would pass either limit is refused whole by the writer: nothing is
  partly written, the scan reports `refused` and logs a plain sentence. Pruning is still not built.
- **Choices where the note is silent:** the order of signals when several fire, the bisect and re-land filters, blame (not a line-number overlap) as the meaning of "overlapping
  hunk", the 60-day candidate window, the own-tick runner instead of merge hooks, and `post_merge_check`'s cause being the merge commit.
- **Known limit.** The schema's revert trigger records a crew revert only for a merge with no escape row. If the scan already logged a `follow_up_fix` or `post_merge_check` for a merge, a later crew revert of it does not add a `revert` source or event to that escape (one escape per merge); the merge row still says `reverted`, and a person decides the escape as usual.
- **Not built.** A scan from the merge and revert paths; per-merge model and lane; any trigger from a route (a person's "scan now" is not in the route plan).

## As built: PR 4, create a check

Migration **52** (`store/crew_escape_checks.sql`, the next free number when it opened; a later merge renumbers: rebase, bump, rerun). `crew_escape_checks(escape_id PK, project_id, path, asserts,
run, fix_commit, task_id, state, proof, note, attempts, diff_commit, diff_hash, diff_paths, approved_commit, approved_hash, proposed_unix, updated_unix)`: at most one check per escape. `crew_escape_check_tasks(task_id PK, escape_id, created_unix)` records **every task an accept ever created**, in the accept transaction, and is never deleted or cleared (a retry clears `task_id`, a new proposal after a withdraw replaces the check row). The
`check_*` event kinds were already in migration 51, so no kind is added. `store/crew_escape_checks.rs` (the writes), `crew/escapes/check.rs` (core API, the proof, the queue),
`check_brief.rs`, `check_paths.rs`, `check_proof.rs` (the scratch tree and the confined run), `host/confine` (`wrap_proof`: the proof's sandbox profile), five routes in `routes/crew_escapes.rs`.

| Route | Does |
| --- | --- |
| `POST /api/crew/escapes/{id}/check` | **Propose.** A confirmed escape with a test gap (or no gap yet) gets a proposal: one new test file `path`, what it `asserts`, the `run` command, and the commit it must pass on. Writes no task and starts nothing. |
| `POST /api/crew/escapes/{id}/check/accept` | **The only route that starts a worker.** `proposed` to `writing`, then the crew task. The proposal is read again inside the transaction (a changed path, assertion, command or fix commit is `409 stale`; an escape that became a tooling gap is `400`). A withdrawn check starts no worker. |
| `POST /api/crew/escapes/{id}/check/approve` | **A person approves the exact change the proof will run**, where this machine cannot confine the proof. Body: `project`, `seen`, `commit` (40 hex) and `diff_hash` (64 hex). |
| `POST /api/crew/escapes/{id}/check/retry` | `failed` back to `proposed` (a new round). Starts nothing: a person accepts again. |
| `POST /api/crew/escapes/{id}/check/withdraw` | `proposed`, `writing`, `failed` or `proven` to `withdrawn`. Cancels a worker still writing, stops a proof still running, turns down a queued merge of it. A writing check whose task is missing, ended or passed can be withdrawn. |

- **States.** `proposed`, `writing`, `proven`, `failed`, `withdrawn`, `merged`. `failed` covers a worker that stopped or was cancelled, a diff that was not exactly the one proposed test file, a proof
  that did not hold (including a check that passes on both), and a start that could not be made once a task existed. `withdrawn` and `merged` are final; a withdrawn check may be proposed again.
  A `writing` check whose change is read and recorded (`diff_commit` set) and not yet approved is **waiting for a person's approval**; no new state, so the migration's CHECK is unchanged.
- **Authority.** Person and enforced (`routes/classes.rs`, `tests/routes.json`, `web/src/api/personKey.ts`), in `crew::changes::STATE_CHANGING` with a `GUARDS` entry, local only, Crew
  Preview gated: browser mode and the CLI get `403 desktop_required`, a remote viewer `403 local_only`, the flag off `404`. No MCP tool, CLI command, scan or timer calls propose, accept,
  approve, retry or withdraw. Each takes `project` and `seen` (`{state, gap, check}`: the escape's state and gap and the check's state); a mismatch is `409 stale`, checked before git is read.
  Each answers `{escape, check}` (`EscapeCheckAnswer`). The same act on a check already in that state answers with it and writes nothing; the opposite act is `400` in plain words.
- **Audit first, one transaction** (rule 6): the ledger row (`crew.escape.check.propose`, `.accept`, `.approve`, `.retry`, `.withdraw`), then the state, then the event where the design lists a kind:
  `check_proposed` (a retry writes it again, cause `retry:<audit id>`), `check_withdrawn` (person), and from the core `check_failed`, `check_proven`, `check_merged` (cause `core:<task id>:<round>`, the round being how many `check_proposed` events the trail holds). Every cause is its own round's, and an event that meets the same escape, kind and cause is already recorded (`ON CONFLICT DO NOTHING`; any other refusal still errors), never an error to the person.
  Accept and approve have no event kind of their own (migration 51 fixed the list), so their record is the audit row (the approval's names the task, commit, hash and paths) and `attempts`. A start
  refused before any task exists writes `crew.escape.check.start_failed` and puts the check back to `proposed` with the reason, so one more accept retries it.
- **The brief** (`check_brief.rs`) is an ordinary crew task written on the lead's behalf and started through `launch_crew_task`, the entry `crew_start` and the review fixes use, so the router
  (refusals, the same-provider default of #675), the capacity gate, the secret and leak checks and the lease all apply. It carries the escape id, the one path (a write lease on exactly that
  file), what the test asserts, how it is run and the two commit ids, and tells the worker to commit. It carries no diff, test body or command output. The key (never sent) has one `file_scope`
  check on that path and protects `.orbyn/crew.json`. The worker is not given a lane, and no lane is removed or changed (`remove-lane` stays the only way).
- **Only one test file** (`check_paths.rs`). The design named no globs, so these are the rules. The path is relative, ASCII only (a non-ASCII name is refused, never normalised), at most 200 bytes, with no
  `..`, no hidden segment (nothing starting with `.`), no segment ending in a dot or a space, no Windows reserved name (`con`, `prn`, `aux`, `nul`, `com1`-`com9`, `lpt1`-`lpt9`, with or without an extension),
  no Windows 8.3 short name (`PROGRA~1`), and no backslash, glob character, colon, quote, `$` (which also refuses `CONIN$` and `CONOUT$`) or shell metacharacter. It is not a config, manifest or build file, **even inside a test folder** (lock files, `go.mod`, `CMakeLists.txt`, `pom.xml`, `Dockerfile`, `BUILD`,
  `WORKSPACE`, `rust-toolchain*`, `build.gradle*`, `tsconfig*.json`, the babel, jest, vitest, vite, cypress, playwright and karma configs, `vitest.workspace.*`, `docker-compose*`, `Gemfile`, `Rakefile`, `requirements*.txt`, `*.csproj` and the other .NET project files, `Directory.Build.props`, `global.json`, `deno.json`, `bunfig.toml`, `meson.build`, `BUCK`, `sitecustomize.py`, the CI folders and the manifests that define what a check does).
  **`conftest.py` is policy**: pytest runs it on collection, so it can run anything, and the check is refused. The path must be in a `tests`, `test`, `__tests__`, `spec`, `specs` or `e2e` folder or named
  `test_x`, `x.test.y`, `x.spec.y`, `x_test.y` or `x_spec.y`. When the task passes, the core reads the worker's committed change from git (`git diff --raw -z --no-renames --no-abbrev` against the commit the worker
  started from) and the change must be **exactly one added regular file, the proposed path**. A modified, deleted, renamed, link or submodule path, a second file or a different path makes the check `failed` and the
  proof never runs. There is no "few files" allowance: one file, so the approval below is easy to read. This is a second line after the key's `file_scope`.
- **The proof** runs the person's `run` command twice: on the escaped merge (`crew_merges.merge_commit`, which must be hex; it must fail) and on the fix (it must pass). The fix is the commit the body names, else the
  revert the merge recorded, else the commit the escape's detection named; an escape with none needs the person to name one. Only both together prove it. Passing on both is refused (`failed`, "passes on the
  escaped merge too"); failing on the fix too and the wrong way round are refused with their own words; a command that could not run (did not start, ran out of time, stopped) is `failed` with no verdict on the
  test. The proof stores two commit ids and two booleans, never output.
  - **Where it runs.** Each run is in a fresh scratch folder under the device's data folder (`crew-escape-proof/<id>`) with its own `tree` (made from `git archive` of that commit, so the person's `.git` (config,
    hooks, other branches) is not there, and any symlink or hard link in the commit is left out), `home` and `tmp`. The one test file is laid over the tree without following any link (each folder on the way is checked
    with `symlink_metadata`; a link or a file where a folder belongs is refused). The folder is removed after each run, and the tick removes any a crash left behind.
  - **What the command gets.** Its output and input are discarded. It has a time limit and stops at once when the check is withdrawn. Its whole process tree is reaped at the deadline, on a withdraw and on drop, a `setsid` child included (macOS and Linux: the descendants are read from the process table while the leader is unreaped, recorded with their start time, and killed on drop; Windows: the job object). Limits (#704 stays open for them): a descendant already orphaned to `init` before any snapshot (a double fork whose middle process has exited, or a leader that exits within the 100 ms between two records), one forked after the last freeze round, and, for `terminate`, a child forked between the snapshot and the signal; closing them needs a container such as a cgroup or a launchd job. Where the process table cannot be read (`/proc` missing or refused, `libproc` refusing, another Unix) only the process group is killed. Only the proof records descendants while it runs, and `terminate` records those of the tree it is called on; a worker whose leader exits normally and is then dropped gets the group kill, as before. A terminal tab keeps the group-only kill. A credential folder too large to check for links makes the proof unconfined (a person approves it). It runs off the tick's critical path. Its environment is cleared and set to
    exactly `PATH`, `HOME` (the scratch `home`), `TMPDIR` (the scratch `tmp`), `LANG` and `CI=1` (plus what Windows needs to start a program): no token, key, session marker or account variable is inherited.
    The person's `run` command is arbitrary code the person wrote; apart from the restriction below it runs as the person.
  - **Confinement, or an approval.** Where the machine can confine it (macOS `sandbox-exec`) the proof runs under **the profile a Crew worker gets** (`host/confine/macos.rs`, so every worker rule is inherited: the data
    folder denied, Claude's saved conversations and history denied, `(deny process-info* (target others))` so another program's command line and environment cannot be read, no task port, Apple events,
    LaunchServices or signals to others), and in addition: **no network**, **no launchd job** (`launchctl submit`), **no writing of any application's preferences** (`defaults write`, so another app's settings cannot be changed to start something outside the sandbox later), **no clipboard**, **no reading of the person's credential folders** (`~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.config/gh`, `~/.netrc`, `~/.docker`, `~/.kube`, `Library/Keychains`; each denied by its path and by its real path when it is a link, such as a dotfiles `~/.ssh`, under `HOME` and under the account's own home, and at the target of a link inside it; the toolchain folders such as `~/.cargo` stay readable; with `HOME` unset or unresolvable the proof counts as not confined and waits for a person's approval), and **file writes only inside its own scratch folder and `/dev/null`** (the shared
    `/private/tmp` and `/private/var/folders` are not writable; the test has its own `TMPDIR`). The data-folder denial comes last, with the scratch folder opened again after it. It can still **read** what the person's account
    can read outside Orbyn's data folder, Claude's saved conversations and the credential folders above. **This is a mitigation, not a boundary:** it is the broad macOS sandbox (everything not named
    is allowed), so system services other than the ones named stay reachable from the proof, and the accept text says so. Such a run needs no approval. Anywhere else (Linux, Windows, a Mac where `sandbox-exec` is
    unavailable) it would run with the person's own access, so **it never runs until a person approves the exact change**: the core records the commit and a SHA-256 over the sorted paths and bytes of the worker's
    change (`diff_commit`, `diff_hash`, `diff_paths`), the check stays `writing`, and the check view (`awaiting_approval`, `diff_paths`, the hash) shows the file and the hash. `check/approve` with that commit and hash
    writes the audit row first and lets the proof run. A different commit or hash is `409 stale`; a new change from the worker drops an earlier approval. The accept text says in plain words what runs, where, under what
    confinement, what it can write and read, and when an approval is needed. A non-test build ignores the test-only switch that says a world is not confinable.
  - **A flaky test can be proven by chance.** One run on each commit cannot tell a test that fails on the escape because of the bug from one that fails by luck or by the machine. Nothing
    enforces a human read of the test: with autopilot on, the only guard is that a check's task can be queued only once its check is `proven` (the guard below), and a proven check can still be a luckily passing one. The
    proof is evidence, not a verdict.
- **The merge queue.** A proven check is queued through `crew_merge_request` (the Review queue merge route's own entry), on the escaped merge's branch, so its own risk tier, holds, owed lanes and the person's
  required CI policy apply as for any change. The core queues only if the check is still `proven`, reads it again just before, and after the entry exists rejects it if the check was withdrawn meanwhile. When the
  queue refuses (no CI names set, no remote) the check stays `proven` with the reason and a person queues it from the Review queue. Nothing here merges, releases or authorises a merge.
- **The guard: no unproven check merges.** One shared helper, `Core::refuse_unproven_escape_check(task)` over the pure `escape_check_blocks_queue(task, recorded, check)`, refuses a task an accept ever created for a check
  (`crew_escape_check_tasks`) **unless the escape's current check names that same task and is `proven`**, with a plain reason, and **fails closed** (a record or check that cannot be read refuses). So a retried check
  (its task cleared), a withdrawn and proposed-again check (its row replaced), a failed or still-writing check, and an earlier attempt's task are all refused; only the current proven task passes. It is one line in each
  of the three entries into the merge queue, with no other change to queue code: `crew_queue_merge` (`autopilot/merge/queue.rs`, the lead's tool and the queue entry), `crew_merge_target` (`autopilot/merge/tool.rs`,
  which `crew_merge_request` calls, the Review queue's route) and `crew_queue_forge_merge` (`autopilot/merge/forge.rs`), and it is also part of `merge_blocker` (`holds.rs`), which both `crew_process_merges` and
  `process_forge_entry` call, so a task whose check stopped being proven after it was queued is blocked when processed. Paths checked for a bypass (by searching every non-test caller): the lead's MCP tool
  `crew_queue_merge` goes `crew_merge_target`, then `crew_queue_forge_merge`, then `crew_queue_merge`; the Review queue's `POST /crew/tasks/{id}/merge` calls `crew_merge_request`, which calls the same two; the escape check's
  own `escape_check_queue` calls `crew_merge_request`; and the only non-test caller of `add_merge_entry` is inside `crew_queue_merge`, after the guard. `process`, `watch` and `release` act on entries that exist and
  pass `merge_blocker`. Tests cover the lead tool, the forge entry, the Review queue route and the queue entry against writing, failed, withdrawn, retried and re-proposed checks (old task refused, current proven task
  accepted) and a damaged row or record.
- **Withdraw and a merge that is landing.** A withdraw is refused, with no state change and in plain words ("the merge is landing; withdraw after it ends"), when the check's queue entry is `merging` or its forge intent is
  `merge_requested` or `merged`; that read is inside the withdraw's transaction. The merge side re-reads the guard right after it writes `merging` (local queue) or saves `merge_requested` (forge) and undoes the step
  if the check is no longer proven, so exactly one order can happen and a withdrawn check never lands. A queue entry a crash left `merging` is settled at startup from git (docs/crew.md, "A local merge a crash interrupted"), so it
  does not hold the refusal for good; while a merge is live in the process the refusal stands. A withdraw that committed does not report an error for a queue entry that is already merged or out of the queue:
  the answer's `note` says so ("Already merged: the merge could not be stopped").
- **The tick** (`run_crew_escape_scan` calls `crew_escape_check_reconcile`, off its critical path, cancelled with the service): it records `merged` once the merge exists, finishes a `writing` check whose task ended
  while the core was down, **unwinds a `writing` check whose task never existed** (the process died between accept and the start) back to `proposed` with a plain reason, **turns down an open queue entry of any task
  whose check is no longer proven for it (withdrawn, failed, proposed again or gone)**, and removes scratch trees a crash left.
- **Read API.** `GET /escapes/{id}` gains an optional `check` (additive; absent for an escape with no check) with the diff, its hash and paths, and whether it awaits or has an approval. The other read shapes are unchanged.
- **Choices where the note is silent:** the proof command comes from the person (the repository has no test selector the core can trust); the test-file rules above; one file only; `conftest.py` is policy; at most one
  check per escape; a tooling gap is refused with no proposal and no row; only a confirmed escape can have a check; retry never starts a worker; approval is a state of `writing`, not a new state; confirmed escapes still
  feed no breaker or router.
- **Limits.** The proof lays one file over a commit, so a test that needs the worker's source changes to compile fails on both and is refused. The test environment's fake worker works in the project's own checkout, so the
  tests reset it between attempts; a real worker has its own worktree. Off macOS there is no confinement: the approval is the control.

## As built: PR 5, web screens

No migration, no route, no read field and no write: the web binds the reads and person acts PR 1 to PR 4 already serve (5a, 5b, 5c).

- **Live (real reads).** `escape-log`, `-empty`, `-low-sample`, `-error`, `-possible`, `-detail`, `-detail-tooling`,
  `-confirmed`, `-dismissed`, `-gap` (5a) on `GET /escapes`, `/escapes/possible`, `/escapes/{id}`; the eight
  `escape-check-*` frames and `escape-limit` (5b) on the same detail read and the person's check routes; `escape-settings`
  on `GET /escapes/thresholds` (read only), and `escape-narrow-window` / `-narrow-panel`, which are the log at 640 px and 320 px
  (5c). A check's retry, withdraw and approve are that page with its dialog opened by a click.
- **Still drawn samples.** `escape-report` (the mark dialog: its figures need the report read and it opens from a Review item's
  menu, which the beta has no queue for), `escape-dashboard`, `escape-route-counts` and `-shifted` (the report and counts routes
  are not built; every figure is Not measured, never a zero), and `escape-create-check*` (the create form is a later batch).
  They are drawn at v79 with Planned pills, and `escape-create-check*` stays at v20.
- **Writes stay a person's.** Every act is a person's click carrying `seen`; a stale answer shows the service's own words and
  reloads, and drawing a screen never writes. Settings save stays Planned (the thresholds read is bound; no write).
- **Derived, not invented.** The log's limit banner appears only when the unfiltered read's total reaches the store's 5,000 cap,
  and its heading names the count from the read; the narrow panel draws the frame's first four cards, the narrow window the page's
  rows. Rate, gap and check counts read Not measured while the report route does not exist.
- **Not built.** No report or counts backend; a merged escape's "Merged to main" timeline row and an as-of line are absent because
  no read carries them; the create-check form is a drawn sample.

## Decisions (owner)

1. A revert is a possible escape at once, labelled so; confirmed only by a person. Counts are shown separately.
2. v1: confirmed escapes do not feed the 4j breaker or Router tie-break (display and checks only); revisit after real runs.
3. v1: post-merge failure is the device repository's own checks only. Forge CI status is Planned.
4. v1: a created check only adds test files, never policy files. Constants (60 d, 14 d, 10 merges, 2x) are defaults stored as
   device config (existing scoped settings, no new table).
