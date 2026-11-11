# Crew Protocol objects

Rust types for [Crew Protocol v0](../../../../docs/crew/protocol/SPEC.md). The schemas there
are the check; `tests/crew_protocol/types.rs` reads every example through these types.

- `CrewObject` is every object, the answer key included. It belongs to the core: never
  return it to a worker or member (H1). `WorkerObject` holds only what a worker is
  shown (brief, message, fact), none of the key or anything carrying its checks (evidence,
  gates, results), and is what a worker-facing reader returns. That reader shows a worker only the messages
  addressed to `all` or to that worker, never one to the lead or to another member. The H1 string check still
  runs on each object before it reaches a worker.
- **Shape and hash only.** `CrewObject::parse` checks shape, rules and that `id` is the hash.
  It does not scan for secrets (H2) or check the answer-key leak (H1's string check). Those
  live at the tool boundary in slice 2, where every object enters the core.
- **Storage.** Orbyn's store keeps these objects (migration 30). Deleting a crew task
  cascades to all its objects on purpose. `run_id` is the protocol's own id and has no
  foreign key to Orbyn's runs or threads; the lifecycle that ties them arrives in a later slice.
