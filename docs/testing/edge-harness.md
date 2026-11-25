# Edge harness proposal: store, devices, Claude and Codex suites

Status: **proposal; step 1 of the order is delivered.** Base: `origin/main` `df6cff22e`
(2026-11-25), plus the delivered step-1 test on this branch. The shared build box was unreachable
while this was written, so it comes from the source and the 24 Nov test ledgers; step 1's test has
since been run on the box (`cargo test -p orbynd --test drivers --
a_claude_run_launches_with_the_documented_flags_and_never_bare`: 1 passed, 48 filtered out).
Everything else below is unrun, and every break-check named is a plan, not a result. Counts are
measured from the named files (see [Counts](#counts)); no timing is given because none was
measured. This revision corrects a review round: the claims below were re-checked against the
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
| Fake device tools | [devices.rs](../../crates/orbynd/tests/devices.rs) (edge) and [test_support.rs](../../crates/orbyn-core/src/devices/test_support.rs) (unit) | Node stand-ins for `adb`, `emulator` and `xcrun` on the service's `PATH`, answering from files in `ORBYN_DEVICES_FAKE_DIR`; the unit one is richer |
| Crash and restart | [support/mod.rs](../../crates/orbynd/tests/support/mod.rs) + `Service::kill()` | used today by [service.rs](../../crates/orbynd/tests/service.rs) `crash_recovery_interrupts_runs_and_expires_approvals`, [composer.rs](../../crates/orbynd/tests/composer.rs) `a_crash_holds_the_queue_across_restart` and `an_interrupted_thread_continues_after_a_restart`, [code_mode_runs.rs](../../crates/orbynd/tests/code_mode_runs.rs) `a_restart_marks_a_run_held_on_an_approval_as_lost` |
| Old-database fixture | [old_database.rs](../../crates/orbynd/tests/old_database.rs) + [database_v1.sql](../../crates/orbynd/tests/fixtures/database_v1.sql) | a schema-1 database opened by the real binary: data survives, one upgrade is announced, a second start applies nothing (landed `f3bb2c74a`, 2026-11-24) |
| Recorded frames | [driver/fixtures/](../../crates/orbyn-core/src/agents/driver/fixtures/) | `claude-2.1.150-initialize.json`, `codex-0.166.0-model-list.json` and `codex-0.166.0-mcp-server-status.json`, used only by unit tests today |
| Web real-service edge | [web/e2e/real/service.ts](../../web/e2e/real/service.ts) | starts the real binary with `ORBYN_CLAUDE_BIN`/`ORBYN_CODEX_BIN` pointing at [scripted-engine.mjs](../../web/e2e/real/scripted-engine.mjs)/[usage-engine.mjs](../../web/e2e/real/usage-engine.mjs), which replay [scenes.mjs](../../web/e2e/real/scenes.mjs); [stand-in-gh.mjs](../../web/e2e/real/stand-in-gh.mjs) is a fake `gh` on `PATH`. This is the existing precedent for replaying a recorded scene at an edge |

## Counts

Measured on this branch (base `df6cff22e` plus the delivered step-1 test) with
`grep -c '#\[test\]\|#\[tokio::test' <file>`; these are attributes, read as tests. The ledgers were
written on 24 Nov against an earlier main, so their numbers differ.

| Suite | Unit file | Tests now | Ledger (24 Nov) | Edge tests today |
| --- | --- | --- | --- | --- |
| Store | [store/tests.rs](../../crates/orbyn-core/src/store/tests.rs) | 56 | 55 | [service.rs](../../crates/orbynd/tests/service.rs) 41, [composer.rs](../../crates/orbynd/tests/composer.rs) 18, [old_database.rs](../../crates/orbynd/tests/old_database.rs) 1 |
| Devices | [devices/tests.rs](../../crates/orbyn-core/src/devices/tests.rs) | 68 | 67 | [devices.rs](../../crates/orbynd/tests/devices.rs) 4 |
| Claude driver | [claude/tests.rs](../../crates/orbyn-core/src/agents/driver/claude/tests.rs) | 62 | 59 | [drivers.rs](../../crates/orbynd/tests/drivers.rs) 49 (all engines), [workflows.rs](../../crates/orbynd/tests/workflows.rs) 6, [no_reply.rs](../../crates/orbynd/tests/no_reply.rs) 3 |
| Codex driver | [codex/tests.rs](../../crates/orbyn-core/src/agents/driver/codex/tests.rs) | 56 | 52 | [drivers.rs](../../crates/orbynd/tests/drivers.rs) 49 (all engines), [interactions.rs](../../crates/orbynd/tests/interactions.rs) 36 |

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
  `a_screenshot_answers_a_png_and_the_log_keeps_no_body_or_image`.
- **The crash/restart harness exists.** The store Wave 4 report asks for "a crash-and-restart
  harness in orbynd/tests"; `Service::kill()` plus a second `Service::start` on the same data
  folder already does this in five tests: the four named above plus
  `remote_mode_needs_tls_assertion_and_exchanges_a_hashed_key`. The restart is not new; what is
  missing is a signal a test can wait for before the kill (H3), and the cases.
- **The old-database fixture exists for schema 1.** `old_database.rs` closed the ledger's
  "old-database start from a real old fixture" parked item for the first release; later versions
  are still unit-only.
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
| `store::tests::a_tool_left_open_by_a_crash_is_closed_with_a_note` | none | existing kill/restart; kill while a tool approval is being answered | the open tool call closes with a note on restart, the run is `interrupted`, nothing replays | M |
| `store::tests::migration_versions_run_from_one_with_no_gap_through_the_account_owner`, `migrations_apply_once`, `newer_database_is_refused_unchanged` | schema 1 only (`old_database.rs::a_database_from_the_first_release_is_upgraded_at_start_with_its_data_intact`) | H4 old-database builder at a chosen version, plus one fixture newer than the latest schema | the binary upgrades an old database once, keeps its rows readable through routes, and refuses a newer database unchanged | L |
| `store::crew_formats_tests::migration_53_keeps_a_scored_lane_and_leaves_unresolved_null`, `migration_57_gives_an_old_background_crew_no_daily_budget_and_keeps_its_runs`, `migration_60_defaults_a_crew_note_to_false_on_old_queues_and_runs`, `migration_69_leaves_an_old_thread_unimported_and_refuses_another_source`, `migration_76_creates_a_manual_check_awaiting_a_person_and_keeps_its_task`, `migration_77_keeps_background_rows_across_reopen_and_lets_a_watch_be_created`, `migration_78_keeps_a_dismissal_to_one_row_and_refuses_a_null_since`, `migration_79_gives_an_old_run_no_resume_and_counts_the_one_after_it` | none beyond schema 1 | H4, one version at a time | a row written by an older format survives to today's schema and is readable through the route, and the upgrade is not applied twice | L per version, only when wave 4 prunes that version's unit test |
| `store::tests::terminal_state_never_changes_even_by_direct_sql`, `audit_is_append_only_at_the_database` | not reachable | none recommended | these are the database's own trigger and append-only rules; a caller cannot reach them over HTTP. Reachable only by editing the file with an external connection between two starts, which tests the trigger, not the product | keep unit |
| `store::tests::stale_owner_cannot_write_run_or_decide` | not reachable | none recommended | it needs two live owners on one data folder; `service.rs::second_service_on_a_data_dir_is_refused` pins that production refuses the second owner, so the unit test guards a case the product cannot create | keep unit |
| `idle_threads_settle_but_recent_or_opted_out_ones_stay`, `due_snooze_wakes_unread_and_early_one_stays` and the other lifecycle rows | none | a clock the service accepts | the service has no test clock and the routes take times from the real clock; moving these to the edge needs a production seam, which 3.3 must not add | keep unit |

### Devices

The unit file is 68 tests over a fake tool program with delays, failures, sizes and foldable modes
([test_support.rs](../../crates/orbyn-core/src/devices/test_support.rs)). The edge fake in
[devices.rs](../../crates/orbynd/tests/devices.rs) is a trimmed copy of the same idea: it answers
list/detail/screenshot and can fail an action (the fake's `action_exit` makes a tool exit 2, and
`device_errors_have_the_fixed_copy` asserts the `tool_failed` copy through it). It cannot delay or
time out, oversize its output, list under a missing tool, or pretend to be foldable.

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `a_missing_android_tool_leaves_ios_working`, `a_missing_xcrun_leaves_android_working` | none | no shared fake: start the service with a `PATH` whose helper writes only two of the three shims | one platform's tool missing leaves the other platform's devices listed, and the platform row says unavailable, not failed | S |
| `a_list_timeout_does_not_fail_the_other_platform`, `a_list_timeout_kills_the_tool` | none | H2: `android_mode=timeout` knob | the timed-out platform answers its fixed copy, the other platform still lists, and the tool process is dead after the response | M |
| `foldable_detection_is_true_from_the_device_state_rows`, `detail_adds_the_fold_actions_only_for_a_foldable`, `fold_runs_the_emulator_command_and_reads_the_posture_back`, `a_foldable_name_in_the_avd_config_alone_is_not_foldable` | none | H2: `android_mode=foldable` and the `fold` state file | a foldable lists fold actions and a fold changes posture; a name alone does not | M |
| `a_second_action_on_one_device_is_device_busy` | none | H2: hold the fake action open (`action_delay_ms`) | a second action while the first runs answers `device_busy` with its fixed copy, and both are logged as attempts | M |
| `an_output_over_one_mib_is_tool_output_too_large`, `a_screenshot_over_sixteen_mib_is_tool_output_too_large`, `a_timed_out_action_is_tool_timeout` | none | H2: `action_stdout_bytes`, `screenshot_bytes`, `action_timeout` knobs | the route refuses with the fixed copy instead of buffering or hanging, and no image leaves for an oversized screenshot | S |
| `injection_shaped_ids_are_refused_before_any_process_starts` | only the invalid-body half (`device_errors_have_the_fixed_copy`) | none: the edge route resolves an id against the fake list first, so send an option- or path-shaped id through the action path | an id shaped like an option or a path never becomes argv, and the tool call log (`calls.ndjson`) shows no call | S |

### Claude driver

The edge covers the lifecycle, approvals, plan, task tools, workflow and background-agent holds,
leftover turns, questions, lost sessions and the usage windows. What remains unit-only is the
frame-shape table and the control-channel rules, because the fake writes only the frames the happy
paths need:

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `claude::tests::command_line_matches_the_contract_and_never_uses_bare` | **delivered** (`drivers.rs::a_claude_run_launches_with_the_documented_flags_and_never_bare`) | none: reads the recorded `claude` launch (`ORBYN_FAKE_LAUNCH_FILE`) | every run carries the documented flags in order and never `--bare` or `--plugin-dir`; `--resume`/`--session-id` are not pinned | delivered |
| `a_refused_stop_is_shown_and_other_replies_are_not` | none; the fake answers every `stop_task` with a success `control_response` and records `claude-stop-task`, so the refusal branch (`workflows.rs::control_response`, request id `orbyn-stop-<tool call>`) has no edge case | fake tag `[stop-refused]` answering the `stop_task` with an error `control_response` | a refused workflow stop shows `workflow.stop_failed`, the run stays open, and a control reply for another id stays out | M |
| `quota_events_only_reject_the_turn_when_the_cli_says_rejected` | none; no stream frame carries `rate_limit_info` (the `get_usage` control reply carries `rate_limits` for the usage read only) | fake tag `[quota-rejected]` | only a `rejected` limit ends the turn as limited, with `resetsAt` from the frame; `allowed` does not | S |
| `control_replies_match_ids_and_bound_untrusted_frames` | none | fake env knob that writes a foreign `request_id` and an oversized line around initialize | only the matching id is accepted, an oversized line is refused, and a marker in the error text never reaches a record | M |
| `tool_result_outcomes_preserve_unknown_flags`, `result_usage_adds_every_token_kind`, `partials_are_not_duplicated_by_the_assistant_frame`, `subagent_messages_are_reported_without_joining_the_turn` | none; the fake's result frame carries no usage, and its tool results carry no unknown flag | fake tag per shape (`[tool-flags]`, `[usage-kinds]`, `[partials]`, `[subagent-lines]`) | unknown tool flags survive, token kinds add up, partial text is not duplicated by the full message, subagent text stays out of the top-level turn | M each |
| `a_local_skill_view_is_added_after_the_existing_add_dir_entries`, `the_librarys_settings_and_agents_ride_along_explicitly` | the library/checkout path is at the edge (`in_place.rs::claude_starts_in_the_checkout_and_reaches_the_library_by_path`); the ride-along is not asserted | the fake already records the `--settings` file (`record_launch("claude-settings")`) | the settings file handed to the CLI carries the library's allowed settings and agents, never the permission gate, and adds the skill view after the folder entries | M |

### Codex driver

The edge covers turn matching, holds, subagents, approvals, denial, forced stop, crashes, feedback
and the `/api/engines/codex/limits` read (`usage.rs::limits_come_from_the_engine_and_only_where_it_reports_them`).
The fake emits a hand-written `model/list` page (`codex/mod.rs:366`) that the models read already
parses, and the checked-in recorded page is not replayed; it does not emit the usage-limit or
retryable-error notifications. The rows below name those gaps:

| Unit test (real name) | Edge today | Harness needed | Invariant to pin | Cost |
| --- | --- | --- | --- | --- |
| `codex::tests::a_usage_limit_stop_carries_its_reset` | limits read only; no turn stop | fake tag `[usage-limited]` sending `account/rateLimits/updated` then an `error` with `codexErrorInfo: usageLimitExceeded` | the run ends as limited with the reset from the latest window, not as a generic failure, and `/api/engines/codex/limits` shows the spent window | M |
| `an_error_the_engine_will_retry_is_shown_and_does_not_fail_the_turn_that_then_completes` | the fake only sends `willRetry: false` | fake tag `[retry-then-complete]` | a retryable error is shown, the run is still running, the following completed turn ends it successfully | S |
| `approval_replies_match_each_schema`, `a_permissions_request_for_write_roots_is_checked_as_a_file_change`, `an_allowed_permissions_request_never_carries_network_along_with_write_roots` | only command and file-change approvals are emitted; no `item/permissions/requestApproval`, `execCommandApproval` or `applyPatchApproval` | fake tags per method | each request gets its documented answer shape; a write-root request is checked like a file change; allowing it never carries network | M |
| `a_recorded_list_parses_as_the_live_one_will` | the fake's own hand-written `model/list` page; the checked-in recorded page is unit-only | recorded-frame replay: fake reads a page from an env file; the test points it at `crates/orbyn-core/src/agents/driver/fixtures/codex-0.166.0-model-list.json` | the real recorded page parses into the same `/api/engines/codex/models` read as the hand-written one (source `engine`, hidden dropped, efforts from the page) | M |
| `codex::tests::tool_items_report_known_outcomes_only` | none | fake tag `[unknown-outcome]` | an unknown tool outcome is not reported as success or failure | S |

## The harnesses to build

**H1, extend `orbyn-fake-engine`, do not add a binary.** Most gaps above are one prompt tag plus
the frames it sends, in [claude.rs](../../crates/orbynd/src/bin/orbyn_fake_engine/claude.rs) or
[codex/](../../crates/orbynd/src/bin/orbyn_fake_engine/codex); the recorded-page and control-id
cases use an env knob instead. This keeps one fake, one launch path and one set of env knobs: a
test that needs the recorded frames gets them replayed through an env file
(`ORBYN_FAKE_CODEX_PAGE`, `ORBYN_FAKE_CLAUDE_INIT`) rather than copied into the fake.

**H2, one device fake tool program, two callers.** The unit fake
([test_support.rs](../../crates/orbyn-core/src/devices/test_support.rs)) has most of the knobs the
remaining rows need. Move the program to `crates/orbynd/tests/fixtures/devices-fake-tool.js`, include it from
the unit support with `include_str!` and from the edge test the same way, so the two suites
exercise one file. The edge keeps `ORBYN_DEVICES_FAKE_DIR`; the unit tests keep their existing fake
directory. If the cross-crate `include_str!` is judged fragile, keep the copy and accept the drift,
but say which file is the source.

**H3, a signal to wait for before the kill.** The restart is not new: five tests already call
`Service::kill()` and then a fresh `Service::start` on the same data folder. What a crash case
needs is a point it can wait for instead of a sleep, plus a small helper that carries the data
folder and options across the restart (a `Service::restart` convenience over the existing pair, not
a new capability). The fake records pids (`ORBYN_FAKE_PID_FILE`, `shared.rs::record_pid`), launches
(`ORBYN_FAKE_LAUNCH_FILE`, `shared.rs::record_launch`) and Claude control frames
(`ORBYN_FAKE_CLAUDE_CONTROL_FILE`), **not approvals**, so a wait for "the approval was asked" has no
signal today; the pid or launch file is the usable signal, and an approval-asked wait needs a new
fake-side record. Tests wait on the signal, call `kill()`, restart, then read runs, approvals and
events through the routes. This is the harness row 3.4 consumes for "crash between resume commit and
prompt send"; 3.3 builds it and one case, 3.4 writes the rest.

**H4, an old-database builder at a chosen version.**
[crates/orbyn-core/src/store/migrations.rs](../../crates/orbyn-core/src/store/migrations.rs) is
private (`mod migrations;` in [store/mod.rs](../../crates/orbyn-core/src/store/mod.rs)), and
[crew_formats_tests.rs](../../crates/orbyn-core/src/store/crew_formats_tests.rs) already builds a
database at version N by applying the real migration list to a bare `rusqlite::Connection`
(`fn database(dir, through)`). Expose the migration list under the existing `test-support` feature
(the feature already gates test helpers for integration tests; nothing exposes this list today), add
the same builder to `orbynd/tests/support`, insert the fixture rows and hand the file to the real
binary. This replaces static SQL fixtures that would go stale: the source of truth stays
`MIGRATIONS`.

## Order

Cheapest first, each independent, none of them a change to product behaviour: step 1 is delivered;
the rest are test code, fake-engine tags (the test double under `crates/orbynd/src/bin/`), or the H4
visibility change. The missing-tool case needs no harness: a `PATH` without one shim is enough.

1. **Claude command line at the edge** (Claude table, delivered):
   `drivers.rs::a_claude_run_launches_with_the_documented_flags_and_never_bare` reads the recorded
   launch and was run green on the box. Break-check still planned: delete `--replay-user-messages`
   from `run_args` and rerun.
2. **Devices missing tool** (Devices table, S): a `PATH` whose helper writes only two of the three
   shims; no H2 needed. Break-check: make the missing-tool check report the platform available, so
   the unavailable row cannot appear and the test fails.
3. **Codex usage limit stop** (Codex table, M): one fake tag in
   `crates/orbynd/src/bin/orbyn_fake_engine/codex/`, one test. Break-check: stop sending
   `account/rateLimits/updated`; the limited run loses its reset and the test fails.
4. **Devices busy** (Devices table, M): the delay knob H2 brings, then the test. Break-check:
   remove the `device_busy` guard; the second-action test fails.
5. **Codex retry, unknown outcome, recorded page** (Codex table, S/M): three tests on the same fake.
6. **Claude stop refused, quota rejected, control ids** (Claude table, M): three tags; these are the
   first real prune enablers for the Claude frame table.
7. **Claude shape tables and ride-along settings** (Claude table, M each): four tags, then the
   settings test.
8. **Codex permission and legacy approval schemas** (Codex table, M): one tag per method.
9. **Devices timeout, foldable, sizes** (Devices table, M): finish H2's knobs.
10. **Store signal wait and two cases** (Store table, S/M): H3.
11. **Old-database builder and versions** (Store table, L): H4, one version per prune, not all 89.

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
| Eng 2, store: "a crash-and-restart harness in orbynd/tests that kills the process mid-run, restarts, and reads runs, approvals and audit through the HTTP routes" | H3; the restart and the read side already exist, the wait-for-the-point signal is the addition |
| Eng 2, devices: "An orbynd test with fake `adb`/`xcrun` executables on PATH ... replaces the async list/detail cases but not the pure parsers or the two path-safety cases" | H2 and the device table; the parsers and the two path-safety cases stay unit |
| Eng 2, Claude: "the edge drives Codex and ACP engines, not Claude" | stale (see above); the remaining gaps are the frame-shape table, not the lifecycle |
| Eng 2, Codex: "Extend the fake codex in drivers.rs to emit usage-limit, catalog and effort frames; only then could the usage/catalog/schema cases go" | the catalog frame already exists; the usage-limit turn stop, retry and permission schemas are H1 (Codex table) |
| Eng 3: "No test that the person key/token is absent from `orbynd` argv/env" | row 3.4's case; 3.3 does not add it |
| Eng 3: "old-database start from a real old fixture" | half closed by `old_database.rs`; H4 closes later versions |
| Eng 3: "`CoreProcess::stop` and the crash `on_exit` path ... needs the desktop crate" | out of scope (desktop shell, not this edge) |
| Eng 4: "ACP rate limit / no-result stream: needs a fake agent process speaking ACP" | the fake engine already has `acp.rs`; an ACP scenario is adjacent, not part of row 3.3's four suites |
| Eng 4: "Code Mode across a real process boundary" | `code_mode_runs.rs::a_restart_marks_a_run_held_on_an_approval_as_lost` already kills and restarts the process; remaining replay cases are unit only |
| Web ledger wave 4 (empty-DOM-after-404 helper, duplicated route matrices) | web suite, not this document |

## Verification: what needs the box

Step 1's test was run green on the box (1 passed, 48 filtered out). Everything else in this document
is unverified. On the box, for each added test: run it green, then apply the named break-check and
confirm it fails for the stated reason, then restore. Step 1's break-check is still a plan, and the
tip's `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass was
not run when the box went away; run both with the next box round, plus the four unit files after any
prune and the touched edge test files (`cargo test -p orbynd --test drivers`, and so on). Windows
behaviour of the device fake and the `.cmd` shims is unobserved here; the CI matrix owns it.
