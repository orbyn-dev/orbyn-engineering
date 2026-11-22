# orbyn-engine-host

The Node host that runs engine SDKs (Claude, Cursor, OpenCode, Muse, ACP and Codex; later Pi) for the
Rust core over newline-JSON on stdio. The wire contract is [docs/engine-host-protocol.md](../docs/engine-host-protocol.md);
change that document in the same PR as any frame change.

Status: skeleton. The `fake` family drives the `HostDriver` tests; `claude` and `cursor` (each behind a flag), `opencode`, `muse`, `acp` and `codex` are the real ones. Not part of the Rust
workspace or the web lint scope; it has its own `package.json`.

```bash
cd engine-host && npm test
node src/main.mjs fake
```

Layout: `src/frame.mjs` (encode, decode, error shape), `src/host.mjs` (handshake and dispatch),
`src/families/` (one file per family), `test/` (protocol tests against the fake family).

## Adding a family

A family is one file in `src/families/` that exports `{ name, sdk: {name, version}, capabilities, create(ctx) }`
(see `fake.mjs` for the smallest, `opencode.mjs` and `muse.mjs` for a served process, `claude.mjs` for an in-process SDK).

1. `create(ctx)` gets `ctx.send(frame)` (write any host frame) and `ctx.log(event)` (a `log` frame, never put prompt text or
   tokens in it). It returns `{ handle(request), close() }`; `handle` returns the response `result` or throws.
2. Implement `open`, `send`, `cancel`, `respond`, `closeSession` (`steer`, `fork`, `models`, `usage`, `readHistory`,
   `readSubagent`, `stopTask`, `background`, `rollback`, `probe` only if the engine can) and list the optional ones in
   `capabilities`. `send` answers `{runId}` when
   the turn is accepted, then streams `delta` frames (the engine's own events, untouched) and ends with exactly one `runDone`.
3. Policy stays in Rust: every engine permission or question becomes a `permission` or `question` frame, answered by `respond`.
   An allow reaches the engine in its once-only form; there is no host timeout on a pending one; cancel, run end and a crash
   refuse it. Never add an always-allow.
4. Errors use `serializeError` (`src/frame.mjs`) with a `kind` from the closed set in the protocol doc.
5. Apply `open.confinement` (argv prefix, env allowlist) to every process the family starts; a family that cannot must
   fail `open` with `kind: "unsupported"`. Log env keys you add before the engine starts.
6. Register the family in `src/main.mjs` (behind an env flag if a licence or terms gate applies; never bundle an SDK
   that is not MIT or approved: mark it `external` in `scripts/build-sea.mjs`).
7. Test by replaying recorded transcripts: copy them unmodified into `test/fixtures/<family>/` with their licence notice in
   `test/fixtures/NOTICE`, drive the family through `runHost` with in-process streams (see `claude.test.mjs`, `replay.mjs`),
   and write each test so it fails before the code. State in the PR what has not run against the real engine.

## Single executable (SEA)

```bash
npm i && npm run build:sea     # writes dist/orbyn-engine-host (about 110 MB, unsigned beyond ad hoc)
```

`scripts/build-sea.mjs` bundles with esbuild, builds a Node 22 SEA blob and injects it with postject.
Checked on macOS arm64 with Node 22.16: the binary answers the handshake and a fake turn, and runs
ad hoc signed with the hardened runtime and `entitlements.plist` (allow-jit, allow-unsigned-executable-memory,
disable-library-validation, the set T3 ships). Decision (owner): ad hoc signing only, for the host and the app,
as `release.yml` does without the `APPLE_*` secrets. Developer ID signing and notarization: Planned.
Not checked: launching the ad hoc signed host from inside the ad hoc signed app bundle (`externalBin`), Windows,
Linux, Intel macOS (T3 found SEA crashes there, so Intel keeps the Rust drivers).
The Tauri `externalBin` wiring and `scripts/check-desktop-assets.mjs` entry come with the first real family.

## Claude family (behind a flag)

`src/families/claude.mjs` runs `query()` from `@anthropic-ai/claude-agent-sdk` over the person's own
`claude` (`options.claudePath`). It starts only with `ORBYN_ENGINE_HOST_CLAUDE=1`, and the Rust side keeps
the CLI driver as the default for subscription sign-in until Anthropic approves the SDK path in writing.
The SDK is **not bundled** (esbuild `external`, no dependency in `package.json`): in development it loads from
`node_modules`, in the single executable it is absent and the family fails with a plain message.

Done and tested against replayed transcripts (T3 Code, MIT, `test/fixtures/NOTICE`): multi-turn session on
one query, permission frames with once-only allow and deny, cancel through `interrupt()`, session and turn
ids, background task frames, a run held open by a background agent or workflow, confinement through
`spawnClaudeCodeProcess` (argv prefix and env allowlist).

Not done, so Planned: the leftover-turn echo rule on a real run, `AskUserQuestion` against the real SDK, and any run
against the real SDK or a real `claude`.

`AskUserQuestion` is a `question` frame answered with `answers`.

Also done, from the SDK's own types (0.3.321), tested against a fake query: `models` (a throwaway query),
`usage` (`getContextUsage`), `stopTask`, `background` (`backgroundTasks`).

Also done, fake SDK only: `rollback` (closes the live query and resumes at the last top-level assistant message of
the target turn), `readHistory` (`getSessionMessages`) and `readSubagent` (`getSubagentMessages`), the last two capped
at the same 6 MiB as OpenCode's history. None of these three has run against the real SDK or a real `claude`.

Run-end rules: the prompt goes in with a `uuid` (the core's `messageId` from `send`, or one the host makes; the accepted answer returns it so the core can tell the prompt's own turn from a leftover one in `command_lifecycle` frames); a `result` that arrives after a `command_lifecycle` `queued` for it but
before `started` belongs to a leftover turn and is ignored (logged as `leftoverResult`); the first result while a
background agent or workflow runs is held, and the follow-up turn's result ends the run. Checked against a T3
transcript, and a real run (SDK 0.3.321, `claude` 2.1.327, haiku) showed `command_lifecycle` `queued` then `started`
for the prompt's uuid before the result. A leftover turn has not been seen on a real run.

## Cursor family (behind a flag)

`src/families/cursor.mjs` runs the public surface of `@cursor/sdk` (`Agent.create`/`Agent.resume`, `agent.send` with `onDelta`,
`run.wait`/`steer`/`cancel`, `agent.close`). It starts only with `ORBYN_ENGINE_HOST_CURSOR=1`; without it `cursor` is an unknown
family (exit 2). The SDK (licence "SEE LICENSE IN LICENSE.md", about 20 MB with native binaries) is **not bundled and not a
dependency** (not even optional): esbuild `external`, loaded with a dynamic import that fails with "The Cursor SDK is not
available in this build." (`kind: "unsupported"`). In the single executable it is therefore absent.

- **Pooled.** One host process serves every Cursor session (T3's single worker, keyed by `sessionId`). A pooled host cannot take a
  per-run argv prefix, so `open` with `confinement.argvPrefix` fails with `kind: "unsupported"` and starts nothing; the core
  starts the whole host under the prefix instead. `confinement.env` is not applied per session either (the host's own environment is
  the SDK's); its keys are logged as `confinementEnvNotApplied`, never the values.
- **Credentials.** The host reads no keychain or token store. The key, if any, is `CURSOR_API_KEY` in the environment the core passes
  at spawn, handed to `Agent.create`/`resume` as `apiKey`; it is redacted out of every error and is never in a frame or log. A key in
  a request (`options.apiKey`) is refused as `invalid_request`.
- **No approvals.** The Cursor SDK raises no interactive approval or question (T3's adapter declares none), so this family never sends
  a `permission` or `question` frame and `respond` only logs. Sessions default to the SDK's own sandbox on (`local.sandboxOptions.enabled`);
  the core's `options.local` wins. Policy for Cursor is therefore the sandbox and the core's confinement of the host, not `ApprovalGate`.
- Requests: `open` (create, resume; `fork` is `unsupported`), `send`, `steer` (`complete_delivered` is `accepted`, anything else
  `rejected` so the core sends a follow-up), `cancel`, `respond`, `closeSession`, `close`. `runDone` exactly once from
  `run.wait()`: `finished` is `completed`, `cancelled` is `cancelled`, `error` is `failed` with an error `kind` (`auth`, `rate_limit`
  and `network` with a `retry` hint, `invalid_request`, `session_lost` on a resume that is not found, `engine`). Deltas are the
  SDK's `interaction.update` payload, untouched.

Tested against replayed T3 Code transcripts (MIT, `test/fixtures/NOTICE`) of the SDK's public calls, not an internal worker
protocol: simple, multi-turn, close then resume, steering, cancel mid tool, a read-only tool call, a plan-mode turn, a queued second turn, a skill
invocation, subagents and a todo list. A run's usage is the SDK result's `usage`, else the sum of the `turn-ended` usages (T3's rule), also on a cancel or a failure. `test/replayCursor.mjs` plays a
transcript as a fake SDK (it asserts each recorded outbound call, then delivers the recorded updates and result). Errors, pooling,
confinement, the key and the flag use a hand-written fake SDK.

Read-only readers (`models`, `usage`; fake `fetch` tests only): see the T3 comparison below.

Not done, so Planned: any run against the real `@cursor/sdk` (the transcripts were recorded by T3, and the error class names,
the `session_lost` message match and the `CURSOR_API_KEY` handoff are unverified); loading the SDK from disk in the single executable
(its webpack chunks and native binaries; T3 loads it with `createRequire`); background shells as `background` frames (T3 reads an
SDK-internal hook, no transcript covers it) and `stopTask`; `readHistory`, `rollback`, `fork`; attachments;
subagent, todo and plan transcripts (`proposed_plan` was not copied: it holds a personal path); per-session confinement; Windows.

## OpenCode family

`src/families/opencode.mjs` runs the promise client of `@opencode/client` (2.3.2, MIT, pinned and bundled into the
single executable) over `opencode serve` of the person's own `opencode` binary (never bundled; `options.opencodePath`
or `opencode` on PATH). Start it with `orbyn-engine-host opencode`; no flag. One server per distinct confinement, started
with `--hostname=127.0.0.1 --port=0` and a generated `OPENCODE_PASSWORD` (logged as an added key, never sent in a frame).
`confinement.argvPrefix` prefixes the server command and `confinement.env` is its whole environment; a malformed prefix is
`invalid_request`, and `options.serverUrl` (attach to a running server) is refused as `unsupported`.

Done and tested against replayed T3 Code transcripts of OpenCode 2.0.18 (MIT, `test/fixtures/NOTICE`): open (create, resume,
fork), multi-turn, every ask as a `permission` frame answered by `respond` (allow is `once` only, deny is `reject` with the
message, never `always`), cancel through `session.interrupt`, steer through `delivery: "steer"`, `fork` at a turn, `runDone`
exactly once from `session.execution.*`, error kinds. Sessions are created asking for everything; the core decides. Also replayed:
- **Question frames** (`question` transcript): an OpenCode form raised by the question tool becomes a `question` frame
  (`tool: "question"`, `input` is the tool call's own input); `respond` `{answers: {"<question text>": "<answer>"}}` becomes
  `session.form.reply` keyed by the form's field keys (a multi-select is split on commas); `{behavior: "deny"}` is `session.form.cancel`.
  A missing required answer is refused (`invalid_request`) and the question stays open. A form that is not a question is cancelled and logged.
- **Background frames** (`background`, `nested_background`, `subagent` transcripts): each subagent session (`session.created` with a
  parent) and each shell the sessions start is a `background` `started`/`ended` event with its `toolCallId` (`kind: "agent"` also has
  `description` and `background`; `kind: "shell"` has `command`). A task is announced when OpenCode names its tool call, so a shell
  with no tool call is announced when it ends. A successful result while a subagent (any depth) still runs **holds the run**
  (rule 1 of the run-end contract); the hold ends on the parent's wake turn, or after 3 s with no wake turn, on cancel (which also
  interrupts the subagents), after 30 minutes, or when the server stops. A failed or cancelled result is never held. Shells are
  reported, never held. The subagent events themselves also arrive as `delta` frames (their `sessionID` tells them apart), and a
  subagent's permission or question reaches the run and is answered on the subagent's own session.
- **`models`** (host-wide; optional `cwd`, `confinement`, `opencodePath`): `model.list` plus `model.default` as a list of
  `{id, providerID, name, efforts, capabilities, limit, status, enabled, family?, default}`; `efforts` are the model's variant ids.
  Settings, headers, body and package are never forwarded (they can hold provider keys). A server started only for the list is stopped after it.
- **`usage`**: the session's cumulative `{input, output, reasoning, cache, costUsd, model}` from `session.get` and
  `context: {usedTokens, maxTokens}` (the last step's tokens against the model's `limit.input ?? limit.context`; `null` if unknown).
- **`readHistory`**: `message.list`, oldest first, all pages, the engine's messages untouched; over about 6 MiB it fails with
  `kind: "host"` instead of cutting.
- **`stopTask`**: a shell through `shell.remove`, a subagent through `session.interrupt` on its session (`stopped` is the server's `interrupted`).
- **`background`**: `session.background` on the run's session; its effect is not documented (see below).

Real runs (macOS arm64, `opencode` 2.2.5, free model `opencode/beam-free`, empty temp folder, cost 0): a one-word reply completed with usage;
a file write raised one `permission` frame (tool `edit`), allowed once, the file was written and the run completed. `models`
through the bundled binary against a real `opencode serve` returned 245 models in the shape above, with a default and efforts, and
stopped the server it had started.

Not done, so Planned: cancel, steer, fork, question, background frames, hold, `usage`, `readHistory`, `stopTask` and `background`
against a live `opencode` (replays and fake clients only; no `-free` model was asked to run a subagent, a question or a shell);
what `session.background` does and whether `backgrounded` should be true (the SDK documents only `sessionID` and returns nothing,
no transcript calls it, so the host sends it and reports `true` on success); `rollback` (`session.revert.*` has a transcript
but no mapping to a turn id yet), `session.compact`; the v1 SDK (`@opencode-ai/sdk`, not added: no transcript covers it);
workflows; a user-started shell shown before it ends; attachments; per-send model; a silent-stream watchdog; Windows.

## Muse family

`src/families/muse.mjs` runs `@muse-code/sdk` (1.4.6, MIT, pinned and bundled into the single executable) over `muse serve`
of the person's own `muse` binary (never bundled; `options.musePath` or `muse` on PATH). Start it with
`orbyn-engine-host muse`; no flag. One `muse serve` per open session. `confinement.argvPrefix` prefixes the command and
`confinement.env` is its whole environment (without `confinement` the host's environment is used minus `META_API_KEY`);
`MUSE_NO_AUTO_UPDATE=1` is added and logged. A malformed prefix is `invalid_request`; `fork` and `options.serverUrl` are
`unsupported`. `options.disableSandbox` adds `--disable-sandbox` (Muse's own sandbox, for use under the core's confinement).

Done and tested against replayed T3 Code transcripts of Muse 1.4.3 to 1.4.6 (MIT, `test/fixtures/NOTICE`) under the real SDK
`Connection`: open (create, resume), multi-turn, every approval as a `permission` frame answered by `respond` (allow is
`allow_once` only, deny is the once-scope reject, never a policy amendment or session grant; if only a wider allow is
offered, allow is answered as a refusal), subagent approvals, cancel through `turn/interrupt`, steer through `turn/steer`,
`runDone` exactly once, workflows as `background` frames that hold the run until the follow-up turn ends it, error kinds.
Sessions always start asking (`promptUnmatched`, `:ask-me`); T3 recorded two of the four full-access.

Checked by hand: the bundled binary opened and closed a session against a stand-in `muse serve` script over real stdio
(no `muse` is installed here). Not done, so Planned: any run against a real `muse` or model turn; `question` frames
(`userInput/*`, from T3's adapter, no transcript); resume against a real server; subagents as `background` frames (their item
never ends in the recordings); `stopTask`, `readHistory`, `rollback`, fork; attachments; the denial message
(Muse's reject choice takes feedback, field unverified); session MCP; a person's message during a held run; the Linux `env`
trampoline. Windows launchers (`.cmd` and `.bat` under cmd.exe, PATH and PATHEXT) and verbatim turn roots follow T3 and are unit tested
with injected platform and file checks only; no Windows run.

## Codex family

`src/families/codex.mjs` is a plain-JS JSON-RPC client of the person's own `codex app-server` (never bundled;
`options.codexPath` or `codex` on PATH) on its documented newline-delimited JSON stdio. Start it with
`orbyn-engine-host codex`; no flag. No SDK, package or npm dependency was added. The app-server version the
recordings name is pinned as `0.156.1`.

- **Processes and confinement.** One app-server per distinct confinement (`argvPrefix`, `env`, `cwd`, path
  and env additions); sessions with the same confinement share it, and it stops with its last session.
  `confinement.argvPrefix` prefixes the command and `confinement.env` is its whole environment; `options.env`
  is added and its keys are logged, never its values. A malformed prefix is `invalid_request`; a family that
  could not apply confinement would be `unsupported`. Windows launchers (PATH/PATHEXT, `.cmd`/`.bat` under
  `cmd.exe`) are the `codexLaunch` helper, unit tested with injected platform and file checks only; no Windows
  run.
- **Options.** `{codexPath?, model?, effort?, mode: "read-only"|"workspace-write" (default read-only),
  approvalPolicy?: "on-request"|"untrusted", mcpServers?, developerInstructions?, env?}`; `open`
  `operation: "fork"` also takes `options.atTurn`. `mcpServers` is sent as `mcp_servers.<name>` in the
  thread's `config`, as T3 does; the host does not decide which tools are allowed. `effort` goes on each
  `turn/start` (the thread request has no effort field).
- **Requests.** `open` (create/resume/fork; the Codex thread id is `engineSessionId`), `send` (`turn/start`;
  `{runId, engineSessionId, turnId?, messageId?}`), `steer` (`turn/steer`; `accepted`/`rejected`, never
  `queued`), `cancel` (`turn/interrupt`, then `runDone` cancelled), `respond`, `rollback`, `fork` (at
  `atTurn`), `readHistory` (`{turns}` from `thread/read`), `closeSession`, `models`, `usage`. `stopTask`,
  `background` and `probe` are not advertised.
- **Approvals.** Every command, file-change, permissions, legacy patch/exec and MCP URL ask is a `permission`
  frame; `item/tool/requestUserInput` and MCP form elicitations are `question` frames. `input` is Codex's
  params untouched. `respond` allow is once-only (`accept`, never `acceptForSession`; permissions with
  `scope: "turn"`; legacy `approved`); deny is `decline`/empty permissions/legacy denied. The host never
  sends `approvalPolicy: "never"` or a `danger-full-access` sandbox: the recordings that held them are
  replayed with `on-request` and a read-only or workspace-write sandbox. There is no host timeout on a pending
  ask; cancel, run end and a crash refuse it, and a late `respond` is logged.
- **Frames and run end.** `delta.update` is `{method, params}` from Codex, untouched; a `session` frame is
  sent when a thread or turn id becomes known later; `background` frames report subagent threads as
  `{type, id, kind: "agent"}`. `runDone` is once per `send`; a turn only ends the run if its `turnId` is the
  prompt's (or the first one, if none was known); other threads' turns are deltas. A subagent still running
  when the parent's turn completes holds the run, and the hold ends on the last agent plus a short follow-up
  grace, on cancel, after 5 minutes without a notification, after 30 minutes, or when the app-server exits
  (running agents are then `background` `ended` with `status: "stopped"`). A background shell is neither held
  nor reported. Usage is the `thread/tokenUsage/updated` delta against the thread baseline, as T3's
  `CodexTurnTokenUsage.ts` does; failure kinds follow Codex's `codexErrorInfo` and message patterns, with a
  `retry` for rate limits, network loss and overload.
- **models and usage.** `models` uses a throwaway app-server: `model/list` pages become
  `{id, name, isDefault, efforts: [{id, label, isDefault?}], defaultEffort?}`, plus `account?` from
  `account/read`. `usage` is `{usage, limits}`; `limits` maps `account/rateLimits/read` primary/secondary/
  individual windows with the plan label. A limits failure is `unavailable: "probeFailed"`, never a request
  error. Both are synthetic-data tests shaped like T3's, not recordings. T3's `resetCredits` is not in this
  document's limit shape and is not carried.

Tested against the 28 replayed T3 Code `codex_transcript.ndjson` recordings (MIT, `test/fixtures/NOTICE`;
app-server 0.156.1), copied unmodified into `test/fixtures/codex/`: simple, multi_turn, message_steering,
queued_turn, turn_interrupt, turn_interrupt_mid_tool, tool_call_read_only_on_request,
tool_call_restricted_granular, tool_call_workspace_never, command_output_streaming, plan_questions,
proposed_plan, todo_list, web_search, provider_thread_resume, thread_fork_native_prior_turn, thread_rollback,
thread_rollback_after_restart, thread_rollback_to_stopped_turn, subagent, subagent_continue, subagent_v2,
subagent_v2_approval, subagent_v2_nested, subagent_v2_nested_approval, thread_merge_back_siblings,
delegated_task_status, codex_model_reroute. `test/replayCodex.mjs` plays a transcript as a fake app-server (it
asserts each recorded request, rewrites ids, ignores T3's recorder config/approval values, and feeds the
recorded replies), and `test/codex.test.mjs` drives every scenario through `runHost` with in-process streams.
Also tested with hand-written fake app-servers: hold-open, confinement, launcher, approval refusal on cancel,
late respond, attachments, readHistory, fork, structured error kinds/reset, models and usage. One test runs a
stand-in app-server script through the default spawn over real stdio, so the child-process path is exercised.

**Nothing here has run against a real Codex turn yet.** The wire, ids, error codes, reset-text timezone and
approval options are T3's recordings, not a live `codex`. Not done, so Planned: `stopTask`/`background`;
`probe`; a real login/auth flow (the host reads no credential and adds no key); URL-mode MCP elicitations
beyond answering `cancel`; distinguishing Codex's own review/compact/memory agents from subagents (they would
hold the run); a quiet/held run longer than the test windows; `resetCredits`; the static/update-required model catalogue of
`codexModelCatalog.ts` (the host reads `model/list` only); T3's per-turn `additionalContext` runtime
instructions (`developerInstructions` is passed as the thread request's string); the `session` server requests
for dynamic tools, attestation and ChatGPT token refresh; a `fork` request that also creates a host session
(the caller opens one); a read-history size cap; and any Windows run. Compared with T3's adapter: one
app-server per confinement group instead of one per provider (so a per-run prefix can still apply), `open`
resume loads the thread eagerly, `rollback.toTurn` names the turn a rollback keeps (later turns are removed;
T3 counts terminal turns after its target), MCP URL asks answer `cancel` on allow, no always/session approvals,
no T3 thread names or recorder `config/read`, and no T3 orchestration child threads.

## ACP family

`src/families/acp.mjs` is the generic Agent Client Protocol family: one agent process per open session, spoken to over
newline JSON-RPC 2.0 on stdio. It starts with `orbyn-engine-host acp`; no flag. The core resolves which agent to run and
where its binary is, in `open.options.agent = {command, args, env, id}`; this host never downloads, installs or resolves a
registry and never fetches the network. `options.agent.id` selects a per-agent quirks entry from
`src/families/acpAgents.mjs` (`default` only in this task; droid, omp, kimi, kiro and vibe come later).

- **Initialize.** ACP protocol version 2, `clientInfo` "Orbyn", `clientCapabilities` advertising only form elicitation:
  **no `fs` and no `terminal`**, so the agent keeps using its own tools under the core's confinement. An agent that sends
  `fs/*` or `terminal/*` anyway gets JSON-RPC method-not-found and a `log` frame naming the method only (T3 advertises
  and implements both; this family deliberately does not). `authenticate` runs only when session setup says sign-in is
  required (-32000) and `options.authMethodId` names an advertised method; otherwise `open` fails `kind: "auth"`.
- **Sessions.** `session/new`, or `session/load` for `resume` (only when the agent advertises `loadSession`, else
  `kind: "unsupported"`), or `session/fork` for `fork` when the agent advertises it. MCP servers from
  `options.mcpServers` are sent in the transports the agent advertised; one it does not support is skipped and logged
  by name. `confinement.argvPrefix` is prepended and `confinement.env` is the whole environment plus the agent's own
  `env`; a malformed prefix is `invalid_request`. Windows launch resolution follows `acpLaunch` (the same PATH/PATHEXT
  and `.cmd` handling as `museLaunch`).
- **Policy stays in Rust.** Every `session/request_permission` is a `permission` frame (`tool` is the tool call's title,
  `input` the request's own tool call, with fields the `tool_call` update already carried filled in; `options` as
  received) answered once by `respond`. Allow is only `allow_once`; deny is `reject_once`; an agent offering only an
  always-allow option is denied. There is no host timeout. Cancel, run end and a crash refuse pending asks. Elicitation
  forms become `question` frames answered by `respond` `{answers}` (multi-selections comma-joined) or `{behavior:"deny"}`
  (sent as `action: "cancel"`).
- **Turns.** `send` writes `session/prompt` with the person's text, plus image or resource blocks only when the agent's
  `promptCapabilities` allow them, answers when the prompt is on the wire, streams each `session/update`'s `update`
  object untouched as a `delta`, and ends with exactly one `runDone` from the prompt response (`end_turn` completed,
  `cancelled` cancelled, `max_tokens`/`max_turn_requests` maxTurns, `refusal` refused, an error failed with a `kind`).
  A malformed `session/update` is dropped and logged by method only. `steer` cancels the running prompt and sends the new
  text as the run's next prompt; `cancel` sends `session/cancel` and ends the run cancelled; `closeSession` cancels and
  stops the process.
- **Models.** `models` with an open `sessionId` returns the session's catalog; without one it takes `agent`, `cwd` and
  optional `confinement`, starts a throwaway agent, reads `session/new`, and stops it. The shape is Muse's
  `{models: [{id, name, isDefault, efforts: [{id, label, description?, isDefault?}], defaultEffort?}]}`, built from
  ACP `configOptions` (`model`, `thought_level`/`reasoning`, the Mistral Vibe `thinking` quirk, `mode`) with `_meta.reasoningEfforts`
  winning per model. `options.model`, `options.effort` and `options.mode` at `open` or `send` go through
  `session/set_config_option`, or `session/set_model` when only a model state is advertised.
  Capabilities: `steer` and `models`.

Done and tested against replayed T3 Code transcripts of generic ACP agents (MIT, `test/fixtures/NOTICE`):
simple, multi-turn, queued turn, message steering, a read-only tool call, a permission ask answered once, turn
interrupt, a todo list, two elicitation forms, a malformed session update and a failed prompt. Nothing here has run
against a real agent yet.

Not done, so Planned: any run against a real agent or model; per-agent quirk entries; background work, `stopTask`,
`background`, `rollback`, `readHistory` and a `usage` request (ACP has no background event; usage rides the update
stream); `session/resume` for agents that do not advertise `loadSession`; `additionalDirectories`; T3's per-agent MCP
transport preference (mcpTransport.ts) and its fs/terminal/URL-elicitation client policy (never ported); attachments
against a real agent; Windows.

## Differences from T3 Code on purpose (Muse, Cursor and Codex)

Compared with T3 `packages/provider-muse` (`sdk.ts`, `adapter.ts`), `packages/provider-cursor` (`CursorAgentSdk.ts`,
`CursorAgentSdkWorker.ts`, `adapter.ts`) and `packages/effect-codex-app-server` plus the Codex adapter files named in the section above,
commit 9168ca7. Kept different: policy stays in Rust, so Muse always starts asking (`promptUnmatched`, `:ask-me`) where T3 maps full
access to `allowAll` and `:unrestricted`; Muse requests no `sessionMcp` capability (no session MCP yet); Cursor has no approvals; the pooled
Cursor host takes no `argvPrefix`; the Cursor SDK is not bundled and is behind `ORBYN_ENGINE_HOST_CURSOR=1`; the API key is only
`CURSOR_API_KEY` at spawn and is redacted; no keychain or stored key. Codex keeps T3's asking shape (`on-request`, read-only or
workspace-write), never T3's full-access `never`/`dangerFullAccess`, and one app-server per confinement group rather than one per provider.

Readers added (read-only, through the existing `models` and `usage` requests, capabilities `models` and `usage` on Cursor, Muse and Codex; not a
protocol bump). Cursor, from `provider-cursor` `CursorSdkCatalog.ts` (`/v1/me`, `/v1/models`, 30 minute cache, account label dropped when a team
blocks it) and `usageLimits.ts` + `apiKey.ts` (`GetCurrentPeriodUsage` through the key exchange): `models` returns `{models, account?}`, `usage`
returns `{usage, limits}` (`src/families/cursorReaders.mjs`). Muse, from `provider-muse` `modelCatalog.ts` (`model/list` from a throwaway
`muse serve`), `protocol.ts` (`MuseSubscriptionUsage`) and `usageLimits.ts` (`museUsageToLimits`): `models` returns `{models}` with efforts, and
`usage` returns `{usage, limits}` from the latest account-wide `usage/changed` (`limits` is `null` until Muse sends one). Codex, from
`Drivers/CodexProvider.ts` (`model/list` with effort descriptors, `account/read`), `CodexTurnTokenUsage.ts` and `codexUsageLimits.ts`:
`models` returns `{models, account?}`, `usage` returns `{usage, limits}` (`limits` from `account/rateLimits/read`). The key, access token
and any response body never appear in a result or log; failures carry fixed text. All of it is tested with a fake `fetch` and a fake
connection only: nothing ran against the real Cursor service, `@cursor/sdk` or `muse`, and the endpoint paths and field names follow T3,
unverified here.

T3 has more that is not added: Cursor `accountUsage.ts`, `CursorUsageAccounts.ts`, `accountCache.ts` and `CursorAccountReader.ts` (billed-event
history, team lookups and an on-disk cache for T3's cost screens; this host keeps no files, and the core owns cost history); `credentialStore.ts`
and `auth.ts` (stored keys, sign-in); Muse `usage.ts` and `MuseUsageHome.ts` (they scan `session.jsonl` transcripts under Muse's home for token
history, a file reader the host does not need; the core reads them, parity J27); Muse `isMuseUsageLimitFailure` and `museUsageLimitResetAt` (retry scheduling from refusal text: the core owns
retry, and Muse's quota reset already rides `retry.resetsAt`); Codex `resetCredits` (this document's limit shape has no field for it).

## In the app bundle (macOS arm64)

`scripts/prepare-bundle.mjs` builds the host and stages it as `binaries/orbyn-engine-host-<triple>`; every other target
stages an empty placeholder (Tauri needs each `externalBin` file), and Intel Macs keep the Rust drivers.
`tauri.conf.json` lists it in `externalBin` and sets `bundle.macOS.entitlements` to `entitlements.plist`, which Tauri
applies when it signs each sidecar and the app. The desktop shell sets `ORBYN_ENGINE_HOST` to the binary's path for
`orbynd` when the file is non-empty; with none, the service keeps the Rust drivers.

Checked by hand on macOS arm64: a stand-in app bundle signed ad hoc in Tauri's order (sidecars, then the bundle,
hardened runtime) verifies, and the host launched from inside it answers the handshake. With the same signing but
without the entitlements the host crashes (SIGTRAP), so the entitlements are required. Not checked: a full `tauri build`
(the app was not built), notarization, other platforms. The entitlements apply to the whole app, not only the host.

**Why the host needs them.** The host is a Node single executable application: it embeds Node, whose V8 engine compiles
JavaScript at run time. Under the hardened runtime that needs `allow-jit` and `allow-unsigned-executable-memory`, and
`disable-library-validation` lets the injected Node runtime load code that was not signed by the same team. Without them the
host crashes at start (SIGTRAP, checked by hand). Nothing else in the app needs them. They are a risk for `orbynd`
(docs/account-threat-model.md control 6), so a release must carry them on `orbyn-engine-host` only;
`scripts/check-macos-entitlements.sh --release` enforces that. Signing a release that way (one entitlement file per
binary, not the single `bundle.macOS.entitlements` Tauri applies to everything) is Planned with Developer ID signing.
