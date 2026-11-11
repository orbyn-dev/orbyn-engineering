# Crew Protocol objects

Rust types for [Crew Protocol v0](../../../../docs/crew/protocol/SPEC.md). The schemas there
are the check; `tests/crew_protocol/types.rs` reads every example through these types.

- `CrewObject` is every object, the answer key included. It belongs to the core: never
  return it to a worker or member (H1). `WorkerObject` is the same union without the key,
  and is what a worker-facing reader returns.
- **Shape and hash only.** `CrewObject::parse` checks shape, rules and that `id` is the hash.
  It does not scan for secrets (H2) or check the answer-key leak (H1's string check). Those
  live at the tool boundary in slice 2, where every object enters the core.
- **Storage.** Orbyn's store keeps these objects (migration 29). Deleting a crew task
  cascades to all its objects on purpose. `run_id` is the protocol's own id and has no
  foreign key to Orbyn's runs or threads; the lifecycle that ties them arrives in a later slice.
