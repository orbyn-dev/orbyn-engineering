# orbyn-engine-host

The Node host that runs engine SDKs (Claude, Cursor, OpenCode, Muse, later Codex, ACP, Pi) for the Rust
core over newline-JSON on stdio. The wire contract is [docs/engine-host-protocol.md](../docs/engine-host-protocol.md);
change that document in the same PR as any frame change.

Status: skeleton. Only the `fake` family exists, for the `HostDriver` tests. Not part of the Rust
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

`AskUserQuestion` is a `question` frame answered with `answers`.

Also done, from the SDK's own types (0.3.321), tested against a fake query: `models` (a throwaway query),
`usage` (`getContextUsage`), `stopTask`, `background` (`backgroundTasks`).

Not done, so Planned: the leftover-turn echo rule (needs the CLI's replay flag and a run with a valid login to
confirm), `rollback`, subagent
transcripts, and any run against a real `claude` with a valid login.
