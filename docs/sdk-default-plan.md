# Claude and Cursor SDKs in the engine host: plan

The Claude Agent SDK and the Cursor SDK may be used (decision of 2026-11-22), on the condition that Orbyn Engineering is open
source for public use. Nothing below is built yet unless it says so. Each step is its own small pull request.

Rules that stay: Orbyn never collects, reads or stores Claude.ai credentials; the `claude` binary is the person's own and is not
modified; the SDKs are dependencies used as shipped, never copied; any licence or notice file an SDK asks for ships with the app.
The written permissions are not copied into the repository.

| Step | What | Status |
| --- | --- | --- |
| 1 | This document and the wording of architecture rule 3, handoff.md and the engine-host README | this change |
| 2 | `docs/open-source-release.md`: the checklist for the owner (licence is the owner's choice) | Planned |
| 3 | `@anthropic-ai/claude-agent-sdk` and `@cursor/sdk` as exact-pinned engine-host dependencies; `npm audit`; their licence files; `THIRD_PARTY_NOTICES` in the app bundle | Planned |
| 4 | Packaging: a production `node_modules` with the two SDKs in the app resources beside the host binary, loaded by the host from that path (the single-file bundle cannot hold native files); the Claude SDK runs against the person's own `claude` through `pathToClaudeCodeExecutable`, so the SDK's own platform binaries are left out; native files signed; `scripts/check-macos-entitlements.sh` walks them | Planned |
| 5 | Core: the Claude path setting `sdk` or `cli`, default `sdk`, shown in the engine picker as Claude (SDK) and Claude (CLI); `ORBYN_ENGINE_HOST_CLAUDE` becomes opt-out; a down host shows its reason and the person may pick the CLI, nothing switches silently; sessions are the same transcript files on both paths | Planned |
| 6 | Verification: a short real Claude turn through the packaged app on a machine already signed in to `claude`, no tools; the packaged build checked with the entitlement script. Cursor needs a key: it stays unobserved in parity until a real run exists | Planned |
