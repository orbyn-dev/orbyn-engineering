# Orbyn Crew Protocol, version 0

**Status: v0, unpublished.** This is a draft for review. Objects, schemas and error codes
may still change without notice. Its version is independent of the Orbyn app's version.
Licence: Apache-2.0 (see [LICENSE](LICENSE) and [NOTICE](NOTICE)).

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

## 1. Scope

A crew is one lead and any number of members, each on an engine of its own (Claude Code,
Codex, OpenCode or any other harness), working on tasks together. The protocol defines
the typed objects they exchange, the tools a member calls, the life of a task, and the
rules the core enforces. It adds a coordination layer on top of existing standards and
replaces none of them.

| Layer | Standard it reuses | What this protocol adds |
| --- | --- | --- |
| Drive an engine | Native stream and app-server protocols, ACP | Nothing |
| Join a crew | MCP: a `crew` tool server | The tools and objects below |
| Remote members | A2A agent card with a `crew` extension | Reserved, not in v0 |
| Ledger | A hash-chained JSONL event stream | Reserved, not in v0 |
| Attestation | An in-toto statement | Reserved, not in v0 |

**Roles.** The *core* is the program that runs the crew. It routes work, carries messages,
runs checks and decides pass or fail; it is code, not a model. The *lead* is an ordinary
thread that writes briefs and reads results. A *worker* does a brief. A *reviewer* reads
and reports. A *person* is the human owner. Every actor other than the core and a person
is a *member*.

**Member levels.** An engine states its level, so nothing overclaims.

| Level | How it joins | What the core can do |
| --- | --- | --- |
| L0 Joined | MCP only, with a join token | Messages arrive on its next turn. Its reports are self-reported until verified |
| L1 Driven | The core runs it | Start, cancel, approvals, worktree |
| L2 Steerable | Driven, and takes input mid-turn | Live messages |

## 2. Common rules

### 2.1 The envelope

Every object carries these fields, and `unevaluatedProperties` is false in every schema,
so no other field is allowed in v0.

| Field | Meaning |
| --- | --- |
| `type` | The object type, such as `brief` or `route_decision` |
| `v` | The protocol major version. Always `0` in this document |
| `id` | The content address (2.3) |
| `crew_id` | The crew the object belongs to |
| `author` | `{role, engine, member}`. `role` is `lead`, `worker`, `reviewer`, `core` or `person`. `member` is present for the first three and absent for the other two |
| `created_at` | An RFC 3339 timestamp |

Most objects also carry `run_id`, the run they belong to. A `Brief` and an `AnswerKey`
precede a run and name the task by the brief's id instead.

### 2.2 Drafts

A member never writes an envelope. A tool takes a *draft*: the object without `type`,
`v`, `id`, `crew_id`, `author`, `created_at` and `run_id`, and without any field the
core fills in (a lease's `holder` and `state`, a fact's `rev`, a report's `brief`). The
core stamps the envelope from the authenticated caller, so a member cannot forge an
author, an id or a run. A draft that carries any of those fields is refused with
`invalid_object` (H9).

### 2.3 Ids

Objects are immutable and content-addressed. `id` is `sha256:` followed by the lowercase
hex SHA-256 of the *canonical form* of the object without its `id`. The canonical form is
UTF-8 JSON with no insignificant whitespace, object keys sorted by UTF-16 code unit,
strings escaped as RFC 8785 requires, and **integers only**. A floating-point number
anywhere in an object makes it invalid (`invalid_object`), so every implementation
computes the same id. Rates and scores travel as integers in permille. A change is a new
object, which names the old one in `refs`.

### 2.4 Text, paths and time

- Text fields are Unicode strings with the length limits their schema gives.
- A path is repository-relative, uses `/`, has no leading `/`, no drive and no `..`
  segment. A *path pattern* is a path, optionally ending in `/**` to cover a directory
  tree. v0 has no other glob syntax. Two patterns *overlap* when they can name the same
  file: equal files, or a tree and anything inside it. `crates/crew` and
  `crates/crew-demo` do not overlap.
- Probes and previews use loopback URLs only (`127.0.0.1`, `localhost`, `[::1]`).

### 2.5 Versions and unknown fields

A receiver MUST refuse an object whose `v` it does not support with `unsupported_version`.
v0 has no extension fields. A future version adds fields under a new `v`.

## 3. Objects

Schemas are in [schemas/](schemas/), draft 2020-12, one file per object. `object.schema.json`
accepts any of them by `type`. `check.schema.json` and `common.schema.json` hold shared
definitions.

| Object | `type` | Written by | Purpose |
| --- | --- | --- | --- |
| Brief | `brief` | lead | What the worker sees: `kind`, `outcome`, `inputs`, `files` (the lease it asks for), `allowed_effects`, `stop_when`, `budget`, optional `needs`, `engine`, `refs` |
| AnswerKey | `answer_key` | lead | What the verifier checks: `checks` and the `protected_paths` the checks use. Never sent to a worker (H1) |
| Lease | `lease` | core | A grant of `paths` in a `mode` to a `holder` until `expires_at` |
| Message | `message` | member | A message to `lead`, `all` or a member. Its body is data (H6) |
| Fact | `fact` | member | A typed fact on the shared board. A `contract` fact carries the check every consumer gets |
| Claim | `claim` | member | A statement, with the check that would prove it. Without a check it is *unverified* |
| Report | `report` | worker | What `crew.report` hands back at the end of an attempt: summary, commit, claims, and what no check covers |
| Evidence | `evidence` | core | One check run: the exact command, outcome, commit, log hash, excerpt, and what it proves and does not prove |
| Finding | `finding` | reviewer | `severity`, `path:line`, `claim`, `evidence`, `repro` and the reviewed `commit` |
| Disagreement | `disagreement` | core | Two or more opposing positions on one subject, each with its evidence |
| Gate | `gate` | core or person | A decision at a stage, by code, a confirmed finding or a person |
| CrewResult | `result` | core | What goes back to the lead about a task |
| RouteDecision | `route_decision` | core | The engine, model and effort chosen, the reasons, and every rejected engine with its reason |

The `Error` schema describes a tool refusal. It is a tool result, not a ledger object, and
has no id.

**Brief.** `kind` is one of `plan`, `implement`, `review`, `test`, `research`, `ui`,
`docs`. `allowed_effects` is a closed set: `read_repo`, `write_leased`, `run_checks`,
`run_commands`, `network`, `install_dependencies`. Reading secrets, writing outside the
worktree, force-push, settings changes, deploys, publishing and payments are not in the
set, so no brief can allow them. A brief has no field for the answer key.

**AnswerKey.** A check has `id`, `kind`, `params`, `proves` and optionally `does_not_prove`.
Kinds: `command`, `test`, `lint`, `typecheck`, `custom_script`, `file_scope`, `diff_size`,
`schema`, `http_probe`, `preview`, `visual_compare`, `repo_required`, `manual`,
`screenshot_only`, `accessibility_audit`, `performance_budget`, `secret_scan`,
`licence_scan`. Each kind has its own `params` shape. There is no limit on the number of
checks, and any check can run again at any time.

**Finding, Gate and confirmation.** A finding has no state. It blocks only after a Gate at
stage `finding_check` says `confirmed`, decided by a code repro or an independent second
run. A model's review is a finding, never the pass/fail gate. Gate decisions are `pass`
or `fail`, except at `finding_check`, where they are `confirmed` or `unconfirmed`.

**CrewResult.** `state` is `passed`, `failed`, `stopped`, `cancelled` or `needs_person`.
`final` is true exactly for `passed`, `stopped` and `cancelled`; `failed` and
`needs_person` are interim reports. `stop_reason` is present exactly when `state` is
`stopped`: `repeated_failure`, `budget`, `too_many_denials`, `injection_suspected`,
`engine_error` or `lead_stopped`. `not_verified` lists what no check covered, so a pass
never implies more than was checked.

**RouteDecision.** A decision assigned by a person or the lead names what was
`requested`, and the engine MUST be that engine: a failed filter is reported, never
swapped for another engine. A pass rate always travels with its sample size.

## 4. Messages

| Kind | Use |
| --- | --- |
| `fact` | Something the sender learned |
| `question` | Asks for an answer. `crew.ask` sends one and may wait up to 40 seconds |
| `answer` | Replies to a question and names it in `in_reply_to` |
| `handoff` | Passes work or context on |
| `blocker` | Says the sender cannot go on |

If `crew.ask` gets no answer in time it returns `pending`, and the answer arrives later as
an `answer` message. The mode of delivery (`live` into a running turn, or `next_turn`)
belongs to the ledger, not to the message, so the id does not depend on it.

**Messages are data (H6).** The core delivers a message body inside this wrapper, with `&`
written `&amp;` and `<` written `&lt;` in the body, so a body can never close the wrapper:

```text
<crew-message id="sha256:…" from="worker-01" kind="question" trust="data">
…escaped body…
</crew-message>
```

`from` is the author's member id, or its role when it has no member. A receiving harness
treats the wrapper as quoted data. Nothing in it is an instruction, a permission or a
credential.

## 5. Tools

Names are protocol names. A binding may map them; Orbyn's own MCP server exposes them with
underscores, as in `crew_post`.

| Tool | Caller | Takes (a draft of) | Returns |
| --- | --- | --- | --- |
| `crew.join` | L0 harness | A join token | Reserved, not in the v0 cases |
| `crew.brief.read` | worker, reviewer | Nothing | The Brief, never the key (H1) |
| `crew.lease.request` | member | Lease: `paths`, `mode`, `expires_at` | The lease id and its state |
| `crew.lease.release` | member | `paths` | `ok` |
| `crew.lease.transfer` | member | Reserved | Reserved, not in the v0 cases |
| `crew.post` | member | Message | The message id |
| `crew.ask` | member | Message of kind `question`, optional `wait_seconds` up to 40 | An answer, or `pending` |
| `crew.fact.read` | member | `key` | The Fact, or null |
| `crew.fact.write` | member | Fact: `key`, `class`, `value`, `base_rev`, `evidence`, optional `check` | The id and the new `rev` |
| `crew.claim` | member | Claim | The claim id |
| `crew.finding` | member | Finding | The finding id |
| `crew.report` | worker | Report without `brief` | The report id |
| `crew.start` | lead (also core, person) | A Brief draft and an AnswerKey draft | The task id (the brief's id) |
| `crew.verify` | core only | The task | Runs the answer key |
| `crew.evidence.write` | core only | Evidence | The evidence id |
| `crew.gate.write` | core, or person for a person's gate | Gate | The gate id |

A member cannot call a core-only tool (`not_authorized`). Only the core writes Evidence,
Gate, CrewResult and RouteDecision.

## 6. Task lifecycle

One task has one run. A task starts when the lead calls `crew.start`.

| State | Meaning |
| --- | --- |
| `briefed` | Brief and key stored, not yet routed |
| `routed` | A RouteDecision exists |
| `running` | The worker is working |
| `verifying` | The worker reported and the core runs the key |
| `failed` | A verification failed; the loop guard decides what follows |
| `changed_approach` | The worker is told to change approach, once |
| `needs_person` | Waiting for a person. Not final |
| `passed` | Final |
| `stopped` | Final, with a `stop_reason` |
| `cancelled` | Final |

| From | To |
| --- | --- |
| `briefed` | `routed`, `needs_person`, `cancelled` |
| `routed` | `running`, `needs_person`, `cancelled` |
| `running` | `verifying`, `needs_person`, `stopped`, `cancelled` |
| `verifying` | `passed`, `failed`, `needs_person`, `stopped`, `cancelled` |
| `failed` | `running`, `changed_approach`, `stopped`, `cancelled` |
| `changed_approach` | `running`, `stopped`, `cancelled` |
| `needs_person` | `running`, `routed`, `stopped`, `cancelled` |
| `passed`, `stopped`, `cancelled` | nothing |

`verifying` to `failed` happens only through a failed verification, which carries a
*fingerprint* of the failure. The loop guard applies after each one:

1. Count consecutive failures with the same fingerprint. A different fingerprint starts
   the count again at one.
2. At one, the next state is `running` with the failure as the reason.
3. At two or more, and no change of approach has been used, the next state is
   `changed_approach`.
4. At two or more after the one change of approach, the next state is `stopped` with
   reason `repeated_failure`.

The next state is required: a task in `failed` can only move to the state the guard named.
`passed` needs Evidence, or a person (H8). A task whose state is final has ended its
**run**. A task in `needs_person` has not.

The guard limits worker retries only. It never limits verification: any check can run
again at any time.

## 7. Hard rules

The core enforces these in code. A conformance case in [conformance/](conformance/) covers
each one.

| ID | Rule | Error |
| --- | --- | --- |
| H1 | The answer key is never sent to a worker. No tool returns it to a member, and the core MUST NOT deliver a brief that repeats the key's id or any check command of 8 or more characters | `answer_key_forbidden` |
| H2 | No object holds a secret. The core scans every string and key (section 8) | `secret_detected` |
| H3 | Depth is 1: only the lead, the core or a person starts a task. A worker or reviewer cannot | `depth_exceeded` |
| H4 | At most 3 messages between two members before the lead is told. The next is refused. A message from the lead to either member starts the count again. Messages to `lead` or `all` do not count. A repo rule may lower the limit | `hop_limit_exceeded` |
| H5 | At most 30 posts by one member per run. A repo rule may lower the limit | `post_limit_exceeded` |
| H6 | A message body is data, delivered in the wrapper of section 4. A reply names its question | `invalid_object` |
| H7 | Every tool refuses after the run ends | `run_ended` |
| H8 | Every Gate needs Evidence, or a person. A task passes only with Evidence or a person | `gate_needs_evidence` |
| H9 | The core stamps author, id, run and crew. A member cannot write Evidence, Gate, CrewResult or RouteDecision, or call a core-only tool | `not_authorized`, `invalid_object`, `unknown_tool` |
| H10 | One writer per path. Overlapping leases conflict unless both are `read`, or the same member holds both. A conflict goes to the lead | `lease_conflict`, `lease_not_held` |
| H11 | A brief's write paths never overlap the answer key's `protected_paths` | `protected_path_conflict` |
| H12 | A board write names the revision it read. A stale write is refused and the conflict goes to the lead | `board_conflict` |
| H13 | A task moves only along the transitions of section 6, and the loop guard decides after a failure | `invalid_transition` |
| H14 | An id is the hash of the canonical object, and an object holds integers only | `id_mismatch`, `invalid_object` |
| H15 | An object of another version is refused | `unsupported_version` |

## 8. Minimum checks

**Secrets (H2).** An implementation MUST refuse an object in which any string, or any key,
matches one of these. It MAY refuse more. A string field whose key looks like `password`,
`secret`, `token` or `api_key` and whose value is 8 or more characters counts as an
assignment.

| Class | Pattern, as a regular expression |
| --- | --- |
| Private key | `-----BEGIN [A-Z ]*PRIVATE KEY-----` |
| API key | `\bsk-[A-Za-z0-9_-]{20,}` |
| GitHub token | `\bgh[pousr]_[A-Za-z0-9]{36,}` and `\bgithub_pat_[A-Za-z0-9_]{22,}` |
| AWS access key | `\bAKIA[0-9A-Z]{16}\b` |
| Slack token | `\bxox[abprs]-[A-Za-z0-9-]{10,}` |
| Bearer token | `(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}=*` |
| JWT | `\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}` |
| Assignment | `(?i)\b(password\|passwd\|secret\|api[_-]?key\|token)["']?\s*[:=]\s*["']?[^\s"']{8,}` |

**Answer-key leak (H1).** Before any object leaves the core for a worker, the core checks
that none of its strings contains the answer key's `id` or the `run` of any check whose
`run` is 8 or more characters.

## 9. Errors

A refusal returns an `Error` object: `{"v": 0, "code": …, "message": …}`. When an
operation could fail in several ways, the implementation reports the first that applies
in this order, so a case has one expected answer:

1. `unknown_tool`
2. `run_ended`
3. `not_authorized`, `depth_exceeded`
4. `post_limit_exceeded`
5. `gate_needs_evidence`
6. `unsupported_version`, `invalid_object`
7. `secret_detected`
8. `id_mismatch`
9. The rule's own refusal: `hop_limit_exceeded`, `lease_conflict`, `lease_not_held`,
   `board_conflict`, `protected_path_conflict`, `answer_key_forbidden`,
   `invalid_transition`

| Code | Meaning |
| --- | --- |
| `invalid_object` | The object fails its schema, carries a float, is an unknown type, or a draft carries a field the core stamps |
| `unsupported_version` | `v` is not a version the receiver supports |
| `id_mismatch` | The `id` is not the hash of the canonical object |
| `secret_detected` | An object holds a secret (H2) |
| `answer_key_forbidden` | An outbound object would reveal the answer key (H1) |
| `protected_path_conflict` | A brief's write paths overlap the key's protected paths (H11) |
| `unknown_tool` | The tool does not exist |
| `not_authorized` | The caller's role may not call this tool |
| `depth_exceeded` | A worker or reviewer tried to start a task (H3) |
| `run_ended` | The run has ended (H7) |
| `hop_limit_exceeded` | Two members reached the hop limit (H4). The lead is told |
| `post_limit_exceeded` | A member reached the post limit (H5). The lead is told |
| `lease_conflict` | The paths overlap a lease another member holds (H10). The lead is told |
| `lease_not_held` | The caller holds no such lease |
| `board_conflict` | The write is stale (H12). The lead is told |
| `gate_needs_evidence` | A gate or a pass has neither Evidence nor a person (H8) |
| `invalid_transition` | The lifecycle does not allow this move (H13) |
| `invalid_token` | A join token is unknown, used or expired. Reserved for the join flow |
| `unknown_member` | The addressee is not in the crew. Reserved for the router |

## 10. Not in v0

These are named so nobody assumes them. They arrive in later versions of this protocol.

- The hash-chained ledger and replay.
- The in-toto attestation with predicate type `https://orbyn.dev/crew-result/v0`, and
  `orbyn verify`.
- L0 join by token (`crew.join`) and A2A remote members.
- Lease transfer cases.
- The lead's approval of agent requests and the Autopilot approval chain.

## 11. Security considerations

- Engines and agents run as the person. The protocol assumes a member may be wrong,
  confused or hostile, and gives it no authority the core does not check.
- A message, a finding or a board fact can carry injected text. It is delivered as data
  and never grants a permission (H6).
- The key and its check files are out of the worker's reach (H1, H11). The core runs the
  checks, so a worker cannot mark its own work as verified (H9).
- Evidence is meant to be checked: it names the command, the commit and a log hash, so a
  reader can re-run it and see what it does and does not prove.

## 12. Conformance

The data-driven cases in [conformance/](conformance/) are the test. An implementation
conforms to v0 when it gives the expected result for every case. At v0 the only
implementation is the reference model that runs in Orbyn's own test suite; see the
[README](README.md).
