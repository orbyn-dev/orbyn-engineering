# Windows runtime parity evidence: 16 November 2026

Related: [#315](https://github.com/orbyn-dev/orbyn-engineering/issues/315).
This adds O19 startup adoption and O27 functional measurements. Both rows remain
partial. Earlier I58, O1, O19 and O27 evidence in [parity.md](parity.md) is retained.

## Inputs and isolation

- Source: `202300f998e99e3d96d5d1cc6dccd2f33170e5b1`, clean at build and proof time.
- Windows 11 Pro x64 build 26200; Ryzen 5 5600X, 6 cores / 12 threads; 32 GiB RAM.
- Rust/Cargo 1.100.0, Node 24.18.0, npm 11.16.0. A new checkout-owned Cargo target
  produced the service, CLI and fake engine with a locked build, one job, no
  incremental compilation and no debug information. Package sources were cached;
  compiled outputs came from this run.
- Fresh `npm ci`, web check, all 283 Vitest files / 3,190 tests, and production
  build passed. O19 served the explicit frozen production UI directory; O27 used
  the original benchmark helper's current-source Vite development UI.
- Task-local Playwright Chromium headless shell revision 1248, version
  156.0.8078.4; no installed Chrome or native app was used.
- Each proof had fresh private home, config, data, temp and browser state. Built-in
  providers pointed to absent absolute paths, known-engine discovery was empty,
  offline mode was enabled, and provider/forge command guards recorded zero hits.
  The benchmark used its scripted fake engine, with no real-provider turn.
- The supervisor assigned suspended children to an owned Windows Job before
  resuming them. Normal descendant exit and timeout tree termination passed a
  bounded capability check. Each proof's process tree ended; browser/service
  cleanup and post-run integrity checks passed.

The three binaries and production UI were physically copied before the proofs.
Git objects matched across the producers at freeze. Source bytes, helper hashes,
binary hashes, UI files and the browser executable hash matched their recorded
inputs before and after each proof.
Read-only attributes are operational protection; full external SDK, package-cache,
toolchain and DLL byte closure is not claimed. This is path/environment isolation,
not an arbitrary-code sandbox.

| Frozen executable | SHA-256 |
| --- | --- |
| `orbyn.exe` | `8c1b5ccb2bc954e006cbce888e0fab8defc812777f35f8569456ed7724fe1f0a` |
| `orbynd.exe` | `7d3e8def9ac794683ea6917909c5b0891c28c65ff3c7c6efd283b58b46b8a7c4` |
| `orbyn-fake-engine.exe` | `ccc288e0cc118cf83f85a0f9f51d2838293c498e44653d72f42fb6e19a00f77c` |
| `chrome-headless-shell.exe` | `f77d8e27c8db1e4ac0399459a00e0c97bb9417adc393474cabaffebe2314ecb0` |

An initial npm version check failed because the private runner assigned the same
empty file to npm's user and global config. Separate private empty files corrected
that fixture failure. The original failure and fresh Rust-build receipt were
preserved. An actual cached locked build then reconfirmed the same source,
toolchain and all three binary hashes under the corrected helper before freezing.

## O19: startup adoption

The fresh service initially published no environment theme. Its CLI successfully
ran `theme set light` against that service's private data directory **before** a
new browser context booted. The context preferred dark and had no stored theme.
The browser's `/api/info` response published light; the document's `data-theme`
and `orbyn.prefs.v1` theme became light. Both the published `set_at` and persisted
`environmentThemeAppliedAt` were `1794776815435`.

Result: passed. This proves startup adoption of the CLI-published preference in
that fresh browser. It adds no native OS focus, live watcher or macOS evidence.
The earlier set/show/clear and explicitly dispatched DOM-focus observations remain
historical evidence with their original limits.

## O27: unchanged seeded benchmark

The tracked [benchmark](../web/e2e/real/render-benchmark.spec.ts) passed 1/1 against
the freshly built service/fake engine and current-source Vite development UI in a
fresh browser. Its original `bench` scene,
assertions and deadlines were unchanged: 200 deltas of 256 bytes at 10 ms intervals,
50 KiB total, with the complete assistant reply ending `END.`. A private config
selected this one spec; it did not substitute the workload or budget.

| Measurement | Recorded value |
| --- | ---: |
| Wall time | 6,465 ms |
| Main-thread task time | 1,983 ms |
| Script time | 1,562 ms |
| Layout time | 62 ms |
| Style time | 48 ms |
| Used heap | 54,748,056 bytes |
| Heap growth | 26,039,160 bytes |
| DOM nodes | 2,924 |
| Mutation records | 938 |
| Mutated nodes | 503 |

CPU load snapshots were 6% before and 8% after; free physical memory was
7,941,116 KiB before and 7,914,748 KiB after. Other owned heavy jobs were idle,
but owner applications remained open. These are **nonquiet functional
measurements**, not a certified quiet-machine baseline or evidence of a speedup.
There was no reference-UI comparison or current macOS run. Those O27 requirements
remain outstanding.
