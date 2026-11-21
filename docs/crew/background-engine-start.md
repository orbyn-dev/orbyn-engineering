# Engine start for background crews (4h)

Design note. Status: PR 1 (the `manual` event, Run now through the core), PR 2 (the budget: turn and minute counting from engine events, and the daily per-engine budget) and PR 4 (the Run now route and the read side of the Review join) built; the listeners and the person's answer to a waiting background review remain to build. A background crew is stored and can be turned on; this note scopes the step that starts a real Crew run from a trigger and brings its result to Review.

## 1. What exists today

- Stored crews and runs (`crew/background.rs`, `store::crew_background`, schema 41 and 42): template, trigger kind and settings, a budget of `turns` and `minutes` per run fixed at creation, on/off, audit rows. Routes create, update, enable, disable, delete and Run now (`POST /api/crew/background/{id}/run-now`, a Person route).
- Run seam: `crew_background_start_run`, `crew_background_spend` and `crew_background_end_run`. Only tests call them. A run is a row; it starts no crew task and no engine.
- Trigger parser (#636): `crew::triggers::parse_and_validate` reads `background` of `.orbyn/crew.json` (`ci_failed`, `schedule`, `manual`). Nothing reads it yet, and it is not mapped to a stored crew.
- Trigger listener: not available today. No CI-failure listener, no schedule timer, no manual start. The pull watch (`scm/watch.rs`) reads checks of linked pull requests only, and the main watch (`forge_watch.rs`) watches merges, not CI on a branch.

## 2. What starts a run

- A person sets up every trigger: creates the crew (local only, created off) and turns it on. A file trigger alone starts nothing; it only fills in a crew the person creates.
- Events: `ci_failed` (a failed check run on the branch head, optional workflow), `schedule` (every `every_hours`), `manual` (the person presses Run). One event key per firing, such as head commit + workflow, so a repeat of the same failure starts nothing new.
- A run goes through `crew_start` with `Links::crew_started` and a `PausedTrigger` of kind `Background` (key: crew id + event key). It is Low priority, so `capacity_start_check` can refuse it while too many merges wait; the refusal is a held row in `crew_paused_starts`, never retried on a timer.
- Refused, not started: crew off or deleted, project gone, Preview off, a run already running, no engine that passes `worker_refusal` and the router's tier rules among the trigger's `engines`.
- A refusal after the task exists (for example the task could not be stored or routed) holds the task for a person and ends the run `needs_review` with the reason, in one transaction with the refusal audit (after the commit each held task gives up its leases and gets one `NeedsPerson` result, best effort; a task with no run row gets none and `review_reason` carries the reason); the run is kept and returned, not an error. A refusal before any task exists takes back the run and its lead thread in one transaction, so a crash leaves nothing behind.
- `crew_start` needs a lead thread. The build opens one thread per run in the crew's project, marked as the crew's.

## 3. Budget

- Per run: `turns` and `minutes`, set by the person, never defaulted, never money. Already stored.
- Per day, per engine: runs and turns each engine may spend for this crew in a day, added at creation (PR 2: `daily` of the create body, migration 57), fixed like the run budget, never defaulted, never money. The day is the person's (`utc_offset_minutes`, the same clock the scheduled tasks take), so it resets at their midnight.
- Spending: each engine turn adds one turn to the engine that ran it, counted from the engine event itself (`crew_worker_run_ended`, one call per turn), minutes are wall time. No engine takes a hard limit, so a run already started is cancelled at the cap (stopped `budget`, waiting for the person) and what the engine spent is shown, never clamped. A crew made before PR 2 has no daily budget: nothing is counted for it and nothing is refused. A daily budget that cannot be read is never taken as unspent: the start is refused, or the run stopped. The offset is fixed at creation, so a daylight-saving change moves the reset an hour off the wall clock.
- Not built yet: review lanes, review fixes and merge-comment fixes are not reachable for a background task, so the PR that adds a person's answer to a waiting background review must gate them with the daily budget (the read-only join of PR 4 adds no way to reach them). Their turns would be counted but never stopped, because the run is no longer running by then.
- Run budget spent: stopped `budget`, waits for review (exists). Daily budget spent: new events are refused before anything is stored — never held, never queued — with an audit row "daily budget spent: <engine>" (action `crew.background.daily.refused`) and a sentence that shows when the day turns.

## 4. Where the run lands in Review

- A run that merges: its merge row is a normal `crew_merges` row with `review_state` `waiting`, so it appears in `crew_review_queue` by risk. As a Low task, `capacity_after_merge` adds it to a sampling batch when the person turned sampling on (one in four).
- A run that stops without a merge: stays on its background run row (`needs_review` or `stopped`), and `GET /api/crew/autopilot/review` now joins it beside the merges as `background_runs` (read side, PR 4): the run's state, stop reason, plain reason, spend and budget, oldest first, in the same page window as the merges (`background_total` counts them all). A running run and one stopped by the person's own switch are not waiting. It is never shown with a merge, a commit or a risk it does not have. **Not built: the person's answer to one.** No route settles or dismisses a waiting background review, so the row is read-only until the owner decides what an answer does.

## 5. Run-end contract

`docs/engine-run-contract.md` applies to every run. A run ends only on the prompt's own turn; a budget cancel or a person's off switch is recorded as `stopped` with its reason, and held background work as `background.stopped`, never a silent kill. No assistant output, or an ACP reply other than `end_turn`, ends the run `engine_error`, never "finished". A loop-guard stop (same failure twice) ends it `repeated_failure`.

## 6. PR split

All behind `CoreConfig::crew_preview`, tests first with fake engines.

1. Manual start in core: start one run of a stored enabled crew by hand (mapping a file trigger to a stored crew is PR 3, the listeners), open the run, lead thread and `crew_start` with `Links::crew_started` and a `Background` trigger; refusals; run-end mapping to stop reasons. Fake-engine tests for capacity pause, no reply and repeated failure.
2. Budget: turn and minute counting from engine events, cancel at the cap, the daily per-engine budget (one migration). Fake-engine tests for overshoot and the daily refusal.
3. Listeners: schedule timer and CI-failure watch as one owned future like `run_crew_merge_watch`, opt-in, awaited on shutdown, deduplicated by event key. Fake-forge tests.
4. Review join: stopped runs in the Review queue, a route for Run now, contracts and `web/src/generated` in the same commit.

## 7. Decisions (conservative defaults, the owner can change them)

1. The daily budget resets at local midnight. A refusal shows the reset time. (Built in PR 2: the person's clock comes with the create body as `utc_offset_minutes`, the automations' own pattern, because the service cannot read the host's zone portably.)
2. v1: every background merge waits for a person. No Autopilot auto-landing for background runs until escape data exists.
3. Run now (`manual`) is part of 4h and is the first PR.
4. `issue_label` and `dependency_update` are refused when turned on, with a plain reason (no listener).
5. A file trigger with no matching crew is ignored: nothing starts and nothing is adopted. At most a diagnostic line, "found in .orbyn/crew.json, no matching crew", and no UI.
6. The listeners PR (3) gets an independent Opus review: event text from outside is never part of the prompt as instructions.
