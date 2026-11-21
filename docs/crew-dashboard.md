# Crew dashboard contract

What the web Crew dashboard (W12 in [crew.md](crew.md), frames `dash-*` of Crew Designs "23. Dashboard") needs from the service. **Planned**: the web page is
built and draws the live parts today; the one read written here is not served, so the page shows a Planned mark where only that read can answer. Eng 2 owns
the service side. A name here changes only if the service PR says so. Row: G17 in [parity](parity.md).

## What exists today and what the page draws from it

| Figure | Source | State |
| --- | --- | --- |
| Merges in the window, reverts, revert rate (a rate from 10 merges) | `GET /api/crew/trust?project=` | Live. Window is the service's own, 60 days (`window_days`); no `days` parameter. |
| Merges and reverts by engine, by area | the same read | Live. At most 50 rows each, `truncated` says more exist. |
| The mode each area is on, its record and the changes of mode | `GET /api/crew/autopilot/areas`, `.../areas/history`, `.../thresholds` | Live. Window 14 days. Escapes are not measured. |
| Autonomy rate, cycle time, cost per engine, merges per week, who reverted, the merges of a small repo | none | **Planned**: `GET /api/crew/dashboard` below. |

Until that read exists the page draws Autonomy, Cycle time and Cost as Planned tiles (a dash, never a zero), the bars as a Planned "Over time" heading,
the 30 and 90 day periods as disabled and Planned, and the Autonomy and Cost columns of the engine table as Planned. The revert rate, the engine and area
rows and Autopilot by area are real. All projects reads `trust` once per repository (at most 24) and says in the row of one it could not read.

## `GET /api/crew/dashboard?project=<id>&days=<30|60|90>`

Local read: reads this device's Crew records, writes nothing, needs no person key, and is refused with `404` while Crew Preview is off (like every Crew route).
`project` is required and is a project id the service lists; `days` defaults to `60`. A repository the person has never used Crew in answers `merges: 0` and
empty lists, not an error. One request answers one repository; the page makes one request per repository for All projects.

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
| `engines` | array, at most 12 | `{name, model, merges, unaided, reverted, cost}`. `cost` is `{unit, amount}` in the engine's own unit (`tokens`, `credits`, `requests`; an unknown unit is shown as given) or `null` when the engine reports none. |
| `areas` | array, at most 50 | `{name, merges, unaided, reverted}`. The first two folders of a path, as in Autopilot by area. No cost: one area's work can use engines counted in different units. |
| `reverts` | array, at most 20, newest first | `{title, at, by, certain}`; `by` is `person` or `auto`; `certain` is a boolean or `null`. |
| `recent` | array, at most 20, newest first | `{title, engine, at, needed, checks}`; `needed` is `nothing`, `answer`, `approval` or `other`; `checks` is `passed`, `failed` or `none`. The page lists these only for a repository under `min_rate_sample` merges. |
| `truncated` | boolean | A list above had more rows than it holds. |

Rules the service keeps and the page relies on:

- Every figure is for the repository and the window asked, on this device. Runs made on other devices are not included, and the page says so.
- Cost is never summed across engines or units, by the service or the page. The page adds one engine's own unit across repositories for the All projects total.
- A `title`, `name` or `model` is text from a pull request, a branch or an engine: untrusted. The service scrubs secrets as `trust` does (`redact`) and caps
  each at 200 characters; the page caps again at 60 and draws it as text, never as markup or a link.
- A count the service cannot measure is left out of the answer (`cycle: null`, `cost: null`), never sent as `0`.

Errors (the usual `{code, message}` body):

| Status | Code | When |
| --- | --- | --- |
| 400 | `invalid_request` | `days` is not 30, 60 or 90, or `project` is missing or not a valid id. |
| 404 | `not_found` | Crew Preview is off, or the project is unknown. |
| 500 | `history_unreadable` | The Crew records on this device could not be read. The page says nothing was changed and offers Try again. |

Limits: one repository per request; the answer is at most about 40 KiB; the read is cheap enough to repeat on a refresh and is not cached by the page.

## As built (web)

- Client: `web/src/features/crew/dashboard/contract.ts`. The default (production) client has `history: null`, so it never sends this request. A test
  (`live.test.tsx`) asserts no `/api/crew/dashboard` call is made.
- Mock: `web/mock/crewDashboard.ts`, used by tests and the dev-only Preview frames (`#/preview/crew/dash-*`). Guard markers in
  `web/scripts/assert-no-previews.mjs` fail a production build that holds it.
- Page: Settings › Crew › Dashboard at `#/settings/crew/dashboard?repo=<project>`, behind Crew Preview. The project and period are remembered per person
  in browser storage (`orbyn.crew-dashboard`), with a fallback when storage is off.
- When the service serves the read: set `history` in `contract.ts` to the typed request and the Planned marks drop out by themselves; update this note's
  "What exists today" table and the G17 row.

## Open questions for Eng 2

1. "Needed nothing from the person" needs a definition the records can answer. Is an answered lead question or an approved held command recorded per merge today?
2. Cycle time starts at the brief. Which record holds that moment for a task that began as a thread or from a background crew trigger?
3. Which engines report a cost, in which unit, per merge? The designs assume tokens for claude, codex and opencode.
4. Should `weeks` use the device's local week or UTC? The designs show local.
