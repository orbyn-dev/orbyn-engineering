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

The Codex protocol schema is not vendored. The turn-id rules above come from observed traffic; the background-work signals below come from the schema `codex app-server generate-json-schema` writes (Codex 0.166.0). None of it has been run against a real Codex, only against the fake engine.

### Background work

A completed turn does not end the run while a sub-agent thread the run started is still running: the agents share the app-server's stdio and die with it. The run holds the completed outcome, keeps reading, and ends when the last agent has ended.

Signals (schema from `codex app-server generate-json-schema`, Codex 0.166.0):

- `thread/started`: `thread.parentThreadId`, or `thread.source.subAgent.thread_spawn.parent_thread_id`, naming this run's thread (or a known agent) makes the thread an agent; `thread.status.type == "active"` means running. A thread whose `source.subAgent` is a string (`review`, `compact`, `memory_consolidation`) or `{ other }` is the engine's own agent and is never held.
- `thread/status/changed`: `threadId`, `status.type`. `active` is running; `idle`, `notLoaded` and `systemError` are not. (`activeFlags` only says it waits on approval or input.)
- `thread/closed`: `threadId`.
- `item/started` and `item/completed` of type `collabAgentToolCall` on a known thread: `receiverThreadIds` and `agentsStates.<id>.status` (`pendingInit`, `running` live; `completed`, `errored`, `interrupted`, `shutdown`, `notFound` not). With no state, `spawnAgent`, `sendInput`, `resumeAgent`, `sendMessage` and `followupTask` leave the agent running, `closeAgent` and `interruptAgent` stop it, a failed or interrupted call changes nothing.
- an item of type `subAgentActivity`: `agentThreadId`, `kind`: `started` live; `completed`, `interrupted` not; `interacted` changes nothing, because the schema does not call it live.

The thread's own status is the authority. An item may mark an agent running, but never one whose `thread/status/changed` or `thread/closed` has ended it; only a later `active` status revives it. A stale or out-of-order item therefore cannot hold the run.

How the held run ends:

- All agents ended: wait for the turn the agent's result starts on this thread (`turn/started` with another id, then its `turn/completed`) and end then. If none starts within 2 seconds, end with the held outcome. The 2 seconds is a guess: the schema does not say a result always starts a turn.
- A message from the person: the held turn is over, so there is none to steer. The message ends the hold (`new message`) and is requeued as the next turn through the core's failed-steer path.
- Cancel: end `cancelled` at once. The parent's turn is over, so there is nothing to interrupt.
- The engine exits, with any status: end with the held outcome (`engine exited`). The answer was given; only the agents went with the process.
- Nothing heard for 5 minutes from this run's thread or any agent it tracks (`idle`): end with the held outcome. This also bounds a follow-up turn whose `turn/completed` never comes.
- 30 minutes in all (`limit`): end with the held outcome.
- A failed or interrupted parent turn ends at once, as before (`turn ended`).

`ORBYN_CODEX_BACKGROUND_LIMIT_MS` and `ORBYN_CODEX_BACKGROUND_IDLE_MS` set the two caps, for tests.

Whenever the run ends with an agent still running, the driver emits one `background.stopped` custom event per agent: `taskId` (the thread id), `taskType: "codex_subagent"`, `description` (the agent's nickname, when known) and `reason` (`cancelled`, `new message`, `limit`, `idle`, `engine exited` or `turn ended`). A force stop aborts the task and cannot say it. The app does not show `background.stopped` yet: it is in the run's event log only, and showing it is a follow-up for the core and UI owners.

Not held: background shells. A `commandExecution` item that has not completed looks the same for a dev server and for a command an interrupt left behind, and `process/exited` is only sent for processes this client started with `process/spawn`. The schema has no reliable "background terminal still running" signal, so shells are neither held nor reported.
