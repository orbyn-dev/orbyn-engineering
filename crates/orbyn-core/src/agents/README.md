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
