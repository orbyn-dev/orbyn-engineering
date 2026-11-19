# A live lane runner for the reviewer backtest (4n)

Design note. Status: proposal, nothing built. Today `crew_backtest_start` (`crew/backtest/api.rs`) is refused (`Unsupported`) because no `LaneRunner` is installed in `Slot.runner`; only `FakeLaneRunner` and `ScriptedRunner` exist, in tests. This note scopes a `LiveLaneRunner` that replays one review lane or check on one past merge with a real engine.

## 1. Engines and tiers

- A lane runs on an engine a Crew worker may use: `worker_refusal` (`host/confine/mod.rs`) must pass. Today that means Claude, and OpenCode once Settings maps its tier, on macOS only. Codex is refused (`CODEX_WORKERS_ALLOWED = false`), terminal engines are refused, and an engine with no tier mapping never takes a lane.
- The tier follows the reviewer lane rules (`crew/router.rs`, `review_lane_tier`): worker tier by default; `security`, `correctness` (`CODE_LANE`) and repo-locked lanes stay on the lead tier, and a model below it is refused, never swapped. Effort is `medium`.
- The person picks the engine and model per lane in the start request (`LaneSpec`). Not available today: `BacktestRequest::validate` checks names only. The build adds the router's tier check.

## 2. Budget

- Tokens per engine, set by the person, one entry per engine, never summed, never money: already enforced by `BacktestRequest` and `request_of`. Lanes on one engine share it.
- At the cap: `replay::execute` checks the engine's spend before each replay, then stops the lane (`LaneState::Stopped`), sets `stop_reason` to "token budget spent: <engine>" and scores what it replayed. One replay can go past the cap, because no engine takes a hard pre-spend limit (see `ApprovalSpec`). The live runner should watch the engine's `Usage` events and end the replay once the engine's remaining budget is spent, and the result should show the overshoot.
- The estimate today (`crew_backtest_estimate`) gives replays per lane only, no tokens. Proposal: tokens per lane = the median of that lane's past reviews on that engine (`crew_reviews` → `crew_runs.worker_thread_id` → `threads.usage.total_tokens`) × eligible merges. Fewer than 3 past reviews, or `total_tokens` missing: Not measured. Open for PR 1: whether `total_tokens` is tokens spent or context size for each engine.

## 3. Confinement

- Workers run in the lead's checkout today; per-worker worktrees are planned and not built. Workers are kept out of the data folder but keep the network, the keychain and write access to the project (`docs/crew.md`). That is too much for a replay.
- The closer precedent is the escape-check proof (`crew/escapes/check_proof.rs`): a tree from `git archive` of one commit in a fresh folder with no `.git`, a scrubbed environment and `wrap_proof` (no network, writes only in its own folder, credential folders denied).
- A lane gets: the merge commit's tree (`MergeRecord.merge_commit`) and its diff from `before_commit`, read-only; its own run folder for writes; the engine login only. It gets no project checkout, no other secrets and no MCP tools that write. Not available today: `wrap_proof` denies all network, and an engine needs its provider. The build adds a profile that allows only the engine's own traffic, or the lane is refused.
- Findings go to a backtest sink, never to `crew_findings`. The sink keeps the fingerprint and file list that `ReplayedFinding` carries.

## 4. Start, stop, cancel

- Start: a person only, `POST /api/crew/backtest/start` (local only, in `STATE_CHANGING`, Preview). One run at a time (`crew_backtest_create_alone`).
- Stop today only asks (`crew_backtest_ask_stop`): the run ends before its next replay. With live engines one replay can take minutes, so the build adds a cancellation token to `LaneRunner::replay`. A stop kills the engine's process tree (`host/process.rs`), records the lane `stopped` with `STOPPED_BY_PERSON`, and keeps the earlier replays.
- After a crash, `crew_backtest_recover` marks a `running` run failed, "interrupted". This exists today.

## 5. Results and failures

- Stored in the existing tables: `crew_backtest_lanes` (`tokens_used`, `state`, escape counts, `unresolved_n`), `crew_backtest_runs` (`state`, `stop_reason`), `crew_backtest_results` (fingerprint, outcome). Ids, counts and fingerprints only. No new migration.
- One lane failing does not fail the run: `execute` marks it `Failed` and goes on, and the run fails only when every lane fails.
- Not measured, never zero: the engine is refused, not installed or not logged in, the model is unavailable, a budget is 0 (already refused at 1), or the escape log cannot say. Today only the escape log case is a distinct state. The build adds a reason per lane for the others (one nullable column, or `stop_reason` text), so the lane does not show as plain `failed`.

## 6. Run-end contract

`docs/engine-run-contract.md` applies to every replay. No silent kill: a cancel or budget stop is recorded as `stopped` with its reason. A replay with no assistant output, or an ACP reply other than `end_turn`, is `ReplayFailed`, never an empty finding list.

## 7. Decisions (conservative defaults, the owner can change them)

1. Cost: the person confirms the estimate before every run and also sets a hard cap. We enforce what we can: number of replays, wall time, turns per replay, and a lane stops when measured spend passes the cap. A single replay can still overshoot, because no engine takes a token limit; the screen says so. If the estimate is Not measured, the run needs the person's explicit confirm of that fact.
2. Data leaving the machine: old diffs go only to an engine the person already uses for Crew, only when the person starts the run. The confirm dialog names the engine, the number of merges and the size. Never automatic, never from a background trigger.
3. Secrets: a merge whose diff touches a protected or secret-class path from `.orbyn/crew.json`, or matches the existing secret-pattern check, is skipped and shown as Not measured with a plain reason. It is never sent.
4. Flaky engines: at most one automatic retry, for a transport error only. An engine failure makes that replay Not measured, never a scored miss.
5. Tree: replay what the reviewer originally saw, the PR head diff against the merge's first parent. If either cannot be resolved, Not measured.
6. Engines: Claude on macOS now. Codex stays refused. OpenCode only after Settings maps its tier.

## 8. PR split

1. `LiveLaneRunner` over the existing drivers, a backtest finding sink, a cancellation token on `LaneRunner`, the tier check, and per-lane Not measured reasons. Fake-engine tests only. It stays uninstalled.
2. A replay sandbox: a tree from `git archive`, a scrubbed environment, a profile limited to the engine's own network, refused where it cannot apply. Install the runner at core open on macOS only.
3. Token estimate in `BacktestEstimate`, from past review usage, Not measured below 3 samples. Contracts and `web/src/generated` in the same commit.
