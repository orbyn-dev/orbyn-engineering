# Engine host protocol

The wire contract between the Rust core and `orbyn-engine-host`, the signed Node SEA binary that runs
engine SDKs and libraries (Claude Agent SDK, Cursor SDK, OpenCode, Muse and Codex; later ACP and Pi).
The Rust `HostDriver{family}` implements `EngineDriver` against this document; the host implements
the other side. Neither side reads the other's code.

Status: slices 1 and 2 of the Rust side are built (spawn and handshake, runs, steer, cancel, restarts, status; then permissions, questions, stop, restart and the spawn secret); the Claude family is behind a flag and the Codex family is registered without one. The shape follows the
worker protocol of T3 Code's Cursor provider (MIT, `CursorAgentSdkWorkerProtocol.ts`), with three
changes: a version handshake, `permission`/`question` frames so policy stays in Rust, and
per-run confinement inputs supplied by Rust.

## Ground rules

1. **Policy stays in Rust.** The host never decides whether a tool may run. Every engine permission
   request (Claude `canUseTool`, ACP `request_permission`, Codex approval) becomes a `permission`
   frame that the core answers through `ApprovalGate` (write scope, plan mode, Autopilot review,
   rules, Person routes). A host that cannot ask refuses the tool.
2. **The host holds no credentials of its own.** It receives only the environment the core
   allowlists for a run. It never reads the keychain, and it never forwards a token to a frame or log.
3. **Run end is the run-end contract.** `runDone` is the only frame that lets the core emit
   `Finished`. The rules in [engine-run-contract.md](engine-run-contract.md) (background work,
   the prompt's own turn, no-reply) are applied by the host adapter for its family and by the core
   on top; the transport adds no new way to end a run.
4. **One process per family** (per instance where the SDK reads process-global environment, such as
   `CLAUDE_CONFIG_DIR`). Cursor is pooled in one host. Idle hosts are stopped by the core, which owns the idle timeout and never stops a host that still has an open `background` event; the host keeps no timer of its own.

## Transport

Newline-delimited JSON over stdio. One frame per line, UTF-8, no embedded newlines (JSON escapes them).
`stdin` carries requests, `stdout` carries host frames, `stderr` is free text for diagnostics and is
never parsed. The host writes nothing else to `stdout`: SDK output is redirected to `stderr` before
any engine code loads. `BigInt` is encoded as a decimal string and `Uint8Array` as base64.

A frame over 8 MiB is a protocol error. Large payloads (file contents, diffs) travel by path, not inline.

## Handshake and version

The core starts the host as `orbyn-engine-host <family> [flags]` and the host speaks first:

```json
{"type":"hello","protocol":1,"host":"0.1.0","family":"claude","sdk":{"name":"@anthropic-ai/claude-agent-sdk","version":"0.3.321"},"capabilities":["steer","fork","background","rollback","models","usage"]}
```

The core answers with the one request it may send before `ready`:

```json
{"id":1,"type":"init","protocol":1,"instance":"claude-default","logLevel":"info"}
```

The host replies `{"type":"response","id":1,"ok":true,"result":{"ready":true}}`.

- `protocol` is an integer; a different number is a hard mismatch. The core refuses the host,
  fails nothing that is running (nothing is), and reports "The engine host does not match this app
  version" with the two numbers. No downgrade negotiation: host and app ship together.
- `capabilities` is the list of requests the family implements. The core never sends a request
  that is not listed; a listed request that fails is an ordinary error. Adding a capability is not a
  protocol bump; adding or changing a frame field is, unless the field is optional and ignorable.
- A host that does not say `hello` within 10 seconds is killed and counted as a crash. The first spawn after an install or update gets 30 seconds (a signed binary's first launch is scanned by the OS); restarts keep 10.

## Requests (core → host)

Every request has an `id` (positive integer, unique per host process) and a `type`. Each gets exactly
one `response` frame with the same `id`, except `send`, whose response comes when the turn is
*accepted* and whose progress then arrives as frames carrying `sendId` (the request's `id`).

| `type` | Fields | Result |
|---|---|---|
| `open` | `operation` (`create` \| `resume` \| `fork`), `sessionId`, `engineSessionId?`, `cwd`, `options` (model, effort, mode, MCP, system prompt; family-shaped, see below), `confinement` | `{engineSessionId}` |
| `send` | `sessionId`, `message` (text, or text plus attachments by path), `messageId?` (a UUID the core chose for this prompt; the host sends it to the engine as the message's `uuid` so the engine's own frames name it, and ignores it if the engine cannot), `options?` | `{runId, engineSessionId?, turnId?, messageId?}` (accepted). Some SDKs assign the session id only on the first message; the host also sends a `session` frame (below) when the id or turn id becomes known later |
| `steer` | `sendId`, `messageId`, `text` | `{outcome, messageId}` (`accepted` \| `queued` \| `rejected`); a refusal is recorded against that message |
| `cancel` | `sendId` | `{cancelled: bool}` |
| `respond` | `requestId`, `decision`: for a `permission`, `{"behavior":"allow"\|"deny","message"?}` and nothing else; for a `question`, `{"answers":{"<question text>":"<answer>"}}` (several selections joined by a comma) or `{"behavior":"deny"}` when the person skips it | `null` |
| `stopTask` | `sessionId`, `taskId` (from a `background` `started` event, which also carries the originating `toolCallId`) | `{stopped: bool}` |
| `background` | `sendId` | `{backgrounded: bool}` |
| `rollback` | `sessionId`, `toTurn` (a `turnId` the host reported) | `{ok: bool}` |
| `fork` | `sessionId`, `atTurn?` | `{engineSessionId}` |
| `readHistory` | `sessionId` | list of messages (family-shaped, mapped by the core) |
| `usage` | `sessionId` (optional for the Cursor, Muse and Codex families, which also answer account-wide) | context and cost counters. Cursor, Muse and Codex: `{usage, limits}`, see below |
| `models` | none (host-wide; Muse takes optional `cwd`, `musePath`, `confinement` for its throwaway process; Codex takes optional `cwd`, `codexPath`, `env`, `confinement`) | model list with efforts and capabilities. Cursor, Muse and Codex: `{models, account?}`, see below |
| `closeSession` | `sessionId` | `null` |
| `probe` | `sessionId`, `kind` (`confinement`) | `{refused: [...], allowed: [...]}`: the host runs a scripted attempt to write outside the roots and read a forbidden path through the engine and reports which were refused; the Crew probe per family and the Codex confinement probe use it |
| `ping` | none | `null` (liveness; see crash and recovery) |
| `close` | none | `null`, then the host exits |

`respond` answers a `permission` or `question` frame by its `requestId`; it has no `sendId` because a
request can outlive the turn that raised it only briefly (see crash semantics). `options` and
history payloads stay family-shaped on purpose: the core maps them in `HostDriver{family}`, and a
shared superset would leak one SDK's vocabulary into every other.

Unknown request types get `ok:false` with `error.name = "UnsupportedRequest"`.

## Host frames (host → core)

```json
{"type":"response","id":7,"ok":true,"result":{}}
{"type":"response","id":7,"ok":false,"error":{"method":"send","cause":{...}}}
{"type":"delta","sendId":7,"update":{...}}
{"type":"permission","sendId":7,"requestId":"p-31","tool":"Edit","input":{...},"options":[...]}
{"type":"question","sendId":7,"requestId":"q-4","tool":"AskUserQuestion","input":{"questions":[{"question":"...","header":"...","options":[{"label":"...","description":"..."}],"multiSelect":false}]}}
{"type":"session","sendId":7,"engineSessionId":"...","turnId":"..."}
{"type":"runDone","sendId":7,"ok":true,"result":{"reason":"completed","usage":{...}}}
{"type":"runDone","sendId":7,"ok":false,"error":{"method":"send","cause":{...}}}
{"type":"log","level":"warn","sessionId":"...","event":{...}}
{"type":"background","sessionId":"...","event":{"type":"started"|"ended","id":"...","kind":"shell|agent|workflow", ...}}
```

- `delta.update` is the engine's own event, passed through unmodified (the SDK message). The core
  maps it to timeline events in `HostDriver{family}`, exactly as the CLI drivers map their stream JSON
  today. The host does not translate, which keeps replay transcripts byte-comparable with T3's.
- A `question` frame is the engine asking the person something (Claude's `AskUserQuestion`); `input` is the engine's own, untouched. It blocks like a `permission` (no host timeout, refused on cancel or run end) and is answered by `respond` with `answers`. A `question` is never auto-answered by policy.
- A `permission` frame blocks the tool until `respond`. If the run is cancelled or the host's run
  ends first, the host resolves the pending SDK callback with a refusal and forgets the id; a late
  `respond` for an unknown id is ignored and counted in `log`, and the core drops its open approval for
  that `requestId`.
  In the real Claude SDK, cancelling a run while a `permission` or `question` is open ends it as `runDone`
  `ok:false` with kind `engine`; the core that sent the cancel does not record that as a failure.
- The core answers a `permission` through `ApprovalGate` and a `question` through `QuestionDesk`; the person, a rule, plan mode, the write scope and Autopilot decide, never the host. Orbyn's own MCP tools (`mcp__orbyn_engineering__*`) govern themselves and are allowed without asking, as on the CLI path. A `question` with nothing to show is answered `deny` and never asked. Only one answer is ever sent for a request id: an answer that comes after the run ended, was cancelled or lost its host is dropped, and so is every ask still open when the host crashes.
- `permission.tool` and `permission.input` are the engine's own, passed through untouched: the host
  adds no field to them and drops none (`requestId` and `sendId` live on the frame, outside `input`),
  because the core binds an approval to every request field but a short bookkeeping list. ACP option
  kinds are sent as received in `options`; the host never invents one. `respond` `allow` always reaches
  the engine in its once-only form (Claude: allow with no `updatedPermissions`; ACP: `allow_once`); if
  the engine offered only an always-allow option the host answers `deny`. The core never grants a
  session-wide allow. There is no host-side timeout on a pending `permission` (a person may take
  minutes), and `ping` keeps answering while one is pending. A `permission` pending at a host crash is
  never retried after the restart; the core sends the next turn only on the person's next message.
- `runDone` is sent exactly once per `send` (success, engine failure, cancel, or adapter-detected
  loss). After it the host sends no more frames for that `sendId`. `result.reason` is one of
  `completed`, `cancelled`, `failed`, `maxTurns`, `refused`. The core maps them: `completed` and
  `cancelled` as named; `failed`, `maxTurns` and `refused` become `Failed(<plain sentence>)` with a
  `detail` code; a `kind: session_lost` error becomes `SessionLost`. The host adds
  `threadDisposition: "reusable" | "broken"`, and on a failure may add
  `retry: {"afterMs"?, "why": "rate_limit"|"network"|"overloaded", "resetsAt"?}`; `resetsAt` (unix seconds,
  inside `retry`, only for `rate_limit`) lets the core emit its `Limited { resets_at }` event.
- `runDone` `result.usage` may also be present on a `cancelled` or `failed` result (the tokens spent before the end); the core treats it as optional on every `reason`.
- `background` frames report shells, subagents and workflows the engine started, so rule 1 of the
  run-end contract can hold or report them. The host keeps the run open per the family's rule
  (see [engine-run-contract.md](engine-run-contract.md)) and sends `runDone` only when the hold ends.
- `log` is for protocol-level events the core stores as run diagnostics (T3's `protocolLog`). Never
  put prompt text or tokens in `log`; the host redacts before sending.

## Account, limits and catalog results (Cursor, Muse and Codex)

Read-only, family-shaped results of `models` and `usage`, all optional and ignorable (no protocol bump; a core that ignores them loses only the display):

- `models` result `{models: [...], account?}`. Cursor: `models[]` is the public catalog item (`id`, `displayName`, `description?`, `aliases?`, `parameters?`, `variants?`);
  `account?` is `{apiKeyName?, createdAt?, userId?, email?, firstName?, lastName?}` and is absent when the team blocks it. Muse: `models[]` is
  `{id, name, isDefault, efforts: [{id, label, isDefault?, description?}], defaultEffort?}`. Cursor needs `CURSOR_API_KEY` at host start, else `kind: "auth"`.
- `usage` result `{usage, limits}`: `usage` is the named session's last finished run total (or `null`); `limits` is `{checkedAt, windows: [{id, kind, label, usedPercent (0 to 100), resetsAt?, windowDurationMins?}], authLabel?, unavailable?, message?}` or, for Muse before any report, `null`. `unavailable` is
  `"unsupported"` or `"probeFailed"` with empty `windows`; a limits failure is never a request error.
- No credential, token, key or response body appears in a result or `log`; the host redacts and failures carry fixed text.

## Codex family

`orbyn-engine-host codex` speaks the newline-delimited JSON-RPC of the person's own `codex app-server`
(`options.codexPath`, default `codex`; never bundled). It starts one app-server per distinct confinement
(`argvPrefix`, `env`, `cwd`, path and env additions); sessions with the same confinement share it, and it is
stopped when its last session closes. `open.confinement` is applied when that process starts; a family that
could not apply it would fail `open` with `kind: "unsupported"`. Windows launchers resolve `codex` with PATH
and PATHEXT and run a `.cmd`/`.bat` under `cmd.exe`.

`open` options (family-shaped): `codexPath?`, `model?`, `effort?`, `mode` (`"read-only"` default, or
`"workspace-write"`), `approvalPolicy?` (`"on-request"` default, or the stricter `"untrusted"`),
`mcpServers?` (a record, sent as `mcp_servers.<name>` thread config), `developerInstructions?`, and `env?`
(environment additions for the child; keys are logged, never values). `open` `operation: "fork"` also takes
`options.atTurn`, sent as `thread/fork` `lastTurnId`. `effort` is applied on each `turn/start`, where Codex
accepts it; the thread request carries `cwd`, `model`, `developerInstructions` and MCP config.

Requests: `open` (`create` is `thread/start`; `resume` is `thread/resume`; `fork` is `thread/fork`; every
answer is the Codex thread id), `send` (`turn/start` with text and image/audio attachments by path, answers
`{runId, engineSessionId, turnId?, messageId?}`), `steer` (`turn/steer`; `accepted` or `rejected`, never
`queued`), `cancel` (`turn/interrupt`, then `runDone` cancelled), `respond`, `rollback` (`thread/read`,
`thread/resume` when unloaded, `thread/turns/list` descending, `thread/revert` before the turn after `toTurn`
(so `toTurn` is kept and later turns are removed), and a reload),
`fork` (`thread/fork` at `atTurn`), `readHistory` (`thread/read` with turns, returned as `{turns}`),
`closeSession`, `models` and `usage`. `stopTask`, `background` and `probe` are not in its capabilities.

Frames: `delta.update` is Codex's own notification as `{method, params}`, untouched; a `session` frame is
sent when a thread or turn id becomes known later; `background` events report a subagent thread as
`{type: "started"|"ended", id: "<thread id>", kind: "agent"}` with `toolCallId`/`description` when Codex
names them. `runDone` is sent once per `send`, with `result.reason`, `threadDisposition`, the turn's token
usage and, on a rate limit, network loss or overload, `retry`. A run ends only on a `turn/completed` for its
own turn id (the first one if Codex never returned one); another thread's turn is a delta and never ends it.
A subagent thread still running when the parent's turn completes holds the run as
[engine-run-contract.md](engine-run-contract.md) describes; the hold ends when the last agent ends plus a short
grace for its follow-up turn, on cancel, after 5 minutes without a notification, after 30 minutes in all, or
when the app-server exits (agents still running are then `background` `ended` with status `stopped`). A
background shell is neither held nor reported. Codex's own review, compact and memory agents are not
distinguished from subagents.

Approvals and questions: `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`,
`item/permissions/requestApproval`, `applyPatchApproval` and `execCommandApproval` are `permission` frames
(`tool` is `command`, `fileChange`, `permissions`, `applyPatch` or `execCommand`; `input` is Codex's params
unchanged; `options` is `availableDecisions` when Codex sends it, else the wire's fixed decision set).
`item/tool/requestUserInput` is a `question` frame (`tool: "request_user_input"`); its answers are keyed by
the question text and sent back by question id. A form-mode MCP elicitation is a `question` frame
(`tool` is the server name); a URL-mode one is a `permission` frame and an allow answers `cancel`, because
this host cannot open the page. `respond` `allow` reaches Codex once only: `accept` (never
`acceptForSession`), the requested permissions with `scope: "turn"`, `approved`, or an elicitation `accept`.
`deny` is `decline`, empty permissions, a legacy denied decision, or an elicitation `decline`. The host
never sends `approvalPolicy: "never"` or a `danger-full-access` sandbox; a recorded one is replaced by the
asking values. `respond` has no timeout; cancel, run end and a crash refuse pending asks, and a late
`respond` is logged and ignored.

`models` (host-wide, throwaway app-server; optional `cwd`, `codexPath`, `env`, `confinement`) is
`{models, account?}`; a model is
`{id, name, isDefault, efforts: [{id, label, isDefault?}], defaultEffort?}` from `model/list`, and `account`
is `{type?, planType?, email?}` from `account/read` when signed in. `usage` without a session also uses a
throwaway app-server (same optional fields); `usage` is `{usage, limits}`:
`usage` is the named session's last finished run total, or `null`; `limits` is `{checkedAt, windows,
authLabel?}` from `account/rateLimits/read`, with `primary`/`secondary`/`individual` windows. A limits
failure is `{checkedAt, windows: [], unavailable: "probeFailed", message}`, never a request error. T3's
`resetCredits` is not carried because this document's limit shape has no field for it.

Not run against a real Codex turn: every behaviour above comes from replayed T3 Code transcripts
(app-server 0.156.1) and synthetic frames; see `engine-host/README.md`.

## Errors

One shape everywhere an error crosses the wire:

```json
{"name":"Error","message":"...","code":"ECONNRESET","status":401,"kind":"auth","stack":"...","cause":{...}}
```

- `cause` nests up to 8 deep; a cycle ends with `{"name":"Error","message":"[Circular error cause]"}`.
- Every property read is guarded: a hostile or broken getter never aborts serialization.
- `kind` is a closed set the core can act on without parsing messages: `auth`, `rate_limit`,
  `network`, `invalid_request`, `unsupported`, `cancelled`, `engine`, `host`, `session_lost`. `session_lost` is returned by `open` with `resume`, and by `runDone` when the engine says the session is gone. The host sets it from the
  SDK's own error classes; absent means unknown. `auth` and `rate_limit` carry what the UI needs
  ("Sign in again", "Limit reached") and nothing that identifies a credential.
- A request-level failure is `response` with `ok:false` and `error = {method, cause}`. A run-level
  failure is `runDone` with `ok:false` and the same shape. Messages are shown to the person only after
  the core's redaction pass; the `stack` is stored in diagnostics only.

## Host status (core to UI)

Not a host frame: this is what the core's `GET /api/engine-host` route returns (Local read), one entry per family. The contract is
`crates/orbyn-contracts/src/engine_host.rs` with the generated `EngineHost*.ts` in `web/src/generated` (HostDriver slices 1 and 2: #941, merged, and this change). The host sends none of these fields; the core builds them from the host's `hello`, its own restart
bookkeeping and its own checks. No entry carries an environment, arguments, a key, a path, a token or a stack.

**Slice 1 (the contract above).** `EngineHostStatus`:

- `family`, `state`, `host_version?`, `restart_count` (restarts after a crash since the app started; a person's restart does not reset it), `last_crash?`, `protocol_app?` and `protocol_host?`
  (for `version_mismatch`), `not_used?`, `detail?` (a plain sentence for a state that stops runs), `action?` (what the person can do, such as `Update Orbyn`).
- `state` is `not_started`, `starting`, `running`, `stopped` (idle, or the person stopped it), `crashed` (crashed and the person asked again), `restarting`, `cant_start`,
  `version_mismatch` or `not_used` (the family runs on its Rust driver).
- `not_used` is `flag_off` or `unsupported_platform`.
- `last_crash` is `{exit_code?, signal?, at (RFC 3339), error_kind, message, stderr_tail}`. `error_kind` is `exited`, `closed_output`, `stalled`, `malformed`, `oversize`,
  `hello_timeout` or `start_failed`. `stderr_tail` is at most 1024 characters with credential-looking lines dropped.

**Slice 2 (built).** Optional and ignorable, no protocol bump:

- `last_error_kind?`: what the family's latest finished run failed with, from the closed set in Errors (`auth`, `rate_limit`, `network`, `invalid_request`,
  `unsupported`, `cancelled`, `engine`, `host`, `session_lost`), so the UI can choose wording without parsing a message. It is separate from the crash `error_kind`,
  which says how the host process failed. Absent after a run that completed, and when the host named no kind.
- `threads?`: the threads with a run open on the host now, `{thread_id, family, state}` with `state` `running`. `starting`, `restarting` and `unavailable` are in the
  contract for the Planned part below; slice 2 reports only threads that are running.
- **Stop and restart** (Person routes, desktop key only, audited): `POST /api/engine-host/{family}/stop` and `POST /api/engine-host/{family}/restart`, body
  `{stop_background?: boolean}`, answering the family's `EngineHostStatus`. Both end the runs open on that host as cancelled. Rule: a host with a `background` event still
  open (a shell, a subagent or a workflow the engine started) is not stopped or restarted, and the route answers `409 background_open` with the count, unless the body says
  `stop_background: true`, which is the person's choice to end that work too. A stop leaves the family `stopped`, and the next run starts it. A restart stops the host
  (same rule), forgets every crash and backoff (so a `cant_start` family is tried again) and starts it at once; `restart_count` keeps its tally. `409 version_mismatch`
  for a mismatch (only an update fixes it), `409 not_used` for a family on its Rust driver, `404` for an unknown family. Each call writes an `engine_host.stop` or
  `engine_host.restart` audit row (actor, family, `stop_background`) before it acts. Answers carry fixed words and the status with every host-derived string passed through the
  redactor, so a key the host printed never leaves.
  The web client (`web/src/features/enginehost/hostControl.ts`) calls both: Stop and Restart in Maintenance › Processes, Try again (the restart) for a `cant_start` family and the
  composer's Restart engine host. A `409 background_open` opens a question with the service's sentence, and only its confirm sends again with `stop_background: true`. Stubbed
  fetch tests only: not run against a running service.
- **Spawn secret.** A host's spawn environment gets an engine's key only from `Core::engine_spawn_secret(engine)`, which honours the person's per-engine opt-in. The key goes
  in the child's environment and nowhere else: not argv, not a frame, not a log line or an error. No key and no core bound yet mean no key.

**Planned (not built).** Names are proposals:

- `threads[].state` of `starting`, `restarting` and `unavailable`, and `threads[].last_error` (`{kind?, message}`, only for `unavailable`), so the UI can show a thread
  whose host is coming up or gone. A pooled host (Cursor) lists every thread it serves.
- `signin?`: `"signed_in" | "signed_out" | "unknown" | "key_in_keychain"` per engine. A state word only: `key_in_keychain` means an API key is saved in the OS
  keychain, and the key is never in the entry. Reading it needs the keychain, which can prompt, so a plain status read does not do it yet.
- `running_without_host?`: `{reason: "flag_off" | "not_installed" | "unavailable"}`, present when the family runs on its CLI driver. Slice 1's `not_used` (`flag_off`,
  `unsupported_platform`) covers `flag_off`; the host driver does not fall back to the CLI when a host is `cant_start` (the run is refused), so `unavailable` would be
  untrue today, and a build with no host has no entry to attach `not_installed` to.

The web client reads this route with the generated types (`web/src/features/enginehost/hostStatus.ts`; a 404 means nothing is reported) and turns each entry into the `HostRow` the Settings screens draw (`contract.ts`). The `HostRow` fields for the Planned items above, and for stop, restart and retry, are filled only by the dev mock: the real client leaves them empty, and the screens draw nothing for them (no Stop, Restart or Try again button until the Person routes of slice 2 exist).

## Crash and recovery

- **Host exits or closes stdout.** The core fails every open run on that host with
  "The engine host stopped. Your work in the project is kept; send your message again." (outcome
  `engine_error`, `threadDisposition` unknown), answers pending `permission` frames as refused (they
  are already unanswerable), and restarts the host with exponential backoff (1 s, doubling, cap 30 s,
  reset after 60 s of uptime). After five restarts inside five minutes the family is marked
  unavailable with the last error and no further restarts until the person asks.
- **Host stalls.** A `ping` request (no fields) must answer within 5 s; three misses in a row count
  as a crash. The core pings only while a run is open.
- **Malformed frame.** Count it, log it, drop it. Ten in a minute counts as a crash. The core never
  guesses what a frame meant.
- **Core exits.** The host sees `stdin` close, cancels every open run, calls `close` on the SDKs,
  and exits within 5 seconds, then force-exits. Engine children are started in the host's process
  group so they die with it. (The run-end contract's observed behaviour, that closing stdin kills
  background agents, holds here too.)
- **Resume.** The core stores `engineSessionId` from `open`; after a restart it sends `open` with
  `operation: "resume"`. A resume the engine refuses ends the run with the existing "The engine lost
  this conversation." message.

## Confinement inputs

The core, not the host, decides what a run may touch. Each `open` (and each `send` that changes
them) carries:

```json
"confinement": {
  "argvPrefix": ["/usr/bin/sandbox-exec","-f","/path/profile.sb","--"],
  "env": {"PATH":"...","HOME":"...","ORBYN_WORKER_ID":"w-123"},
  "cwd": "/project/worktree",
  "writeRoots": ["/project/worktree"]
}
```

- `argvPrefix` is prepended to the engine process's command. For Claude the host passes it through
  the SDK's `spawnClaudeCodeProcess` hook; for a family with no spawn hook the whole host is started
  under the prefix (Cursor).
- `env` is the complete allowlist. The host builds the engine's environment from it and from nothing
  else. A key the host adds itself (`NODE_OPTIONS`, proxies) is reported in `log` before the engine starts, never after.
- `ORBYN_WORKER_ID` and the other worker identity variables (the git identity that names the Crew task, and whatever else the core sets for a CLI worker today; the list lives in the Rust spawn code, not here) are set by Rust; the host copies them and never
  invents or changes them.
- `writeRoots` are for the host's own pre-checks and `log`; the enforcement is the Seatbelt profile
  and `ApprovalGate`. Policy never depends on the host checking them.
- If `confinement` is present and the family cannot apply `argvPrefix` (no spawn hook, or a pooled host), `open` fails with `kind: "unsupported"` and starts nothing. A pooled Cursor host cannot carry a per-run prefix, so a Crew writing worker on Cursor is refused until the host is started under the prefix per run.
- A family passes a **confinement probe** (a run that tries to write outside the roots and to read a
  forbidden path) before Crew routes writing briefs to it. `CODEX_WORKERS_ALLOWED` stays false until
  Codex passes.

## Versioning and tests

- Protocol changes bump `protocol` and land with this document in the same PR.
- The host's **fake family** (`orbyn-engine-host fake`) implements every request and frame from
  scripted transcripts, including a hang, a crash mid-run, a malformed frame, a late `respond`, and a
  cause chain nine deep. It drives the `HostDriver` tests in `crates/orbynd/tests/`.
- Real-SDK behaviour is pinned by replay transcripts adapted from T3's provider-testing harness
  (MIT, keep its notice); a transcript runs without network or login.
- Nothing in this document has been run against a real SDK. Claude comes first (H2) and its
  transcripts will correct what is wrong here.

## Not in this document

- Which families ship, and when (the sprint plan owns that).
- Credential opt-in and the Anthropic and Cursor terms gates; the Claude SDK path stays behind a flag
  with the CLI driver as the default for subscription sign-in.
- The UI frames for host status and errors (the Designer owns them).
