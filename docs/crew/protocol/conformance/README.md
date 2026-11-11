# Crew Protocol conformance cases (v0, unpublished)

Language-neutral cases. An implementation conforms to v0 when it gives the expected result
for every case, run in file order against a fresh run. The cases are Apache-2.0 like the
rest of this folder. The Rust runner is
`crates/orbyn-contracts/tests/crew_protocol/main.rs`, with its reference model in
`crates/orbyn-contracts/tests/crew_protocol/model.rs`.

## Files

- `cases/<suite>.json`: `{"suite": …, "cases": [ … ]}`.
- `fixtures.json`: named fragments (callers, a standard `start`, an Evidence draft, a Gate
  draft) that cases reuse as `{"$use": "worker1"}`, with optional `"set": {key: value}`
  replacing top-level keys.

## A case

```json
{
  "id": "msg-hop-limit",
  "title": "The fourth message between two members is refused and the lead is told",
  "rules": ["H4"],
  "limits": {"exchanges": 3, "posts": 30},
  "steps": [ { "op": "call", "tool": "crew.post", "caller": {"$use": "worker1"},
               "args": {"to": "worker-02", "kind": "fact", "body": "…"},
               "expect": {"error": "hop_limit_exceeded", "lead_notified": true} } ]
}
```

- `rules` names the hard rules (`H1` to `H15`, SPEC.md section 7) the case covers. Every
  rule needs a case, and a case may not name a rule the spec lacks.
- `limits` is optional. A repo rule may lower the hop limit and the post limit; the
  defaults are 3 and 30.
- A step may carry `"repeat": N` to run N times.

## Steps

| `op` | Fields | What it does |
| --- | --- | --- |
| `call` | `tool`, `caller`, `args` | A tool call by a caller (`role`, `engine`, `member`). `args` is a draft |
| `validate` | `object` | Checks a full stamped object: version, shape, secrets, then id. `object` may be `{"$example": "<example file name>", "mutate": {key: value}}` |
| `wrap` | `object` | The data wrapper for a message |
| `transition` | `to`, `evidence`, `person` | A core lifecycle step. `evidence` is how many Evidence objects back it |
| `verify_failed` | `fingerprint` | A failed verification; the loop guard names the next state |
| `end_run` | | Ends the run |

## Expectations

`expect` is `{}` for success or `{"error": "<code>"}`, plus optional checks:

| Key | Meaning |
| --- | --- |
| `result_has` | Top-level keys of the result object that must equal these values |
| `result_text` | `contains`, `absent` and `counts` over the result as text |
| `delivered_text` | The same, over the wrapped message a recipient sees |
| `task_state` | The task's state after the step |
| `run_ended` | Whether the run has ended |
| `lead_notified` | Whether the lead got a notice from this step |

## Fake secrets

A string may hold `{{fake:github_token}}`, `{{fake:aws_key}}`, `{{fake:private_key}}`,
`{{fake:bearer}}` or `{{fake:assignment}}`. The runner expands it to a value of that class
at run time, so no secret-shaped literal sits in the repository.

## Adding a case

Name the rule, keep the case to one idea, and prefer a reused fixture to a long inline
draft. Run the test. If you add an error code, add its row to the SPEC.md table and the
`error.schema.json` list, and a case that expects it.
