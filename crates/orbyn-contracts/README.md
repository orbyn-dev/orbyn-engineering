# orbyn-contracts

The wire contract: serialized types shared by the service and the UI, with no
behaviour. Timestamps are RFC 3339 UTC strings and ids are opaque strings.

## Contract

Every public type, one file per domain under `src/` (projects, threads, runs,
checkpoints, files, git, pulls, engines, tools, acp, history, usage, settings,
updates, workspace, terminal, preview, shell, automations, governance,
operations), and the Crew Protocol v0 objects in `src/crew/`, which
[docs/crew/protocol](../../docs/crew/protocol/README.md) specifies and its test checks. `lib.rs` only re-exports them, so paths stay `orbyn_contracts::Type`.
`npm run generate` in `web` (which runs
`cargo test -p orbyn-contracts`) writes their TypeScript into `web/src/generated`.

## Consumers

- `orbyn-core` and `orbynd`, as Rust types.
- `web`, only through `web/src/generated`.

## Known limitations

- `web/src/generated` is committed, not built. Commit it in the same commit as
  every contract change, or the UI and service disagree.
- A type's domain file is the only place to edit it; the `wire!` and `wire_enum!`
  macros live in `src/macros.rs`.
