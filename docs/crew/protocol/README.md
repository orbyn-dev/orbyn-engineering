# Orbyn Crew Protocol (v0, unpublished)

An open protocol for engines of any kind to work as one crew under a lead: typed objects,
a small tool set, a task lifecycle and hard rules the core enforces. Read the
[spec](SPEC.md).

**Status.** Draft. Not published, and its version (`v: 0`) is independent of the app's.

## Licence

This folder is licensed under **Apache-2.0**: [LICENSE](LICENSE) and [NOTICE](NOTICE). It
holds the spec, the JSON Schemas, their examples and the conformance cases, so other
harnesses and tools can implement the protocol.

The rest of this repository, **including the Orbyn Engineering app and the Rust test that
runs these cases (`crates/orbyn-contracts/tests/crew_protocol/`), stays under PolyForm
Strict 1.0.0** (the `LICENSE` at the repository root). Publishing this folder as its own
repository is a separate decision by the owner; until then it lives here.

## What is here

| Path | What |
| --- | --- |
| [SPEC.md](SPEC.md) | Objects, messages, tools, lifecycle, hard rules, error codes |
| [schemas/](schemas/) | JSON Schema draft 2020-12, one file per object, plus `common`, `check`, `error` and an `object` union |
| [schemas/examples/](schemas/examples/) | `<schema>.valid.<name>.json` and `<schema>.invalid.<name>.json`; `schemas/invalid-examples.json` records where each invalid one must fail |
| [conformance/](conformance/) | Data-driven cases and how to read them |
| [tools/stamp_ids.py](tools/stamp_ids.py) | Recomputes the content-address ids in the examples |

## Run the checks

```bash
cargo test -p orbyn-contracts --test crew_protocol
python3 docs/crew/protocol/tools/stamp_ids.py --check   # ids of the examples are current
```

The Rust test compiles every schema, validates every example (valid ones pass, invalid ones
fail), checks every example's id against the canonical hash, replays every conformance case
against the reference model, and keeps the spec, the error schema and the cases in step: every
error code has a table row in the spec and a case, and every hard rule `H1` to `H15` has a case.
CI runs it with the rest of `cargo test --workspace`, and a change under
`docs/crew/protocol/` triggers the Rust job even though it is documentation.

No case needs an engine, so none uses the fake engine yet. Cases that need one arrive with
the router and the bus (slices 2 to 4).

## Rust types and store tables (slice 1)

The Rust wire types live in `crates/orbyn-contracts/src/crew/`: one file per object (`Brief`,
`AnswerKey`, `Lease`, `CrewMessage`, `Fact`, `Claim`, `Report`, `Evidence`, `Finding`,
`Disagreement`, `Gate`, `CrewResult`, `RouteDecision`), the shared shapes, the refusal object
`CrewError` and the `CrewObject` union that carries the `type` tag. ts-rs writes their
TypeScript into `web/src/generated`.

- **Field names are the wire names.** Every object is a flat `snake_case` JSON object. The
  envelope fields (`v`, `id`, `crew_id`, `author`, `created_at`) are declared on each type by one
  macro, so `deny_unknown_fields` closes it the way `unevaluatedProperties: false` does.
- **Closed enums are Rust enums**, and a pattern string (a hash, a path, a member id, a commit,
  a loopback URL, a timestamp) is a newtype that refuses a bad value when it is read.
- **Integers only.** No `f32` or `f64` appears. `CrewObject::parse` refuses a float anywhere,
  even `30.0`, then checks `v`, the type's own rules (`validate`) and that `id` is the hash.
- **The schema is the check.** `tests/crew_protocol/types.rs` reads every valid example into its
  type and writes it back to the same canonical JSON, refuses every invalid example, and
  validates what the types write against the schemas. A field added to a type without the
  schema (or the reverse) fails it.
- **Store tables** (`crew_tasks`, `crew_briefs`, `crew_answer_keys`, `crew_messages`,
  `crew_leases`, `crew_evidence`, `crew_gates`, `crew_results`, migration 29) keep each object as
  canonical JSON keyed by its `id`, which is the content address, and verify it on the way in.
  The answer key has its own table and reader, so the brief a worker reads cannot hold it (H1).
  A task is keyed by its brief's id. Facts, claims, reports, findings, disagreements and route
  decisions have no table yet; they arrive with the router and the bus.

When the real router and bus exist (slices 2 to 4), they run the same
[conformance cases](conformance/README.md) through a second adapter. The reference model
in the test stays as the oracle.
