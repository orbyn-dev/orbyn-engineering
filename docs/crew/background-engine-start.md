# Engine start for background crews (4h)

Design note. Status: PR 1 (the `manual` event, Run now through the core), PR 2 (the budget: turn and minute counting from engine events, and the daily per-engine budget), PR 3 (the listeners: the schedule timer and the CI-failure watch, one owned future) and PR 4 (the Run now route and the read side of the Review join) built, and the owner's acknowledge-only answer to a waiting background review is built. A background crew is stored and can be turned on; this note scopes the step that starts a real Crew run from a trigger and brings its result to Review.

## 1. What exists today

- Stored crews and runs (`crew/background.rs`, `store::crew_background`, schema 41 and 42): template, trigger kind and settings, a budget of `turns` and `minutes` per run fixed at creation, on/off, audit rows. Routes create, update, enable, disable, delete and Run now (`POST /api/crew/background/{id}/run-now`, a Person route).
- Run seam: `crew_background_start_run`, `crew_background_spend` and `crew_background_end_run`. Only tests call them. A run is a row; it starts no crew task and no engine.
- Trigger parser (#636): `crew::triggers::parse_and_validate` reads `background` of `.orbyn/crew.json` (`ci_failed`, `schedule`, `manual`). The listeners (PR 3) read it only to log a trigger that has no stored crew; it is never mapped to a stored crew and starts nothing.
- Trigger listeners (PR 3, built): the schedule timer and the CI-failure watch (section 2a), and the manual start (Run now, PR 1). The pull watch (`scm/watch.rs`) reads checks of linked pull requests only, and the main watch (`forge_watch.rs`) watches merges, not CI on a branch; the CI-failure watch reads the branch head's checks itself (`crew_head_checks`).

## 2. What starts a run

- A person sets up every trigger: creates the crew (local only, created off) and turns it on. A file trigger alone starts nothing; it only fills in a crew the person creates.
- Events: `ci_failed` (a failed check run on the branch head, optional workflow), `schedule` (every `every_hours`), `manual` (the person presses Run). One event key per firing, such as head commit + workflow, so a repeat of the same failure starts nothing new.
- A run goes through `crew_start` with `Links::crew_started` and a `PausedTrigger` of kind `Background` (key: crew id + event key). It is Low priority, so `capacity_start_check` can refuse it while too many merges wait; the refusal is a held row in `crew_paused_starts`, never retried on a timer.
- Refused, not started: crew off or deleted, project gone, Preview off, a run already running, no engine that passes `worker_refusal` and the router's tier rules among the trigger's `engines`.
- A refusal after the task exists (for example the task could not be stored or routed) holds the task for a person and ends the run `needs_review` with the reason, in one transaction with the refusal audit (after the commit each held task gives up its leases and gets one `NeedsPerson` result, best effort; a task with no run row gets none and `review_reason` carries the reason); the run is kept and returned, not an error. A refusal before any task exists takes back the run and its lead thread in one transaction, so a crash leaves nothing behind, except a low-disk refusal: that run stays as a stopped not-started row (`reason_code` `disk_low`, no task, not waiting) with its sentence, only its lead thread is removed, and the person still gets the refusal.
- `crew_start` needs a lead thread. The build opens one thread per run in the crew's project, marked as the crew's.

### 2a. The listeners as built (PR 3)

- One owned future, `run_crew_background_listeners`, behind the Preview flag and an interval (`ORBYN_CREW_BACKGROUND_WATCH_INTERVAL_SECS`, 60s to 24h). **No interval means no pass**, not even at startup. With one, a pass runs at startup and then once per interval.
- Only crews that are on are read from the forge or claimed. A crew that is off costs no forge call and spends no event: turned on later, it sees the branch head and the current schedule bucket as new events. The start's own refusal stays as the second guard.
- Claims (table `crew_background_events`, migration 59, first write wins; the claim is the dedupe and the refusal is never retried):

| Event | Claim identity | `PausedTrigger` key |
| --- | --- | --- |
| schedule | `schedule:<crew>:<bucket>` | `<crew>:schedule:<bucket>` |
| CI failure | `ci:<crew>:<head>:<name hash>` | `<crew>:ci:<head>:<name hash>` |

  `<bucket>` is the Unix time divided by `every_hours`; a bucket at or before the newest one claimed for the crew never fires, so a clock that moves back starts nothing. `<name hash>` is 16 hex characters of the failing check's SHA-256, so a long check name cannot break the held row's 128-character key.
- At most one start attempt per crew per pass. A head with several failing checks fires the first (by name) and leaves the rest unclaimed for a later pass.
- A pass watches at most 64 CI crews, from a rotating start, so a store with more is covered over a few passes.
- Refused for capacity, the daily budget, an untrusted folder, no engine, a run in progress or a crew that was turned off meanwhile: dropped, never retried. One code-owned audit row (`crew.background.listener.refused`: the crew, the listener, a fixed reason word) records it, and capacity also leaves its held row.
- A stop is looked at between firings and never inside one, so a claim is always followed by its whole start. A run a crash leaves behind is settled when the core next opens (`recover_background_starts`).
- Text from outside (a check's name, a head sha, a file trigger's name) only decides whether a run starts: it never reaches the brief, a prompt, the lead thread or an audit row. The listener's audit actor is the constant `crew`.
- Not done: `crew_background_events` grows by one row per firing and is not pruned or removed with the crew.

## 3. Budget

- Per run: `turns` and `minutes`, set by the person, never defaulted, never money. Already stored.
- Per day, per engine: runs and turns each engine may spend for this crew in a day, added at creation (PR 2: `daily` of the create body, migration 57), fixed like the run budget, never defaulted, never money. The day is the person's (`utc_offset_minutes`, the same clock the scheduled tasks take), so it resets at their midnight.
- Spending: each engine turn adds one turn to the engine that ran it, counted from the engine event itself (`crew_worker_run_ended`, one call per turn), minutes are wall time. No engine takes a hard limit, so a run already started is cancelled at the cap (stopped `budget`, waiting for the person) and what the engine spent is shown, never clamped. A crew made before PR 2 has no daily budget: nothing is counted for it and nothing is refused. A daily budget that cannot be read is never taken as unspent: the start is refused, or the run stopped. The offset is fixed at creation, so a daylight-saving change moves the reset an hour off the wall clock.
- Not built yet: review lanes, review fixes and merge-comment fixes are not reachable for a background task, so the PR that adds a person's answer to a waiting background review must gate them with the daily budget (the read-only join of PR 4 adds no way to reach them). Their turns would be counted but never stopped, because the run is no longer running by then.
- Run budget spent: stopped `budget`, waits for review (exists). Daily budget spent: new events are refused before anything is stored — never held, never queued — with an audit row "daily budget spent: <engine>" (action `crew.background.daily.refused`) and a sentence that shows when the day turns.
- A run cancelled at the daily cap is stopped `budget` with the core's sentence in `review_reason` (it names the engine, its turns and when the budget resets). The Needs you row shows that sentence as read and the next step, wait for the daily reset, with the reset time the sentence carries. A daily budget that could not be read or updated carries no reset time, so the row says "No reset time" and that this is not a wait for the reset; it never works a time out. The row has one action, Open; no button changes the budget (Crew Designs `needsyou-daily-budget-stop`, `-narrow`, `-no-reset`). The lifetime budget stop keeps its own line: that budget cannot be changed. The wire has no flag for the daily stop, so the web tells it by the words "daily budget" in the sentence; a structured field would be the stronger contract.

## 4. Where the run lands in Review

- A run that merges: its merge row is a normal `crew_merges` row with `review_state` `waiting`, so it appears in `crew_review_queue` by risk. As a Low task, `capacity_after_merge` adds it to a sampling batch when the person turned sampling on (one in four).
- A run that stops without a merge: stays on its background run row (`needs_review` or `stopped`), and `GET /api/crew/autopilot/review` joins it beside the merges as `background_runs` (read side, PR 4): the run's state, stop reason, plain reason, spend and budget, oldest first, in the same page window as the merges (`background_total` counts them all, `next_offset` is set while either list has more). A running run, one stopped by the person's own switch and one whose review the person acknowledged are not waiting. It is never shown with a merge, a commit or a risk it does not have. **The person's answer is acknowledge-only (owner ruling, 2026-11-20):** `POST /api/crew/background/runs/{id}/acknowledge` (a Person route behind the Preview flag and local only) clears the run's wait, keeps why it waited, its state, stop reason and spend, and returns the run. The crew's `reviews_waiting` drops, the same crew may run again, and the run leaves the queue (`background_total` no longer counts it); the run keeps its reason for the record. A run that is not waiting is refused plainly. Nothing else is offered: no dismiss, no open, and the Review screen's acknowledge control is not in the beta. A run that stopped for a failure still appears, because it has nothing to review and nothing to acknowledge (PR 4's open question about failed stops is unchanged).

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
