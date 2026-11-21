# orbyn-engine-host

The Node host that runs engine SDKs (Claude, Cursor, OpenCode, Muse, later Codex, ACP, Pi) for the Rust
core over newline-JSON on stdio. The wire contract is [docs/engine-host-protocol.md](../docs/engine-host-protocol.md);
change that document in the same PR as any frame change.

Status: skeleton. The `fake` family drives the `HostDriver` tests; `claude` (behind a flag), `opencode` and `muse` are the real ones. Not part of the Rust
workspace or the web lint scope; it has its own `package.json`.

```bash
cd engine-host && npm test
node src/main.mjs fake
```

Layout: `src/frame.mjs` (encode, decode, error shape), `src/host.mjs` (handshake and dispatch),
`src/families/` (one file per family), `test/` (protocol tests against the fake family).

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

Not done, so Planned: the leftover-turn echo rule, models and usage requests, `rollback`, `stopTask`,
`background` request, `AskUserQuestion` as a `question` frame, subagent transcripts, and any run against the real SDK
or a real `claude`.

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
exactly once from `session.execution.*`, error kinds. Sessions are created asking for everything; the core decides.

Checked by hand: the bundled binary opened and closed a session against a real `opencode` 2.2.5 on macOS (no model turn).
Not done, so Planned: a real model turn, permission, cancel or steer against a live `opencode`; the v1 SDK (`@opencode-ai/sdk`,
not added: no transcript covers it); background shells and subagent sessions as `background` frames; `models`, `usage`,
`readHistory`, `rollback`, `stopTask`; question frames; attachments; per-send model; a silent-stream watchdog; Windows.

`AskUserQuestion` is a `question` frame answered with `answers`.

Also done, from the SDK's own types (0.3.321), tested against a fake query: `models` (a throwaway query),
`usage` (`getContextUsage`), `stopTask`, `background` (`backgroundTasks`).

Run-end rules: the prompt goes in with a `uuid`; a `result` that arrives after a `command_lifecycle` `queued` for it but
before `started` belongs to a leftover turn and is ignored (logged as `leftoverResult`); the first result while a
background agent or workflow runs is held, and the follow-up turn's result ends the run. Checked against a T3
transcript, and a real run (SDK 0.3.321, `claude` 2.1.327, haiku) showed `command_lifecycle` `queued` then `started`
for the prompt's uuid before the result. A leftover turn has not been seen on a real run.

Not done, so Planned: `rollback`, subagent
transcripts, and any run against a real `claude` with a valid login.

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
never ends in the recordings); `stopTask`, `models`, `usage`, `readHistory`, `rollback`, fork; attachments; the denial message
(Muse's reject choice takes feedback, field unverified); session MCP; a person's message during a held run; Windows launchers
(`.cmd`), verbatim paths and the Linux `env` trampoline.

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
