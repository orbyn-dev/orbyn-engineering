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

## Decisions (owner)

1. A revert is a possible escape at once, labelled so; confirmed only by a person. Counts are shown separately.
2. v1: confirmed escapes do not feed the 4j breaker or Router tie-break (display and checks only); revisit after real runs.
3. v1: post-merge failure is the device repository's own checks only. Forge CI status is Planned.
4. v1: a created check only adds test files, never policy files. Constants (60 d, 14 d, 10 merges, 2x) are defaults stored as
   device config (existing scoped settings, no new table).
