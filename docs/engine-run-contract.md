# When an engine run ends

Every driver (Claude, Codex app-server, ACP, OpenCode, AG-UI) follows the same three rules. A run that
breaks one looks like a message with no reply, and the person has to send it twice.

## The contract

1. **A run stays open while background work the engine started is still running**: subagents, workflows,
   background shells. The alternative is to say plainly, before the run ends, that the work stopped. A driver
   never ends the run and silently takes the work down with the engine process.
2. **A run ends only on the turn that answered the person's prompt.** A turn the engine started for its own
   notifications or leftovers (a background agent that finished while the process was down, a shell that
   exited) is shown but does not end the run, whatever it returns, including an error.
3. **A run that said or did nothing is meant to never be plain success.** The core ends it as failed with
   the message "No reply: the engine finished without answering. Send your message again." (see
   `conversation::runs::turn`), and a run whose engine no longer holds the conversation with the message
   "The engine lost this conversation." (see `conversation::runs::execute`). Slash commands and goals are
   exempt: they may have nothing to say. A Crew worker and a subagent are judged by what they leave behind, not by what
   they say, and are exempt too. So is a run on a Crew lead's thread that Crew itself started: its prompt is a message Crew queued
   (a task's result, a review notice, a start it refused), recorded on the queued message (`queued_messages.crew_note`) and copied to the
   run at admission (`runs.crew_note`). The lead's thread is also the person's own chat, so a run the person started there, or a message the
   person queued there, still fails as "No reply". Messages other parts of the app queue on that thread (automations, a delegated
   thread's report) are not Crew's notes and owe a reply too. A run admitted before migration 60 counts as the person's. The one
   exception to all of this is the worker of a background crew's run, whose turn nobody reads live: no reply there ends the run
   `engine_error`.

Rule 3 counts only output that belongs to the prompt's own turn. The core counts a run as having replied on an event
that shows work (`shows_work` in `conversation::runs::turn`): words, a tool start, a permission ask, an activity snapshot,
a question. A driver that can tell a leftover turn from the prompt's wraps what that turn says in `EngineEvent::Leftover`:
the core shows it as it shows any event, and does not count it. A silent prompt turn after a leftover turn that spoke
therefore ends as "No reply". The Claude CLI driver wraps what arrives after a `task_notification` and before the echo of the
prompt; the engine host's Claude family wraps what arrives after the prompt's own `command_lifecycle` `queued` and before its
`started` (the core sends the prompt's `messageId`, the host sends it as the SDK message `uuid`). Codex wraps what a notification names with another turn's `turnId` than the prompt's. A driver with no such
signal wraps nothing, so nothing it says is dropped. ACP runs one process per run and drops another session's updates, so no leftover turn reaches the core.

## How each rule is met

| Engine | Rule 1 (background work) | Rule 2 (the prompt's own turn) |
|---|---|---|
| Claude | `task_started` with type `local_agent` holds the first `result` until the matching `task_notification` or `task_updated` arrives; the follow-up turn then ends the run. A hold whose work never notifies ends the run after 5 minutes of quiet or 30 minutes in all, and the work is reported as `background.stopped` with that reason (idle or limit). A shell (`local_bash`) cannot be held, because a dev server may never end; it is reported as `background.stopped` when the run ends. A workflow is held from the Workflow tool's `async_launched` result (`claude/workflows.rs`), not from `task_started`. | The prompt goes in with a `uuid` and the CLI runs with `--replay-user-messages`. It echoes the prompt as it takes it up; a `result` before the echo, after a turn had begun, belongs to the leftover turn and is ignored. A failure with no turn begun (lost session, no login) still ends the run. If the prompt's turn never follows a leftover result, the run ends on that result after a short grace instead of waiting forever. |
| Codex (app-server) | A sub-agent thread (`thread/started` with a `parentThreadId`, `thread/status/changed`, `thread/closed`, `collabAgentToolCall` items) still running when the parent's `turn/completed` arrives holds the run. The thread's own status outranks any item state. The hold ends when the last agent ends (plus a short wait for the turn its result starts), on cancel, on a person's message (requeued as the next turn), when the engine exits, after 5 minutes without a notification, or after 30 minutes in all. Agents still running at the end are reported as `background.stopped` with a reason. The engine's own review, compact and memory agents are not held. A background shell has no reliable signal and is neither held nor reported. | The run ends only on a `turn/completed` whose turn id is the one started for the prompt (from the `turn/start` reply, or the first `turn/started` on this thread). A different id is ignored; a frame with no id ends the run; a run that never learned its id ends on the first completed turn. Other threads' turns are ignored. |
| ACP, OpenCode (`opencode acp`) | No signal: the protocol has no event for work the agent keeps doing after a turn. The run ends with the prompt's reply and the process is stopped. The driver sees only tool calls still open at the stop, and closes them without a notice (`acp/events.rs`). | The run ends on the `session/prompt` reply matched by request id, from a process started for this run (a resume replays history first and the driver discards it). A `session/update` that names another session is dropped; one with no `sessionId` is taken as ours. A reply without a `stopReason`, or with `refusal`, `max_tokens`, `max_turn_requests` or an unknown one, fails with a plain message; only `end_turn` completes. |
| OpenCode (`opencode serve`, the engine host) | A subagent session (`session.created` with a parent, any depth) still running when the parent's first `session.execution.succeeded` arrives holds the run; the host reports each as `background` `started`/`ended`. The hold ends on the parent's wake turn (the next `session.execution.*` after a subagent's report), 3 s after the last subagent ends with no wake turn, on cancel, after 30 minutes, or when the server stops. A failed or cancelled first result is never held. A shell is reported, never held. | The run ends on the parent's first terminal `session.execution.*` (held as above); other sessions' events are passed as `delta` frames and never end it. |
| AG-UI | No signal: the protocol subset has no background-work event. The run ends at `RUN_FINISHED`. The driver keeps no set of open tool calls and sends no `ToolCallEnd` at the end, so a call still open is not closed by the driver (and no close at run end was found in the core). | Every POST (the first and each resume after an interrupt) is a new run with its own `runId`. A `RUN_FINISHED` or `RUN_ERROR` that names another run or thread is ignored; a frame with no ids still ends the run. |

The app shows each `background.stopped` from Claude (shells, subagents) and Codex (subagents) as a row under
its run; a Claude workflow keeps its own state and gets no row. Nothing in the Codex, ACP, OpenCode-host or AG-UI rows has been run against a real engine (the OpenCode host row comes from replayed 2.0.18 transcripts);
the Codex signals come from the schema of Codex 0.166.0 and the rest from the fake engines.

## Observed on Claude Code 2.1.327

- A resumed session runs a turn for a task notification first, and queues the person's prompt behind it.
  `--replay-user-messages` emits the prompt (`type: user`, `isReplay: true`, same `uuid`) when it is dequeued.
- Backgrounded subagents report `system/task_started` (`task_type: local_agent`, `is_backgrounded: true`) and
  end with `system/task_updated` (`patch.status`) and `system/task_notification` (`status`).
- Exiting the CLI (closing stdin) kills its background agents.

## Tests

Claude and Codex reproduce both steps with their fake engine in `crates/orbynd/src/bin/orbyn_fake_engine/`:
a background task still running when the turn's result arrives, and a leftover turn ahead of the prompt.
The Claude cases are `[agent-finish]` and `[leftover]` in `crates/orbynd/tests/workflows.rs`, and the
mapper unit tests in `driver/claude/tests.rs`. The Codex cases (`[bg-agent-*]`, `[stale-turn]`) are in
`crates/orbynd/tests/drivers.rs` and the driver unit tests under `crates/orbyn-core/src/agents/driver/`.
ACP and AG-UI have no background or leftover case, because their protocols carry no such signal; they test foreign-session
or foreign-run frames (`[stray]`) and the stop-reason and error cases in `crates/orbynd/tests/drivers.rs`.
