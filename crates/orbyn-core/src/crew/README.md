# Crew

Crew is Preview: the engine-agnostic crew of agents of [docs/crew.md](../../../../docs/crew.md),
behind `CoreConfig::crew_preview`, which ships `false` (`ORBYN_CREW_PREVIEW=1` turns it on for the service). With it off, no Crew tool is listed or
callable and nothing here runs. The protocol's objects are in `orbyn-contracts` and are stored
by `store::crew` ([Crew Protocol](../../../../docs/crew/protocol/SPEC.md)).

## What it owns

| Module | Owns |
| --- | --- |
| `router` | Router v0: filters, scores from seeded defaults, an explicit engine honoured and still checked, and the `RouteDecision`. Pure: no model, no process, no file |
| `filters` | The hard filters, in order, each rejection with its reason |
| `score` | Pass rate for the brief's kind (seeded defaults until `EvalHistory` has a measured rate) and quota headroom, in permille |
| `tiers` | The tier a role needs and the model that fills it, matched against the models this machine lists |
| `view` | A plain copy of one engine's status, models and usage |
| `gather` | Builds the views from the engine registry, the model list and the limit report the engine picker reads |
| `stamp` | A brief draft becomes a brief: the core adds the envelope, a draft that carries one is refused |
| `guard` | The secret scan (H2) and the answer-key leak check (H1) |
| `persist` | Storing a decision with its task, after both checks |
| `tool` | `crew_route`, the dry-run tool |

## Rules to keep

- The router's inputs are the brief and this machine's own configuration. A brief has no
  field for the answer key, and no function here takes one except the leak check on storing.
- An engine or model the machine has not configured is never routed to. An explicit engine
  that fails a filter is refused with its reason, never swapped for another.
- Effort is `medium`, never higher. A review role is lead tier, and a named model below it is refused.
- Every rejection carries a reason, and a score carries its basis (`used_defaults`, or the
  measured rate with its sample size).
- No learning, trust score or counterfactual fork here: those are later slices, and
  `EvalHistory` is the one place they will feed in.

## How it is tested

Fake engines (`tests/fake.rs`) stand in for the registry, so each filter, each score, the
explicit override and the tool are checked with no process started. `tests/core.rs` runs the
tool through a real `Core` with a scripted engine driver.

The crate's [group table](../../README.md#groups) lists every group.
