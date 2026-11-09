# orbynd

The local service and command line: transport over
[orbyn-core](../orbyn-core/README.md), with no product rules of its own.

## Contract

| Binary | Role |
| --- | --- |
| `orbynd` | HTTP, SSE and WebSocket API, the static UI, access control |
| `orbyn` | The command line; it calls a running service through the same API as the UI |
| `orbyn-fake-engine` | A deterministic protocol double for tests; never shipped as an engine |

## Layout

| Path | Holds |
| --- | --- |
| `src/main.rs` | Start-up only: arguments, core configuration, termination, the serve loop |
| `src/routes/` | One module per area. Each owns its route list and its handlers; `mod.rs` merges them and applies the access layers |
| `src/http/` | Shared plumbing: `AppState`, `ApiFailure` and error mapping, `ApiJson`, request activity, access control |
| `src/cli/` | Helpers for the `orbyn` command (`service`, `stdio`, `trace`, `triage`) |
| `src/bin/` | The `orbyn` and `orbyn-fake-engine` binaries |

Handlers parse, call `orbyn-core` and map the result. A rule a handler would need
belongs in core. `routes/tests.rs` builds the real router and compares every method
and path, and which routes need a credential, with `tests/routes.json`. After an
intended route change, regenerate it with
`ORBYN_UPDATE_ROUTES=1 cargo test -p orbynd --bin orbynd route_table`.

## Consumers

- `web`, over the HTTP API and streams.
- `desktop/src-tauri`, which starts `orbynd` as a process.
- People and scripts, through `orbyn`.

## Tests

`tests/` drives the real service against the fake engine, one file per area.

## Known limitations

- It binds to loopback unless started with `--remote`, which needs
  `--tls-terminated`: TLS comes from Tailscale, SSH or a proxy, never from
  `orbynd` itself.
- The fake engine's frames are written by hand from each engine's schema, not
  recorded from live engines.
