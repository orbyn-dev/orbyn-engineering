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
| Claude | `task_started` with type `local_agent`, and `local_workflow`, hold the first `result` until the matching `task_notification` or `task_updated` arrives; the follow-up turn then ends the run. A shell (`local_bash`) cannot be held, because a dev server may never end; it is reported as `background.stopped` when the run ends. | The prompt goes in with a `uuid` and the CLI runs with `--replay-user-messages`. It echoes the prompt as it takes it up; a `result` before the echo, after a turn had begun, belongs to the leftover turn and is ignored. A failure with no turn begun (lost session, no login) still ends the run. |
| Codex, ACP, OpenCode, AG-UI | Engineering 3 audits these against the contract. | Same. |

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
mapper unit tests in `driver/claude/tests.rs`.
