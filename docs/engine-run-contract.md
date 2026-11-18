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
3. **A run that produced no assistant output is never plain success.** The core ends it as failed with
   the message "No reply: the engine finished without answering. Send your message again." (see
   `conversation::runs::turn`). Slash commands and goals are exempt: they may have nothing to say.

## How each rule is met

| Engine | Rule 1 (background work) | Rule 2 (the prompt's own turn) |
|---|---|---|
| Claude | `task_started` with type `local_agent`, and `local_workflow`, hold the first `result` until the matching `task_notification` or `task_updated` arrives; the follow-up turn then ends the run. A shell (`local_bash`) cannot be held, because a dev server may never end; it is reported as `background.stopped` when the run ends. | The prompt goes in with a `uuid` and the CLI runs with `--replay-user-messages`. It echoes the prompt as it takes it up; a `result` before the echo, after a turn had begun, belongs to the leftover turn and is ignored. A failure with no turn begun (lost session, no login) still ends the run. If the prompt's turn never follows a leftover result, the run ends on that result after a short grace instead of waiting forever. |
| Codex (app-server) | A sub-agent thread (`thread/started` with a `parentThreadId`, `thread/status/changed`, `thread/closed`, `collabAgentToolCall` items) still running when the parent's `turn/completed` arrives holds the run. The thread's own status outranks any item state. The hold ends when the last agent ends (plus a short wait for the turn its result starts), on cancel, on a person's message (requeued as the next turn), when the engine exits, after 5 minutes without a notification, or after 30 minutes in all. Agents still running at the end are reported as `background.stopped` with a reason. The engine's own review, compact and memory agents are not held. A background shell has no reliable signal and is neither held nor reported. | The run ends only on a `turn/completed` whose turn id is the one started for the prompt (from the `turn/start` reply, or the first `turn/started` on this thread). A different id is ignored; a frame with no id ends the run; a run that never learned its id ends on the first completed turn. Other threads' turns are ignored. |
| ACP, OpenCode (`opencode acp`) | No signal: the protocol has no event for work the agent keeps doing after a turn. The run ends with the prompt's reply and the process is stopped. The driver sees only tool calls still open at the stop, and closes them without a notice. | The run ends on the `session/prompt` reply matched by request id, from a process started for this run (a resume replays history first and the driver discards it). A `session/update` that names another session is dropped. A reply without a `stopReason`, or with `refusal`, `max_tokens`, `max_turn_requests` or an unknown one, fails with a plain message; only `end_turn` completes. |
| AG-UI | No signal: the protocol subset has no background-work event. The run ends at `RUN_FINISHED`; open tool calls are closed without a notice. | Every POST (the first and each resume after an interrupt) is a new run with its own `runId`. A `RUN_FINISHED` or `RUN_ERROR` that names another run or thread is ignored; a frame with no ids still ends the run. |

Not yet shown in the app: the `background.stopped` events from Claude and Codex have no display, so the person
sees the run end without a note. Nothing in the Codex, ACP or AG-UI rows has been run against a real engine;
the Codex signals come from the schema of Codex 0.166.0 and the rest from the fake engines.

## Observed on Claude Code 2.1.327

- A resumed session runs a turn for a task notification first, and queues the person's prompt behind it.
  `--replay-user-messages` emits the prompt (`type: user`, `isReplay: true`, same `uuid`) when it is dequeued.
- Backgrounded subagents report `system/task_started` (`task_type: local_agent`, `is_backgrounded: true`) and
  end with `system/task_updated` (`patch.status`) and `system/task_notification` (`status`).
- Exiting the CLI (closing stdin) kills its background agents.

## Tests

Each driver reproduces both steps with its fake engine in `crates/orbynd/src/bin/orbyn_fake_engine/`:
a background task still running when the turn's result arrives, and a leftover turn ahead of the prompt.
The Claude cases are `[agent-finish]` and `[leftover]` in `crates/orbynd/tests/workflows.rs`, and the
mapper unit tests in `driver/claude/tests.rs`. The Codex, ACP and AG-UI cases are in
`crates/orbynd/tests/drivers.rs` and the driver unit tests under `crates/orbyn-core/src/agents/driver/`.
