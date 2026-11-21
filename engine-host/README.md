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
