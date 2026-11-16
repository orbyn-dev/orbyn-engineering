# Crew escape flywheel: backend design

Design note for the escape backend (beta item 4k, U2). It changes no code. The screens exist as display-only Preview frames
(`escape-*`, Crew Designs v20, see [crew.md](../crew.md) "slice 4f, UI part C"); this note says what feeds them.
Reverts are counted today (slice 4j); escapes are **not measured**: `escapes` is `null` everywhere.

## What an escape is

A merge that got past review and checks and was later found defective. Four sources, from data the core already has:

| Source | Read from | Counts |
| --- | --- | --- |
| `revert` | `crew_merges.review_state = 'reverted'` (Crew's watch or the person's Revert) | at once, as the design's Settings says; a person can mark **Not an escape** |
| `person_marked` | the person's "Mark as an escape" on a merge, or a review comment they promote | at once (it is the person's act) |
| `post_merge_check` | `crew_merges.watch = 'broken'` on a merge that was **not** reverted (the forge watch sets this, `forge_watch.rs`) | only after a person confirms |
| `follow_up_fix` | a later commit on the target branch, see below | only after a person confirms |A "reopened task" is not a source (`crew_tasks.state` has none); a person marks the merge instead.

## Possible-escape detection

Nothing here counts until a person confirms; each signal fills the "Possible escape" card (`escape-possible`), one open candidate per merge.

1. **Revert of a merged crew task made outside the crew.** Reads `git log` of the target for a `Revert "..."` commit or `git revert`
   body naming a `merge_commit` we hold. False positives: a revert to bisect, or a re-land.
2. **Follow-up fix on the same files.** Reads the merge's file list (`before_commit..merge_commit`, from git; only `areas` is
   stored) and later commits within 14 days (Proposed) whose subject starts `fix` and that change an overlapping hunk.
   Highest false-positive risk of all: ordinary follow-up work, a typo, a rename. The card says why, as the design does.
3. **Post-merge check failure.** `watch = 'broken'` with `reverted_commit` null, from the forge watch's repository checks on
   the forge tip. False positives: a flaky check, a failure caused by a later merge, or a broken environment.
   No forge CI status is read today; this is the repository's checks run on this device.

The scan runs after a merge is recorded, after a revert and on the watch's tick, bounded to 50 merges and 200 commits per run.
It writes candidates, never counts; a GET never writes; a dismissed commit is not offered again (unique merge, source, cause).

## Create a check

From a confirmed escape the person may ask Crew to propose a check. It is a **proposal**, never applied by itself:

- Crew suggests the gap: **test gap** (plain input and output) or **tooling gap** (a repo rule or tool step). The person can change it.
- The proposal is a brief for an ordinary crew task: a new test file, or an edit to `.orbyn/crew.json` (`checks`, or a `lanes`
  entry with `when`). It carries the escape id, paths and a one-line "what it asserts".
- **Proof before it is added** (the design's rule): it must fail on the escaped commit and pass on the revert or fix. If it
  passes on both, the row goes to `failed` and the person decides (`escape-check-invalid`).
- It merges through the **merge queue like any change**, with its own risk tier. Rules only add or tighten, so a check is
  only an addition; removing a lane stays the person-only `/review/{id}/remove-lane`. **Authority is the lane rules' own:**
  the person confirms in the app (a `Person` route), workers hold no merge authority, N1 refuses the confirm path by name,
  and Orbyn writes nothing into a repository itself (rule 4). `crew.json` is read at the base commit: it applies to later tasks.

## Report

`GET /escapes/report` takes a project and 30, 60 or 90 days (default 60, Proposed): merges, escapes (reverted or not), gap
counts, checks added, being written and failed, and a per-week series, grouped by **engine and model**, **area**
(`risk::area_of`) and **lane**. A group under 10 merges returns `rate: null`, `low_sample: true` (Proposed). An escape counts
against the engine, model and area copied from the merge **when logged**. A hard-deleted lead thread drops out, as for 4j.

## Screens to wire

All are `#/preview/crew/<frame>` in `web/src/features/crew/escape/`, drawn with literal sample data and every button disabled;
`web/mock/` has no escape data yet. Frame ids are Crew Designs v20 in `orbyn-designs` (path in `docs/match-kit.md`).

| Frame | Needs |
| --- | --- |
| `escape-log`, `-empty`, `-low-sample`, `-error` | `GET /escapes`: totals, per-engine counts, rows, `as_of`, `low_sample`; a read error is 500 `unreadable` |
| `escape-possible` | `GET /escapes/possible`, `POST .../confirm`, `.../dismiss` |
| `escape-create-check`, `-tooling`, `escape-report` | the Review queue's merge row, `POST /escapes/mark`, `POST .../check` |
| `escape-detail`, `-detail-tooling`, `-check-invalid` | `GET /escapes/{id}`: events, the check and its proof results, counts against, area record |
| `escape-dashboard`, `escape-narrow-*` | `GET /escapes/report` by week, engine, area, lane (narrow: layout only) |
| `escape-route-counts`, `-route-shifted` | per-engine counts for the task's area (`GET /escapes/counts?project=&area=`); only display, until the owner decides the Router tie-break |
| `escape-settings` | `GET /escapes/thresholds` (window, 10 merges, 2x; `proposed: true`); one real setting, "ask each time", stays a constant in v1 |

## Data model

**Migration 47**, additive. Highest on main is 46 (`crew_review_controls.sql`), but Engineering 3 builds review capacity (4l) and
the backtest (4n) as new modules and also takes migrations: **take the next free number at merge and rebase.**

- `crew_escapes(id PK, project_id, merge_task_id UNIQUE REFERENCES crew_merges(task_id), source, state, gap, gap_by, engine,
  model, areas, lane, reason, found_unix, decided_unix, decided_by)`. `state`: `possible`, `confirmed`, `dismissed`.
  `source` and `gap` are checked enums. `reason` is at most 200 characters, redacted by the same scrub the ledger uses.
- `crew_escape_events` (append-only, UPDATE and DELETE refused by triggers, like `crew_area_events`): `detected`, `confirmed`,
  `dismissed`, `reclassified`, `check_proposed`, `check_failed`, `check_proven`, `check_merged`, `check_withdrawn`;
  unique `(escape_id, kind, cause)`. The audit row is written in the same transaction as the state change (rule 6).
- `crew_escape_checks(escape_id PK, kind, path, task_id, state, proof)`. `proof` is JSON: commit ids and two booleans.

**Stored:** ids, commit ids, paths, enums, areas, engine and model names, one short reason. **Never stored:** diffs, test bodies,
command output, prompts, secrets or comment text. On this device only (rule 4). Limits: 5,000 escapes and 20,000 events per
project; oldest `dismissed` and `possible` rows go first, `confirmed` never. Unreadable fails closed, nothing counted (rule 7).

## HTTP and route classes

All under `/api/crew`, Preview-gated; GETs in `crew::changes::READS`, writes in `STATE_CHANGING` with a `GUARDS` entry. Classes
follow [worker-token-isolation.md](../worker-token-isolation.md), which is not built: routes are `local_only` today and move to
`Person` when the key lands.

| Route | Class |
| --- | --- |
| `GET /escapes`, `/escapes/{id}`, `/escapes/possible`, `/escapes/report`, `/escapes/counts`, `/escapes/thresholds` | Local |
| `POST /escapes/mark`, `/escapes/{id}/confirm`, `/dismiss`, `/gap` | Person |
| `POST /escapes/{id}/check` (propose), `/check/accept`, `/check/retry`, `/check/withdraw` | Person |

Each write takes a `seen` token (state and last event id; a mismatch is `409 stale`, as areas do) and answers with the escape
as it now stands. `check/accept` is the only route that starts a worker.

## PR plan

1. **Store and revert log.** Migration, tables, `revert` source (back-filled), `GET /escapes`, `/{id}`, `/thresholds`.
   Tests: migration leaves older tables alone; a revert makes one escape, idempotent; immutable events; unreadable fails closed.
2. **Person acts.** `mark`, `confirm`, `dismiss`, `gap`, audit first. Tests: `409 stale`; refused without person transport;
   N1 refuses an agent; counts change only after confirm.
3. **Possible-escape scan.** Three signals, `GET /escapes/possible`. Tests: temporary git repos per signal and per false
   positive above; bounds; dismissed not re-offered; a GET writes nothing.
4. **Create a check.** Brief, proof, merge-queue path, `failed` state. Tests: passes-on-both refused; no lane removed; fake engine.
5. **Report and screens.** `/report`, `/counts`, web client, bindings, mock, the 17 frames on real data. Tests: week buckets,
   low-sample boundary, web tests per frame state. Counts into the 4j breaker or Router tie-break are **not** in this plan.

## Open questions for the owner

1. Do reverts count at once (the Settings frame) or only after a confirm like the other signals?
2. Should confirmed escapes feed the 4j breaker and the Router tie-break? Both change enforced behavior; own PR and thresholds.
3. Are this device's repository checks enough for "post-merge CI failure", or is a forge status wanted?
4. May a check edit `.orbyn/crew.json`, or only add test files in v1? Build with the Proposed constants (60 days, 14 days, 10 merges, 2x)?
