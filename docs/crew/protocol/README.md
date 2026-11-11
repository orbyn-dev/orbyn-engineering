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
| [schemas/examples/](schemas/examples/) | `<schema>.valid.<name>.json` and `<schema>.invalid.<name>.json` |
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

## For slice 1: Rust types and store tables

Slice 1 adds the Rust wire types `Brief`, `AnswerKey`, `CrewResult`, `RouteDecision` and
`CrewMessage` to `orbyn-contracts`. The schemas are shaped for that:

- **Field names are the wire names.** Every object is a flat JSON object in `snake_case`
  with `additionalProperties`-style closure (`unevaluatedProperties: false`), so a Rust
  struct maps one to one. The envelope (`type`, `v`, `id`, `crew_id`, `author`,
  `created_at`) is one shared struct, flattened into each type with `#[serde(flatten)]`,
  and `type` is the serde tag.
- **Closed enums become Rust enums.** `kind`, `state`, `stop_reason`, check `kind`, error
  `code` and the others are `enum` lists in the schemas.
- **Integers only.** No `f32` or `f64` appears on the wire, so ids hash the same in every
  language. Rates travel as permille.
- **Validate, do not generate, in v0.** The types stay hand-written with the contract
  macros, and ts-rs still writes the TypeScript. The schema is the check: slice 1 extends
  `crew_protocol.rs` so that every valid example deserialises into its Rust type and
  serialises back to the same JSON, every invalid example is refused by the schema, and a
  sample of each type validates against its schema. A field added to a Rust type without
  the schema (or the reverse) then fails the test.
- **Store tables** keep the objects as canonical JSON keyed by `id`, which is the content
  address, so a row can be verified by re-hashing it.

When the real router and bus exist (slices 2 to 4), they run the same
[conformance cases](conformance/README.md) through a second adapter. The reference model
in the test stays as the oracle.
