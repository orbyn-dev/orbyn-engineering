# Agents

Everything about the coding engines Orbyn runs: which exist, how to talk to each, and what they keep on their own.

| Module | Owns |
| --- | --- |
| `engines` | Engine registry, status and updates through the installer that owns each binary |
| `driver` | One driver per engine protocol, turning it into `EngineEvent`s: `claude/`, `codex/` and `acp/` are folders (`mod.rs` is the driver, the other files its pieces, `tests.rs` its tests) beside the shared `pipes`, `rpc`, `terminal` and `diagnostics` |
| `acp_registry` | The public ACP agent registry and installing an agent as a custom engine |
| `acp_mcp_bridge` | Stdio access to a run's MCP endpoint for ACP agents |
| `native_sessions` | An engine's own saved conversations: list, import, delete |
| `claude_history` | One Claude conversation read from Claude's history folder |
| `history_import` | Claude and Codex conversations imported as threads |
| `console` | The engine's own console resumed in an Orbyn terminal |
| `permission_modes` | Which permission modes each provider offers |
| `provider_facts` | One provider's state facts (installed, signed in, broken, managed, approvals), read by the crew router's filters and the Providers list alike |
| `provider_list` | The grouped Providers list: labels, found apps and engines not built |
| `selection` | Model, effort and options a turn asks of its engine |
| `usage` | Usage from each engine's local session history |
| `triage` | A fresh native support session outside governed runs |
| `sse` | An incremental `text/event-stream` parser |
| `titles` | Thread titles from a short engine call |

The crate's [group table](../../README.md#groups) lists every group.

## Codex run end

A Codex run ends only on `turn/completed` for the turn started for the person's prompt. A previous or engine-started turn on the same thread does not end it.

- The turn id comes from the `turn/start` reply, or from the first `turn/started` when the reply had none. A `turn/started` counts only when it is on this thread (or names none), so a sub-agent's turn is never learned.
- A `turn/completed` with the same id (`turn.id`, or `turnId`) ends the run.
- A `turn/completed` with a different id is ignored.
- A `turn/completed` with no id ends the run, so an engine that omits it never hangs the run.

The Codex protocol schema is not vendored, so these frame shapes are inferred from observed traffic.

### Background work

A completed turn does not end the run while a sub-agent thread the run started is still running: the agents share the app-server's stdio and die with it. The run holds the completed outcome, keeps reading, and ends when the last agent has ended.

Signals (schema from `codex app-server generate-json-schema`, Codex 0.166.0):

- `thread/started`: `thread.parentThreadId`, or `thread.source.subAgent.thread_spawn.parent_thread_id`, naming this run's thread (or a known agent) makes the thread an agent; `thread.status.type == "active"` means running.
- `thread/status/changed`: `threadId`, `status.type`. `active` is running; `idle`, `notLoaded` and `systemError` are not. (`activeFlags` only says it waits on approval or input.)
- `thread/closed`: `threadId`.
- `item/started` and `item/completed` of type `collabAgentToolCall` on a known thread: `receiverThreadIds` and `agentsStates.<id>.status` (`pendingInit`, `running` live; `completed`, `errored`, `interrupted`, `shutdown`, `notFound` not). With no state, `spawnAgent`, `sendInput`, `resumeAgent`, `sendMessage` and `followupTask` leave the agent running, `closeAgent` and `interruptAgent` stop it, a failed or interrupted call changes nothing.
- an item of type `subAgentActivity`: `agentThreadId`, `kind` (`started`, `interacted` live; `completed`, `interrupted` not).

How the held run ends:

- All agents ended: wait for the turn the agent's result starts on this thread (`turn/started` with another id, then its `turn/completed`) and end then. If none starts within 2 seconds, end with the held outcome. The 2 seconds is a guess: the schema does not say a result always starts a turn.
- Cancel: end `cancelled` at once. The parent's turn is over, so there is nothing to interrupt.
- The engine exits: end with the held outcome when it exited cleanly, otherwise failed with the exit message.
- An hour (`ORBYN_CODEX_BACKGROUND_LIMIT_MS` sets it, for tests): end with the held outcome.
- A failed or interrupted parent turn ends at once, as before.

Whenever the run ends with an agent still running (cancel, engine exit, the limit, a failed or interrupted turn), the driver emits one `background.stopped` custom event per agent (`taskId`, `taskType: "codex_subagent"`, `description`, `reason`: `cancelled`, `limit`, `engine exited` or `turn ended`), the same event the Claude driver uses. A force stop aborts the task and cannot say it.

Not held: background shells. A `commandExecution` item that has not completed looks the same for a dev server and for a command an interrupt left behind, and `process/exited` is only sent for processes this client started with `process/spawn`. The schema has no reliable "background terminal still running" signal, so shells are neither held nor reported.
