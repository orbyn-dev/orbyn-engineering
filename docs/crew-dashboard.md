# Crew dashboard contract

What the web Crew dashboard (W12 in [crew.md](crew.md), frames `dash-*` of Crew Designs "23. Dashboard") needs from the service. **The
service side is served** (`GET /api/crew/dashboard` and the project menu's `GET /api/crew/dashboard/projects`, this document) and the
single-repository read's web client is wired to it; a service without the route answers `404` and the page falls back to the trust
record, drawing a Planned mark where only this read can answer. Eng 2 owns the service side. A name here changes only if the service
PR says so. Row: G18 in [parity](parity.md).

## What exists today and what the page draws from it

| Figure | Source | State |
| --- | --- | --- |
| Merges in the window, reverts, revert rate (a rate from 10 merges) | `GET /api/crew/trust?project=` | Live. Window is the service's own, 60 days (`window_days`); no `days` parameter. |
| Merges and reverts by engine, by area | the same read | Live. At most 50 rows each, `truncated` says more exist. |
| The mode each area is on, its record and the changes of mode | `GET /api/crew/autopilot/areas`, `.../areas/history`, `.../thresholds` | Live. Window 14 days. Escapes are not measured. |
| Autonomy rate, cycle time, cost per engine, merges per week, who reverted, the merges of a small repo | `GET /api/crew/dashboard` below | **Served** by the service; the production client sends it (a `404` falls back to the trust record and Planned tiles). |

Until that read exists the page draws Autonomy, Cycle time and Cost as Planned tiles (a dash, never a zero), the bars as a Planned "Over time" heading,
the 30 and 90 day periods as disabled and Planned, and the Autonomy and Cost columns of the engine table as Planned. The revert rate, the engine and area
rows and Autopilot by area are real. All projects reads `trust` once per repository (at most 24) and says in the row of one it could not read.

## `GET /api/crew/dashboard?project=<id>&days=<30|60|90>&utc_offset_minutes=<n>`

Local read: reads this device's Crew records, writes nothing, needs no person key, and is refused with `404` while Crew Preview is off (like every Crew route).
`project` is required and is a project id the service lists; `days` defaults to `60`. `utc_offset_minutes` is the person's own offset, as the daily digest takes
it, and defaults to `0` (UTC): it decides which Monday a week starts on. A repository the person has never used Crew in answers `merges: 0` and empty lists, not
an error. One request answers one repository; the page makes one request per repository for All projects.

Answer:

| Field | Type | Notes |
| --- | --- | --- |
| `project` | string | Echo of the project id. |
| `days` | 30, 60 or 90 | The window actually read. |
| `as_of` | unix seconds | When the figures were taken. |
| `min_rate_sample` | integer | A rate shows from this many merges. `10`. Sent so the page never hard-codes it. |
| `min_median_sample` | integer | A median shows from this many merges. `5`. |
| `merges` | integer | Merges in the window. Stopped and cancelled tasks are not merges and are not counted anywhere. |
| `unaided` | integer | Merges that needed nothing from the person after the brief: no answered question, no approval, no override, no edit. `unaided <= merges`. |
| `reverted` | integer | Merges later reverted, by anyone. `reverted <= merges`. |
| `reverted_auto`, `reverted_by_person` | integers | The split of `reverted`; the two add up to it. |
| `cycle` | object or `null` | `{median_s, p90_s, fastest_s, slowest_s}`, each seconds or `null`. Brief given to merge. `null` while no merge has both times. The page shows a median only from `min_median_sample` merges. |
| `weeks` | array, at most 14, oldest first | `{start, merges, unaided, reverted}`; `start` is unix seconds of the week's first day (the device's local Monday). Empty when `merges` is 0. |
| `engines` | array, at most 12 | `{name, model, merges, unaided, reverted, cost}`. `cost` is `{unit, amount}` in the engine's own unit, as the merge reported it: `tokens` when an engine reports tokens, `micros` for the billing unit's millionths, or the raw unit string when it is neither - never added across different units, so an engine that reported two units gets `cost: null`. `null` when the engine reports none. |
| `areas` | array, at most 50 | `{name, merges, unaided, reverted}`. The first two folders of a path, as in Autopilot by area. No cost: one area's work can use engines counted in different units. |
| `reverts` | array, at most 20, newest first | `{title, at, by, certain}`; `by` is `person` or `auto`; `certain` is a boolean or `null`. |
| `recent` | array, at most 20, newest first | `{title, engine, at, needed, checks}`; `needed` is `nothing`, `answer`, `approval` or `other`; `checks` is `passed`, `failed` or `none`. The page lists these only for a repository under `min_rate_sample` merges. |
| `live` | object | The "Right now" row (KC3): a live count of now, not a figure over the window, so `days` never narrows this block while it does narrow every figure above it. `needs_you` is `[{kind, count}]` from the one needs-you field (`GET /api/crew/needs-you`) narrowed to this repository, dismissed items left out, most items first then by kind: the same field the Inbox and the Taskboard read, though not necessarily the same count (a background run's wait has no board card, and another repository's items are not here). `working` is `[{why, count}]` from the one `working` function: the workers of this repository that own live work now, by why (`running`, `verifying`, `retry`, `fix`), most first then by why. `truncated` is true when the working read hit its 500-worker bound, so `working` covers the newest workers only. Idle is not served: the store keeps no crew member that holds no task, so an idle tile would be invented. |
| `truncated` | boolean | A list above had more rows than it holds. |

Rules the service keeps and the page relies on:

- Every figure is for the repository and the window asked, on this device. Runs made on other devices are not included, and the page says so.
- Cost is never summed across engines or units, by the service or the page. The page adds one engine's own unit across repositories for the All projects total.
- A `title`, `name` or `model` is text from a pull request, a branch or an engine: untrusted. The service scrubs it like the trust read (`redact`), withholds the task's
  answer-key text, masks the person's own Settings › Secrets patterns, and caps each at 200 characters; text that still fails the secret or answer-key check is
  withheld, never sent. The page caps again at 60 and draws it as text, never as markup or a link.
- A count the service cannot measure is left out of the answer (`cycle: null`, `cost: null`), never sent as `0`.

Errors (the usual `{code, message}` body):

| Status | Code | When |
| --- | --- | --- |
| 400 | `invalid_input` | `days` is not 30, 60 or 90, `project` is missing or not a valid id, or `utc_offset_minutes` is not a whole number between -840 and 840. |
| 404 | `not_found` | Crew Preview is off, or the project is unknown. |
| 500 | `history_unreadable` | The Crew records on this device could not be read. The page says nothing was changed and offers Try again. |

Limits: one repository per request; the answer is at most about 40 KiB. The merge read is bounded to the window in SQL, so a refresh re-reads only the window's merges and the page does not cache the answer.

## `GET /api/crew/dashboard/projects?days=<30|60|90>`

The project menu's counts: how many merges each repository the service lists made in one window, so the menu can draw a
count beside a name without one request per repository first (which is why the built menu shows no count at all). Same
family, gate and `days` rule as the read above: `days` defaults to `60` and is one of 30, 60 or 90. There is no
`utc_offset_minutes`: no week is counted here, so the person's zone cannot change a count.

Answer:

| Field | Type | Notes |
| --- | --- | --- |
| `days` | 30, 60 or 90 | The window actually read. |
| `as_of` | unix seconds | When the counts were taken. |
| `projects` | array, at most 200, in the service's own project order | `{project, merges}`. One row per repository the service lists: a repository the person has never used Crew in is a row with `merges: 0`, never left out, because no merges is a figure the records can state. |
| `truncated` | boolean | The project list had more repositories than one answer holds; the rows beyond the bound are not here, and the menu says the counts are partial rather than drawing a cut list as whole. |

Rules the service keeps:

- A count is the same figure `GET /api/crew/dashboard` answers as `merges` for that repository and window, so the menu and the page never disagree.
- Counts only. A rate, a median, a cycle time or a cost stays with the one-repository read above; this answer is never used to work one out.
- Counted in one query, bounded to the window in SQL: the read never hands the window's merge rows back, and never reads a merge's own brief, proof or ledger rows. A merge whose task is no longer joined to a lead thread is in no project's count.
- Nothing here is text, so nothing here is scrubbed: `project` is an id, and a name is the project list's.

Errors: `400 invalid_input` when `days` is not 30, 60 or 90; `404 not_found` while Crew Preview is off; `500 history_unreadable` when the Crew records or the project list on this device cannot be read, never an empty menu.

## Answers to the four open questions

The service answers them from records it already keeps (`crates/orbyn-core/src/crew/dashboard.rs`); none needed a new table or column.

1. **"Needed nothing from the person"** is: no row in the approval ledger for the task of the kinds `held`, `approved_once`, `denied_by_person`,
   `dismissed_by_person` or `hold_used`; no engine question asked and answered after the task's brief on its lead or worker thread (one `EXISTS` scoped to
   the task, so a question from an earlier task on the shared lead thread never marks a later one); and no row of the kinds `person`
   (a request that needs a person), `main_broken` (a broken main it asked about) or `merge_waiting`. `needed` and `unaided` are both read from that.
   An answered lead question inside a Crew task is an engine question (`question_requests`), not a crew message, so that is the record read.
2. **Where the brief moment lives:** `crew_tasks.created_at`, the row written when the brief was accepted. A task that began as a thread or from a
   background crew trigger has the same row, so cycle time never needs a second source. A task whose stored stamp cannot be read, or whose merge
   predates it, contributes no cycle figure rather than a negative one. Each cycle runs from that moment to `crew_merges.merged_unix`.
3. **Which engines report a cost, in which unit:** the merge's own proof (`crew_merges.proof.cost`, taken from the passed result's `usage`). Tokens are
   `tokens_in + tokens_out` under the unit `tokens`; failing that, `cost_micros` under the unit `micros`. An engine that reported nothing has
   `cost: null`, and an engine whose merges reported two different units also has `cost: null`: a cost is never added across units. Nothing here
   assumes `claude`, `codex` or `opencode`; whatever an engine reports is shown in its own unit.
4. **Local or UTC weeks:** local, as the designs show. The service cannot read the machine's zone on its own (the `time` crate is built without
   `local-offset`), so the request carries `utc_offset_minutes`, the same value the daily digest already takes, and the service counts each week from
   the local Monday it names. Absent, weeks are UTC.

## As built (service)

- Route: `GET /api/crew/dashboard` in `crates/orbynd/src/routes/crew.rs`, listed in `crates/orbynd/tests/routes.json`. Read: `Core::crew_dashboard`
  in `crates/orbyn-core/src/crew/dashboard.rs`. Types: `crates/orbyn-contracts/src/crew_dashboard.rs` (`CrewDashboard` and its rows).
- The project menu's counts: `GET /api/crew/dashboard/projects`, read by `Core::crew_dashboard_projects` and counted by one query
  (`Store::crew_merge_counts_since`, grouped by the lead thread's project and bounded to the window in SQL), over the service's own
  project list (`Store::list_projects`, at most 200 rows, `truncated` when it is cut). Types: `CrewDashboardProjects` and
  `CrewDashboardProjectCount`. Nothing is text, so nothing is scrubbed, and a read that fails is `history_unreadable`, never an empty menu.
  Tests: `crew::tests::dashboard`'s `the_projects_read_counts_every_repository_and_only_the_window` (the count equals the
  one-repository read's `merges` for every window), `..._refuses_a_window_it_does_not_serve_and_the_flag_being_off`, `..._writes_nothing`,
  `..._says_when_the_menu_lists_more_repositories_than_one_answer_holds` and `..._answers_history_unreadable_when_the_records_cannot_be_read`,
  plus `crates/orbynd/src/routes/crew_dashboard_tests.rs` (`the_projects_read_needs_the_flag_the_token_and_answers_a_row_for_every_repository`).
- Weeks: one row per week from the first week with a merge to the week of `as_of`, or to the week of a merge stamped after `as_of` (it is still counted in
  `merges`), at most 14 (newest kept), so a quiet week is a zero bar and never a missing one and the weeks add up to `merges`. `title`, `name` and `model`
  are redacted, have the task's answer-key text withheld and the person's Settings › Secrets patterns masked, and are capped at 200 characters; the page
  caps again at 60.
- `certain` is `true` for a revert the watch made by itself or a recorded certain break, `false` when the watch recorded a break without certainty and a
  person reverted it, and `null` when no record says why.
- The merge read is bounded to the project and the window in SQL, and the brief, task stamp, answer key, ledger rows and answered-question check for all
  the window's tasks are each read once, not once per merge. A store read that fails answers `500 history_unreadable`, as the contract says.
- `merge_checks` and `not_verified` are not part of this read: it sends the task's own checks as `passed`, `failed` or `none`, and the proof itself stays
  in the Review queue reads.
- KC3 (the live row): `live` is read after the window and is not narrowed by it, unlike every figure in the answer. `needs_you` comes
  from the one needs-you field (`Core::crew_needs_you`) narrowed to this repository, so a device-local dismissal hides the item here
  and in the Inbox together; the same field, not a second rule and not a count that must equal the Inbox's. `working` is the one `working` function
  (`crates/orbyn-core/src/crew/working.rs`) over this repository's tasks, with the latest fix of a finished task as the only work that outlives
  the task's state. It reads at most 500 workers, which `truncated` reports, and it writes nothing. `needed` counts a `tripwire` ledger row as
  `other`: a task the trip wire stopped and a person then merged by hand did not need nothing. Tests: the `working` truth table,
  `crew::tests::dashboard`'s `the_live_row_reads_the_one_needs_you_field_and_the_one_working_function`, and `crew::tests::background_review`'s
  `what_waits_elsewhere_is_the_one_needs_you_field` for the Review queue's share of the same field.
- Tests: `crates/orbyn-core/src/crew/tests/dashboard.rs` (window, project, weeks, offsets, question scoping, answer-key and person-pattern scrubbing,
  cost units, reverts and certainty, checks, cycle, truncation, refusals, and that the read writes nothing), unit tests beside the read, and
  `crates/orbynd/src/routes/crew_dashboard_tests.rs` (gate, local-only, token, request shape, not-found, unreadable history, and the empty answer).

## As built (web)

- Client: `web/src/features/crew/dashboard/contract.ts`. The production client sends `GET /api/crew/dashboard?project=&days=&utc_offset_minutes=`
  (the person's own offset, so weeks start on their Monday) and `fromService` makes the omitted fields explicit (`null`). A `404` (an older
  service, or Crew Preview off) falls back to the trust record, which fixes the period to its own window and keeps Planned marks. A test
  (`live.test.tsx`) asserts the request and the fallback.
- Mock: `web/mock/crewDashboard.ts`, used by tests and the dev-only Preview frames (`#/preview/crew/dash-*`). Guard markers in
  `web/scripts/assert-no-previews.mjs` fail a production build that holds it.
- Page: Settings › Crew › Dashboard at `#/settings/crew/dashboard?repo=<project>`, behind Crew Preview. The project and period are remembered per person
  in browser storage (`orbyn.crew-dashboard`), with a fallback when storage is off.
- The project menu still draws no count beside a name (`Controls.tsx` says why): the count read above is served and the menu is not
  wired to it yet, so wiring it is web work outside this service change.

## Open questions for Eng 2

1. "Needed nothing from the person" needs a definition the records can answer. Is an answered lead question or an approved held command recorded per merge today?
2. Cycle time starts at the brief. Which record holds that moment for a task that began as a thread or from a background crew trigger?
3. Which engines report a cost, in which unit, per merge? The designs assume tokens for claude, codex and opencode.
4. Should `weeks` use the device's local week or UTC? The designs show local.
