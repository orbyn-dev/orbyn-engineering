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
disable-library-validation, the set T3 ships). Not checked: Developer ID signing, notarization,
Gatekeeper first launch, Windows, Linux, Intel macOS (T3 found SEA crashes there, so Intel keeps the
Rust drivers). The release job owns signing with the app's certificate; no certificate is used here.
The Tauri `externalBin` wiring and `scripts/check-desktop-assets.mjs` entry come with the first real family.
