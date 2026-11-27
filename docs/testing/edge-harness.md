# Edge harness proposal: store, devices, Claude and Codex suites

Status: **proposal; harnesses H1-H4 are built, order steps 1, 3, 4 and 11 are delivered, and step 10
delivered its harness and one of its two cases.**
Base: `origin/main` `df6cff22e` (2026-11-25); H1/H2 landed on top of `origin/main`
`5aeb2a01f` (2026-11-27), and H3/H4 on top of `origin/main` `bf804077d` (2026-11-27), which
already carried steps 1, 3 and 4. The shared build box was unreachable while this was written, so
it comes from the source and the 24 Nov test ledgers; the delivered steps have since been run on
the box (P3.3 H1/H2 and H3/H4, see Order and
[Verification](#verification-what-needs-the-box)). Everything below not marked delivered is unrun,
and every break-check named there is a plan, not a result.
Counts are measured from the named files (see [Counts](#counts)); no timing is given because none
was measured. This revision corrects a review round: the claims below were re-checked against the
files they name.

Source: `assignments-2026-11-24.md` row 3.3, "Edge harness for store, devices, Claude and Codex
driver suites, so wave 4 can prune with proof" (lanes A, C). Row 3.6 is the consumer: prune a
unit test only where an edge case provably covers it. Row 3.7 (Crew flake isolation) is separate
and is not covered here.

## Why a proposal first

Row 3.3 is four suites whose unit tests mix pure parsing tables with lifecycle rules. The tdd
policy says a unit test survives only when the behaviour is costly to break, unreachable from the
edge, and has many cases (the outer library's tdd skill; not this repository). A suite cannot be
pruned until the edge case exists and is shown red, so this document names the edge case per
group, the invariant it pins, the harness it needs and the order to build them. It also names what
will never be reachable at the edge, so wave 4 does not chase it.

## What the edge already is

A test in `crates/orbynd/tests/` is an edge test here: it starts the real `orbynd` binary over real
HTTP and real subprocess pipes, with engines replaced only at the executable boundary.

| Harness | File | What it already does |
| --- | --- | --- |
| Service | [support/mod.rs](../../crates/orbynd/tests/support/mod.rs) | `Service::start` runs `orbynd serve --data-dir`, sets `ORBYN_CLAUDE_BIN`/`ORBYN_CODEX_BIN` to the fake engine and `ORBYN_OFFLINE=1`; HTTP and SSE helpers; `kill()`; a fresh `Service::start` on the same data folder; `setup()` makes a trusted project and a thread |
| Protocol double | [orbyn_fake_engine/](../../crates/orbynd/src/bin/orbyn_fake_engine/) | one `orbyn-fake-engine` binary speaking Codex app-server ([codex/](../../crates/orbynd/src/bin/orbyn_fake_engine/codex)), Claude stream-json ([claude.rs](../../crates/orbynd/src/bin/orbyn_fake_engine/claude.rs)), ACP, AG-UI, one-shot, PTY and command blocks; frames are written by hand from the real schemas; a prompt tag picks the scenario and `ORBYN_FAKE_*` env knobs pick variants; it asserts some launch flags itself |
| Fake device tools | [devices.rs](../../crates/orbynd/tests/devices.rs) (edge) and [test_support.rs](../../crates/orbyn-core/src/devices/test_support.rs) (unit), both running [devices-fake-tool.js](../../crates/orbynd/tests/fixtures/devices-fake-tool.js) | one Node program for `adb`, `emulator` and `xcrun`, answering from files in its fake dir; the edge puts shims that pass `ORBYN_DEVICES_FAKE_DIR` on the service's `PATH` |
| Crash and restart | [support/mod.rs](../../crates/orbynd/tests/support/mod.rs) + `Service::restart()` and `Service::wait_for_launch(mode)` (H3) | `restart` kills the process and starts a fresh service on the same data folder and options; `wait_for_launch` waits on the fake engine's own `ORBYN_FAKE_LAUNCH_FILE` record, so a case kills at a point the engine recorded, not a sleep. Used by [service.rs](../../crates/orbynd/tests/service.rs) `a_crash_while_an_approval_is_open_closes_the_tool_call_and_interrupts_the_run`; the older kill/start cases [service.rs](../../crates/orbynd/tests/service.rs) `crash_recovery_interrupts_runs_and_expires_approvals`, [composer.rs](../../crates/orbynd/tests/composer.rs) `a_crash_holds_the_queue_across_restart` and `an_interrupted_thread_continues_after_a_restart`, [code_mode_runs.rs](../../crates/orbynd/tests/code_mode_runs.rs) `a_restart_marks_a_run_held_on_an_approval_as_lost` still call `kill()` + `Service::start` directly |
| Old-database fixture | [old_database.rs](../../crates/orbynd/tests/old_database.rs) + [database_rows.sql](../../crates/orbynd/tests/fixtures/database_rows.sql) + `support::old_database` (H4) | schema 1 is a frozen file ([database_v1.sql](../../crates/orbynd/tests/fixtures/database_v1.sql)); later versions are built by `support::old_database(data, through)`, which applies the real `MIGRATIONS` list up to a chosen version, and `support::latest_schema()` names the newest. The real binary upgrades one at start, keeps the rows readable, applies nothing on a second start, and refuses a database newer than the build unchanged (landed 27 Nov 2026) |
| Recorded frames | [driver/fixtures/](../../crates/orbyn-core/src/agents/driver/fixtures/) | `claude-2.1.150-initialize.json`, `codex-0.166.0-model-list.json` and `codex-0.166.0-mcp-server-status.json`, used only by unit tests today |
| Web real-service edge | [web/e2e/real/service.ts](../../web/e2e/real/service.ts) | starts the real binary with `ORBYN_CLAUDE_BIN`/`ORBYN_CODEX_BIN` pointing at [scripted-engine.mjs](../../web/e2e/real/scripted-engine.mjs)/[usage-engine.mjs](../../web/e2e/real/usage-engine.mjs), which replay [scenes.mjs](../../web/e2e/real/scenes.mjs); [stand-in-gh.mjs](../../web/e2e/real/stand-in-gh.mjs) is a fake `gh` on `PATH`. This is the existing precedent for replaying a recorded scene at an edge |

## Counts

Measured on `origin/main` `bf804077d` (which includes P3.6's unit-test prune and H1/H2) plus this
branch's H3/H4 tests, with `grep -c '#\[test\]\|#\[tokio::test' <file>`; these are attributes,
read as tests. The ledgers were written on 24 Nov against an earlier main, so their numbers differ.

| Suite | Unit file | Tests now | Ledger (24 Nov) | Edge tests today |
| --- | --- | --- | --- | --- |
| Store | [store/tests.rs](../../crates/orbyn-core/src/store/tests.rs) | 56 | 55 | [service.rs](../../crates/orbynd/tests/service.rs) 42, [composer.rs](../../crates/orbynd/tests/composer.rs) 18, [old_database.rs](../../crates/orbynd/tests/old_database.rs) 3 |
| Devices | [devices/tests.rs](../../crates/orbyn-core/src/devices/tests.rs) | 67 | 67 | [devices.rs](../../crates/orbynd/tests/devices.rs) 5 |
| Claude driver | [claude/tests.rs](../../crates/orbyn-core/src/agents/driver/claude/tests.rs) | 61 | 59 | [drivers.rs](../../crates/orbynd/tests/drivers.rs) 51 (all engines), [workflows.rs](../../crates/orbynd/tests/workflows.rs) 6, [no_reply.rs](../../crates/orbynd/tests/no_reply.rs) 3 |
| Codex driver | [codex/tests.rs](../../crates/orbyn-core/src/agents/driver/codex/tests.rs) | 55 | 52 | [drivers.rs](../../crates/orbynd/tests/drivers.rs) 51 (all engines), [interactions.rs](../../crates/orbynd/tests/interactions.rs) 36 |

## Where the ledgers are stale

The 24 Nov ledgers understate the edge that exists on main today. Wave 4 must start from these
facts, or it will rebuild harnesses that are already there:

- **Claude has an edge.** `test-ledger-eng2-core.md` (Wave 4 report) says the edge "drives Codex
  and ACP engines, not Claude". On this branch the Claude edge tests are
  `drivers.rs::claude_stream_maps_to_events`, `claude_permission_goes_through_mcp_approve`,
  `claude_denial_reaches_the_engine`, `claude_exit_during_approval_fails_and_expires`,
  `claude_error_result_fails_the_run`, `claude_cancel_closes_the_process`, the task-tools tests,
  all six in `workflows.rs` (workflow hold, script, stop, finished workflow, background agent,
  leftover turn), the three in `no_reply.rs`, the question, denial and lost-session cases in
  `interactions.rs`, the subscription-window and gate cases in `usage.rs`, plus the delivered
  step-1 test `a_claude_run_launches_with_the_documented_flags_and_never_bare`.
- **Devices has a fake-executable edge.** The same ledger says `orbynd/tests/devices.rs` "only
  checks route classes". It now couples the real binary to fake `adb`/`emulator`/`xcrun`
  executables on `PATH` and covers list, detail, a screenshot, the fixed error copy and log
  redaction: `device_reads_are_local_and_every_action_is_person_class`,
  `a_shell_less_service_answers_desktop_required`, `device_errors_have_the_fixed_copy`,
  `a_screenshot_answers_a_png_and_the_log_keeps_no_body_or_image`, and the H2 shared-fake busy case
  `a_second_action_while_one_runs_is_device_busy_and_starts_no_tool`.
- **The crash/restart harness exists, and H3 gives it a kill point.** The store Wave 4 report asks
  for "a crash-and-restart harness in orbynd/tests"; `Service::kill()` plus a second
  `Service::start` on the same data folder already does this in six tests: the four named above
  plus preparation_retry.rs:334 (a crashed preparation restarts as a retryable failure without
  replaying a command) and `remote_mode_needs_tls_assertion_and_exchanges_a_hashed_key`. H3 adds
  `Service::restart()` (the pair with the folder and options carried over) and
  `Service::wait_for_launch(mode)` (the fake engine's own record as the point to kill at), and the
  tool-left-open case below. The missing case is the stop saved before the crash: the stop route
  answers before the engine is down, so the kill lands on either side of the run ending.
- **The old-database fixture covers schema 1 and, since H4, any chosen version.** `old_database.rs`
  closed the ledger's "old-database start from a real old fixture" parked item for the first
  release; `support::old_database` now builds a database at a chosen version from the real
  migration list, so a later version's case no longer needs a static SQL copy of its schema. The
  delivered case starts at 34; the versions the Crew format unit tests cover still need one edge
  case each.
- **The decision race is already at the edge.** `service.rs::approval_decided_twice_keeps_the_first_outcome`
  sends two decisions for one approval and asserts one 200 and one 409 `already_decided`;
  `service.rs::replay_then_live_has_no_gap_or_duplicate` asserts gapless event sequence numbers
  across replay and live. The unit siblings `first_decision_wins_and_duplicate_reports_it` and
  `events_are_gapless_per_thread_and_survive_reopen` are not wave-4 prune candidates on this
  evidence alone (the edge case is weaker: it races two requests, not eight threads).

## Gaps, suite by suite

Cost is build size, not time: **S** = one edge test, no product-code change (a test-support or
fake-engine edit is sometimes still needed, and the row says so); **M** = a fake-engine tag or
test-support helper plus one or two tests; **L** = a new fixture or a visibility change plus one
test per version. Break-checks are stated per row.

### Store

Edge siblings already cover run admission (`service.rs::second_run_on_a_thread_is_409_and_cancel_is_final`),
the crash recovery of a dead owner's run and its approvals
(`crash_recovery_interrupts_runs_and_expires_approvals`), the queue across a crash
(`composer.rs::a_crash_holds_the_queue_across_restart`), a lost Code Mode run
(`code_mode_runs.rs::a_restart_marks_a_run_held_on_an_approval_as_lost`) and the schema-1 upgrade.
These are the units that still have no edge sibling:

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `store::tests::a_stop_saved_before_a_crash_ends_cancelled_rather_than_interrupted` | none | existing kill/restart, killed after the stop route answers | a stop accepted before the process died ends the run `cancelled` on restart, not `interrupted`, and no engine turn starts | S |
| `store::tests::a_tool_left_open_by_a_crash_is_closed_with_a_note` | **delivered** (`service.rs::a_crash_while_an_approval_is_open_closes_the_tool_call_and_interrupts_the_run`) | H3 built: `Service::wait_for_launch("codex-turn")` gives the kill point, then the pending approval, then `Service::restart()` | the open tool call closes with a note on restart, the run is `interrupted`, nothing replays | delivered; break-check applied 27 Nov 2026 |
| `store::tests::migration_versions_run_from_one_with_no_gap_through_the_account_owner`, `migrations_apply_once`, `newer_database_is_refused_unchanged` | schema 1 (`old_database.rs::a_database_from_the_first_release_is_upgraded_at_start_with_its_data_intact`), 34 and one newer than the build (H4: `a_database_from_before_the_account_owner_is_upgraded_at_start_and_keeps_its_rows`, `a_database_newer_than_this_build_is_refused_and_left_untouched`) | H4 built: `support::old_database(data, through)` applies the real migration list up to any chosen version; 34 is the version the named unit test builds | the binary upgrades an old database once, keeps its rows readable through routes, and refuses a newer database unchanged | delivered at 34; L still per later version wave 4 prunes |
| `store::crew_formats_tests::migration_53_keeps_a_scored_lane_and_leaves_unresolved_null`, `migration_57_gives_an_old_background_crew_no_daily_budget_and_keeps_its_runs`, `migration_60_defaults_a_crew_note_to_false_on_old_queues_and_runs`, `migration_69_leaves_an_old_thread_unimported_and_refuses_another_source`, `migration_76_creates_a_manual_check_awaiting_a_person_and_keeps_its_task`, `migration_77_keeps_background_rows_across_reopen_and_lets_a_watch_be_created`, `migration_78_keeps_a_dismissal_to_one_row_and_refuses_a_null_since`, `migration_79_gives_an_old_run_no_resume_and_counts_the_one_after_it` | none beyond schema 1 and 34 | H4 built; one version at a time still to write | a row written by an older format survives to today's schema and is readable through the route, and the upgrade is not applied twice | L per version, only when wave 4 prunes that version's unit test |
| `store::tests::terminal_state_never_changes_even_by_direct_sql`, `audit_is_append_only_at_the_database` | not reachable | none recommended | these are the database's own trigger and append-only rules; a caller cannot reach them over HTTP. Reachable only by editing the file with an external connection between two starts, which tests the trigger, not the product | keep unit |
| `store::tests::stale_owner_cannot_write_run_or_decide` | not reachable | none recommended | it needs two live owners on one data folder; `service.rs::second_service_on_a_data_dir_is_refused` pins that production refuses the second owner, so the unit test guards a case the product cannot create | keep unit |
| `idle_threads_settle_but_recent_or_opted_out_ones_stay`, `due_snooze_wakes_unread_and_early_one_stays` and the other lifecycle rows | none | a clock the service accepts | the service has no test clock and the routes take times from the real clock; moving these to the edge needs a production seam, which 3.3 must not add | keep unit |

### Devices

The unit file is 67 tests over a fake tool program with delays, failures, sizes and foldable modes.
Since H2 that program is one file,
[devices-fake-tool.js](../../crates/orbynd/tests/fixtures/devices-fake-tool.js):
[test_support.rs](../../crates/orbyn-core/src/devices/test_support.rs) includes it with
`include_str!` and runs it with the tool name and fake dir as argv, and
[devices.rs](../../crates/orbynd/tests/devices.rs) includes the same file and puts shims for `adb`,
`emulator` and `xcrun` on the service's `PATH`; the shims pass the same two arguments, and
`ORBYN_DEVICES_FAKE_DIR` names the directory the program answers from. Every knob the unit tests
have is therefore reachable at the edge: the delivered busy case uses `action_delay_ms`. The
trimmed edge copy is gone.

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `a_missing_android_tool_leaves_ios_working`, `a_missing_xcrun_leaves_android_working` | none | no shared fake: start the service with a `PATH` whose helper writes only two of the three shims | one platform's tool missing leaves the other platform's devices listed, and the platform row says unavailable, not failed | S |
| `a_list_timeout_does_not_fail_the_other_platform`, `a_list_timeout_kills_the_tool` | none | H2 built: `android_mode=timeout` knob | the timed-out platform answers its fixed copy, the other platform still lists, and the tool process is dead after the response | M |
| `foldable_detection_is_true_from_the_device_state_rows`, `detail_adds_the_fold_actions_only_for_a_foldable`, `fold_runs_the_emulator_command_and_reads_the_posture_back`, `a_foldable_name_in_the_avd_config_alone_is_not_foldable` | none | H2 built: `android_mode=foldable` and the `fold` state file | a foldable lists fold actions and a fold changes posture; a name alone does not | M |
| `a_second_action_on_one_device_is_device_busy` | **delivered** (`devices.rs::a_second_action_while_one_runs_is_device_busy_and_starts_no_tool`) | H2 built: hold the fake action open (`action_delay_ms`) | a second action while the first runs answers `device_busy` with its fixed copy, and both are logged as attempts | delivered; break-check applied 27 Nov 2026 |
| `an_output_over_one_mib_is_tool_output_too_large`, `a_screenshot_over_sixteen_mib_is_tool_output_too_large`, `a_timed_out_action_is_tool_timeout` | none | H2 built: `action_stdout_bytes`, `screenshot_bytes`, `action_timeout` knobs | the route refuses with the fixed copy instead of buffering or hanging, and no image leaves for an oversized screenshot | S |
| `injection_shaped_ids_are_refused_before_any_process_starts` | only the invalid-body half (`device_errors_have_the_fixed_copy`) | none: the edge route resolves an id against the fake list first, so send an option- or path-shaped id through the action path | an id shaped like an option or a path never becomes argv, and the tool call log (`calls.ndjson`) shows no call | S |

### Claude driver

The edge covers the lifecycle, approvals, plan, task tools, workflow and background-agent holds,
leftover turns, questions, lost sessions and the usage windows. What remains unit-only is the
frame-shape table and the control-channel rules, because the fake writes only the frames the happy
paths need:

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `claude::tests::command_line_matches_the_contract_and_never_uses_bare` | **delivered** (`drivers.rs::a_claude_run_launches_with_the_documented_flags_and_never_bare`) | none: reads the recorded `claude` launch (`ORBYN_FAKE_LAUNCH_FILE`) | every run carries the documented flags in order and never `--bare` or `--plugin-dir`; `--resume`/`--session-id` are not pinned | delivered; unit pruned (P3.6) |
| `a_refused_stop_is_shown_and_other_replies_are_not` | none; the fake answers every `stop_task` with a success `control_response` and records `claude-stop-task`, so the refusal branch (`workflows.rs::control_response`, request id `orbyn-stop-<tool call>`) has no edge case | fake tag `[stop-refused]` answering the `stop_task` with an error `control_response` | a refused workflow stop shows `workflow.stop_failed`, the run stays open, and a control reply for another id stays out | M |
| `quota_events_only_reject_the_turn_when_the_cli_says_rejected` | none; no stream frame carries `rate_limit_info` (the `get_usage` control reply carries `rate_limits` for the usage read only) | fake tag `[quota-rejected]` | only a `rejected` limit ends the turn as limited, with `resetsAt` from the frame; `allowed` does not | S |
| `control_replies_match_ids_and_bound_untrusted_frames` | none | fake env knob that writes a foreign `request_id` and an oversized line around initialize | only the matching id is accepted, an oversized line is refused, and a marker in the error text never reaches a record | M |
| `tool_result_outcomes_preserve_unknown_flags`, `result_usage_adds_every_token_kind`, `partials_are_not_duplicated_by_the_assistant_frame`, `subagent_messages_are_reported_without_joining_the_turn` | none; the fake's result frame carries no usage, and its tool results carry no unknown flag | fake tag per shape (`[tool-flags]`, `[usage-kinds]`, `[partials]`, `[subagent-lines]`) | unknown tool flags survive, token kinds add up, partial text is not duplicated by the full message, subagent text stays out of the top-level turn | M each |
| `a_local_skill_view_is_added_after_the_existing_add_dir_entries`, `the_librarys_settings_and_agents_ride_along_explicitly` | the library/checkout path is at the edge (`in_place.rs::claude_starts_in_the_checkout_and_reaches_the_library_by_path`); the ride-along is not asserted | the fake already records the `--settings` file (`record_launch("claude-settings")`) | the settings file handed to the CLI carries the library's allowed settings and agents, never the permission gate, and adds the skill view after the folder entries | M |

### Codex driver

The edge covers turn matching, holds, subagents, approvals, denial, forced stop, crashes, feedback
and the `/api/engines/codex/limits` read (`usage.rs::limits_come_from_the_engine_and_only_where_it_reports_them`).
The fake emits a hand-written `model/list` page (`codex/mod.rs:366`) that the models read already
parses, and the checked-in recorded page is not replayed. Since H1 the fake also emits the
usage-limit notification and its failure under the `[usage-limited]` tag, and a later app-server
session (the limits read) reports the same spent windows. It still does not emit a retryable error
or the permission schemas. The rows below name the remaining gaps:

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `codex::tests::a_usage_limit_stop_carries_its_reset` | **delivered**: `drivers.rs::a_codex_usage_limit_stop_keeps_the_latest_spent_window_and_its_sentence` and `a_codex_usage_limit_stop_leaves_the_spent_window_for_the_limits_read` | H1 built: tag `[usage-limited]` sends `account/rateLimits/updated` then an `error` with `codexErrorInfo: usageLimitExceeded`; the tag leaves `rate-limits-spent` beside the launch file so a later limits read reports the spent windows | the run ends as limited with the reset from the latest window, not as a generic failure, and `/api/engines/codex/limits` shows the spent window | delivered; both break-checks applied 27 Nov 2026 |
| `an_error_the_engine_will_retry_is_shown_and_does_not_fail_the_turn_that_then_completes` | the fake only sends `willRetry: false` | fake tag `[retry-then-complete]` | a retryable error is shown, the run is still running, the following completed turn ends it successfully | S |
| `approval_replies_match_each_schema`, `a_permissions_request_for_write_roots_is_checked_as_a_file_change`, `an_allowed_permissions_request_never_carries_network_along_with_write_roots` | only command and file-change approvals are emitted; no `item/permissions/requestApproval`, `execCommandApproval` or `applyPatchApproval` | fake tags per method | each request gets its documented answer shape; a write-root request is checked like a file change; allowing it never carries network | M |
| `a_recorded_list_parses_as_the_live_one_will` | the fake's own hand-written `model/list` page; the checked-in recorded page is unit-only | recorded-frame replay: fake reads a page from an env file; the test points it at `crates/orbyn-core/src/agents/driver/fixtures/codex-0.166.0-model-list.json` | the real recorded page parses into the same `/api/engines/codex/models` read as the hand-written one (source `engine`, hidden dropped, efforts from the page) | M |
| `codex::tests::tool_items_report_known_outcomes_only` | none | fake tag `[unknown-outcome]` | an unknown tool outcome is not reported as success or failure | S |

## The harnesses to build

**H1, extend `orbyn-fake-engine`, do not add a binary. Built.** The first tag,
`[usage-limited]` in [codex/turn.rs](../../crates/orbynd/src/bin/orbyn_fake_engine/codex/turn.rs),
sends the two spent `account/rateLimits/updated` windows and the `usageLimitExceeded` failure;
it also leaves `rate-limits-spent` beside the launch file, and
[codex/mod.rs](../../crates/orbynd/src/bin/orbyn_fake_engine/codex/mod.rs) answers a later
`account/rateLimits/read` (the limits route's own short session) with the same spent account. The
rest of the gaps above are one prompt tag or env knob each, added the same way: this keeps one
fake, one launch path and one set of env knobs; a test that needs recorded frames gets them
replayed through an env file (`ORBYN_FAKE_CODEX_PAGE`, `ORBYN_FAKE_CLAUDE_INIT`) rather than
copied into the fake.

**H2, one device fake tool program, two callers. Built.** The program moved to
[devices-fake-tool.js](../../crates/orbynd/tests/fixtures/devices-fake-tool.js);
[test_support.rs](../../crates/orbyn-core/src/devices/test_support.rs) includes it with
`include_str!` and [devices.rs](../../crates/orbynd/tests/devices.rs) does the same, so the two
suites exercise one file and it is the source of truth. The edge keeps `ORBYN_DEVICES_FAKE_DIR`:
its shims pass the tool name and that directory to the same program the unit support invokes with
argv, so a knob added for either suite is reachable from both. The cross-crate `include_str!` path
is judged acceptable: both crates live in this workspace and the file is source, not a build
artifact.

**H3, a signal to wait for before the kill. Built.** The restart is not new: six tests already call
`Service::kill()` and then a fresh `Service::start` on the same data folder. H3 adds
`Service::restart()` — the same pair with the data folder and options carried over, no new
capability — and `Service::wait_for_launch(mode)`, which waits until the fake engine itself recorded
that launch in `ORBYN_FAKE_LAUNCH_FILE` (`shared.rs::record_launch`) and returns the record. The
fake records pids (`ORBYN_FAKE_PID_FILE`, `shared.rs::record_pid`), launches and Claude control
frames (`ORBYN_FAKE_CLAUDE_CONTROL_FILE`), **not approvals**; the launch record is the harness's wait
signal, while the delivered case's kill point is the pending approval (see step 10). Tests wait on the signal, kill at that recorded point, restart, then read runs,
approvals and events through the routes. This is the harness row 3.4 consumes for "crash between
resume commit and prompt send"; 3.3 built it and one case, 3.4 writes the rest.

**H4, an old-database builder at a chosen version. Built.**
[crates/orbyn-core/src/store/migrations.rs](../../crates/orbyn-core/src/store/migrations.rs) is
private (`mod migrations;` in [store/mod.rs](../../crates/orbyn-core/src/store/mod.rs)), and
[crew_formats_tests.rs](../../crates/orbyn-core/src/store/crew_formats_tests.rs) already builds a
database at version N by applying the real migration list to a bare `rusqlite::Connection`
(`fn database(dir, through)`). `store/mod.rs` now re-exports `MIGRATIONS` under the existing
`test-support` feature (`#[cfg(feature = "test-support")] pub use migrations::MIGRATIONS;`; nothing
in a normal build reads it), and `orbynd/tests/support` adds `old_database(data, through)` — the
same builder, writing the file the service opens — plus `latest_schema()`. A case builds the
schema, inserts its rows ([database_rows.sql](../../crates/orbynd/tests/fixtures/database_rows.sql))
and starts the real binary. Static SQL fixtures for later versions would go stale; the source of
truth stays `MIGRATIONS`, and the frozen schema-1 file stays only as the first release's record.

## Order

Cheapest first, each independent, none of them a change to product behaviour: order steps 1, 3, 4
and 11 are delivered and step 10 delivered its harness and one of its two cases; the remaining
steps are test code and fake-engine tags (the test double under `crates/orbynd/src/bin/`). The
missing-tool case needs no harness: a `PATH` without one shim is enough. The only `src/` edits the
delivered work needed are `#[cfg]`-gated test support: the fake engine's own binary, and H4's
`test-support` re-export.

1. **Claude command line at the edge** (Claude table, delivered):
   `drivers.rs::a_claude_run_launches_with_the_documented_flags_and_never_bare` reads the recorded
   launch and was run green on the box. Break-check run on 27 Nov 2026 (P3.6): deleting
   `--replay-user-messages` from `run_args` fails it, and the covered unit test was pruned.
2. **Devices missing tool** (Devices table, S): a `PATH` whose helper writes only two of the three
   shims; no H2 needed. Break-check: make the missing-tool check report the platform available, so
   the unavailable row cannot appear and the test fails.
3. **Codex usage limit stop** (Codex table, delivered with H1): one fake tag in
   `crates/orbynd/src/bin/orbyn_fake_engine/codex/`, two tests:
   `drivers.rs::a_codex_usage_limit_stop_keeps_the_latest_spent_window_and_its_sentence` and
   `a_codex_usage_limit_stop_leaves_the_spent_window_for_the_limits_read`. Break-checks run
   27 Nov 2026: taking the earliest spent window (`min` for `max` in
   `codex/limits.rs::spent_window_reset`) fails the reset assertion; reporting the old 12% window
   to the limits read fails the second test with `left: Number(12.0)`.
4. **Devices busy** (Devices table, delivered with H2): the delay knob H2 brings, then the test
   `devices.rs::a_second_action_while_one_runs_is_device_busy_and_starts_no_tool`. Break-check run
   27 Nov 2026: removing the one-writer guard in `devices/mod.rs::try_busy` answers the second
   action `200` and the test fails on the status assertion.
5. **Codex retry, unknown outcome, recorded page** (Codex table, S/M): three tests on the same fake.
6. **Claude stop refused, quota rejected, control ids** (Claude table, M): three tags; these are the
   first real prune enablers for the Claude frame table.
7. **Claude shape tables and ride-along settings** (Claude table, M each): four tags, then the
   settings test.
8. **Codex permission and legacy approval schemas** (Codex table, M): one tag per method.
9. **Devices timeout, foldable, sizes** (Devices table, M): H2's shared program already holds the
   knobs; the cases are still unwritten.
10. **Store signal wait and two cases** (Store table, S/M): H3 built, one case delivered
    (`service.rs::a_crash_while_an_approval_is_open_closes_the_tool_call_and_interrupts_the_run`).
    It waits for the pending approval - the deterministic kill point (`wait_for_launch` polls its
    record with a 10 s cap and is redundant for this synchronous handshake) - then kills, restarts,
    and asserts the open tool call closed with "Interrupted before a result.", the run is
    `interrupted`, the approval expired, and no engine turn started (the service turns
    `continue_threads_after_update` off, service.rs:635-644; the product default would start a
    continuation run). Break-check run 27 Nov 2026:
    removing `close_open_tools` from `Store::recover` fails it on the missing close. The other case,
    the stop saved before the crash
    (`a_stop_saved_before_a_crash_ends_cancelled_rather_than_interrupted`), is still unwritten: the
    stop route answers before the engine is down, so the kill lands on either side of the run
    ending; it needs a fake-engine hold after the interrupt, or a service-side signal that the stop
    is saved and the run is still open. The launch record alone cannot tell those apart.
11. **Old-database builder and versions** (Store table, L): H4 built; the test starts at 34 (the
    version the named unit test builds, before migration 35 adds the account owner) and a second
    test refuses a database one version newer. Break-checks run 27 Nov 2026: making `refuse_newer`
    accept the future version fails the refusal test; returning no upgrade from `migrate` fails the
    old-database test on the missing announcement. One version per prune, not all 89.

## What this will not cover

- **Live engines and real tools.** The fake's frames are hand-written from a schema and every device
  tool is a Node stand-in. The checked-in recorded frames are three files; a real Claude or Codex turn, a real
  `adb`/`xcrun`, and real provider acceptance remain the `live_` ignored tests and box evidence.
- **Direct database rules**: terminal states immutable even by direct SQL, audit append-only, and the
  lane constraints. A caller cannot reach them over HTTP; a unit test at the database is the right
  owner.
- **Two live owners on one data folder.** Production refuses the second service
  (`service.rs::second_service_on_a_data_dir_is_refused`), so a stale live owner cannot be built at
  the edge.
- **The lifecycle clock** (settle, snooze, automatic continuations). The service exposes no test
  clock in the source today; moving these to the edge is a production seam, and 3.3 adds none.
- **Windows-only branches** (junctions, `.cmd` shims, path separators) and device emulators for real
  Android/iOS hardware. The CI matrix and the Windows tasks own those; the fake tool shims have only
  been exercised on Linux here, so their Windows branch is unobserved.
- **Web UI and the web suites.** [web/e2e/real/](../../web/e2e/real) has its own harness and the web
  ledger's wave 4 works from it.
- **Crew flake isolation (3.7)** and the store's slow Crew tests; separate row, separate owner.
- **The cases row 3.4 owns** (resume at the limit, crash between the resume commit and prompt send,
  cross-project secrets, person key absent from argv, `install.ps1`). 3.3 builds harnesses; 3.4 writes
  those cases.

## Ledger parked items this answers or leaves

| Ledger parked item | This proposal |
| --- | --- |
| Eng 2, store: "a crash-and-restart harness in orbynd/tests that kills the process mid-run, restarts, and reads runs, approvals and audit through the HTTP routes" | H3 built; the restart and the read side already existed, and `Service::wait_for_launch`/`Service::restart` add the kill point and the carried-over restart |
| Eng 2, devices: "An orbynd test with fake `adb`/`xcrun` executables on PATH ... replaces the async list/detail cases but not the pure parsers or the two path-safety cases" | H2 and the device table; the parsers and the two path-safety cases stay unit |
| Eng 2, Claude: "the edge drives Codex and ACP engines, not Claude" | stale (see above); the remaining gaps are the frame-shape table, not the lifecycle |
| Eng 2, Codex: "Extend the fake codex in drivers.rs to emit usage-limit, catalog and effort frames; only then could the usage/catalog/schema cases go" | the catalog frame already exists; the usage-limit turn stop, retry and permission schemas are H1 (Codex table) |
| Eng 3: "No test that the person key/token is absent from `orbynd` argv/env" | row 3.4's case; 3.3 does not add it |
| Eng 3: "old-database start from a real old fixture" | closed for schema 1 by `old_database.rs` and for a chosen later version by H4's builder (delivered at 34); versions later than 34 still need a case each |
| Eng 3: "`CoreProcess::stop` and the crash `on_exit` path ... needs the desktop crate" | out of scope (desktop shell, not this edge) |
| Eng 4: "ACP rate limit / no-result stream: needs a fake agent process speaking ACP" | the fake engine already has `acp.rs`; an ACP scenario is adjacent, not part of row 3.3's four suites |
| Eng 4: "Code Mode across a real process boundary" | `code_mode_runs.rs::a_restart_marks_a_run_held_on_an_approval_as_lost` already kills and restarts the process; remaining replay cases are unit only |
| Web ledger wave 4 (empty-DOM-after-404 helper, duplicated route matrices) | web suite, not this document |

## Verification: what needs the box

Step 1's test was run green on the box (1 passed, 48 filtered out), and its break-check and the
unit-test prune it enabled were run on 27 Nov 2026 with wave 4 (P3.6; box results are in the
wave-4 line of the test ledgers). Steps 3 and 4's tests and break-checks ran on the box on
27 Nov 2026 (P3.3 H1/H2, branch `test/edge-h1-h2` from `origin/main` `5aeb2a01f`):

| Command | Result |
| --- | --- |
| `cargo test -p orbynd --test drivers -- a_codex_usage_limit_stop` | 2 passed, 0 failed (49 filtered) |
| `cargo test -p orbynd --test devices` | 5 passed, 0 failed |
| `cargo test -p orbyn-core --lib devices:: -- --test-threads=8` | 67 passed, 0 failed |
| `cargo test -p orbyn-core --lib tools::mcp::tools -- --test-threads=8` | 85 passed, 0 failed (the other caller of the shared device fake) |
| `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` | both exit 0 |

H3/H4's tests and break-checks ran on the box on 27 Nov 2026 (P3.3 H3/H4, branch `test/edge-h3-h4`
from `origin/main` `bf804077d`):

| Command | Result |
| --- | --- |
| `cargo test -p orbynd --test old_database` | 3 passed, 0 failed (schema 1, the new 34→latest case, and the newer-than-build refusal) |
| `cargo test -p orbynd --test service` | 41 passed, 0 failed (Linux; `folder_browser_skips_windows_hidden_system_and_junction_folders` is compiled out, while the `#[cfg(unix)]` sidecar case ran) |
| `cargo test -p orbynd --test project_icon_metadata` | 9 passed, 0 failed (its custom-spawn service now goes through `Service::adopt`) |
| `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` | both exit 0 |

H3's break-check: commenting out `close_open_tools` in `Store::recover` (so a dead process's open
tool call is not closed) fails the delivered test on the missing close. H4's: `refuse_newer`
returning `Ok(())` fails the refusal test ("a database newer than this build is refused"); setting
`migrate`'s `before` to 0 fails the old-database test with `left: Null` against `34`. Each was
applied alone, failed for that reason, and the file was restored.

The break-checks were applied one at a time, each failed the delivered test for the stated reason,
and the files were restored. No product behaviour changed: the new frames and the shared JS program
are test doubles, and the only `src/` edit is `#[cfg]`-gated (H4's `#[cfg(feature = "test-support")]`
re-export of `MIGRATIONS`, which a normal build never sees; the fake engine is untouched on this branch).
Windows behaviour of the device fake and the `.cmd` shims is unobserved here; the CI matrix owns it.
