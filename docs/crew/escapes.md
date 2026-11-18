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
  drops out, as for 4j. A failure to log never undoes a revert (`INSERT OR IGNORE`).
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

## Decisions (owner)

1. A revert is a possible escape at once, labelled so; confirmed only by a person. Counts are shown separately.
2. v1: confirmed escapes do not feed the 4j breaker or Router tie-break (display and checks only); revisit after real runs.
3. v1: post-merge failure is the device repository's own checks only. Forge CI status is Planned.
4. v1: a created check only adds test files, never policy files. Constants (60 d, 14 d, 10 merges, 2x) are defaults stored as
   device config (existing scoped settings, no new table).
