# Autopilot

Slice 4f of [docs/crew.md](../../../../../docs/crew.md#autopilot), behind `CoreConfig::crew_preview`. Backend only: no
screen is built from it. The lead approves what an agent asks for; code, not the lead, limits what it may approve.
The merge queue, the after-merge Review queue and the watch on main are the second part of the slice.

| Module | Owns |
| --- | --- |
| `mod` | `Mode`: `off`, `on` or `full` (the default). The person chooses it per crew, repository or as the default |
| `hard` | The always-denied list N1 to N10 ([SPEC.md 7.1](../../../../../docs/crew/protocol/SPEC.md#71-never-allowed)) and its order |
| `hard_tools` | N5, N6, N8, N9 and N10: tables of programs and sub-commands |
| `shell` | Reads a command: simple commands, wrappers, `sh -c`, redirects, paths, hosts. Not a shell |
| `hosts` | `allowed_hosts` of `.orbyn/crew.json`, read at the commit the work started from |
| `tier` | What an action needs: nobody, the lead, or the lead and a second key |
| `chain` | The approval chain over the `Approver` trait: the lead, then a second key on another vendor |
| `tripwire` | 3 denials in a row, 20 in a run, and the injection detector |
| `ledger` | Writes the approval ledger rows: redacted, the key's text withheld, scanned |
| `review` | `Core`'s side: the mode, the approvers, and `ActionReview` for the gate |

The account marker is [`account::changes`](../../account/changes.rs): the list of account routes that change state,
which N1 refuses by path. `orbynd`'s route-table test fails when a route under `/api/account` is in neither list.

## How an action goes

`ApprovalGate::request_inner` asks `ActionReview` before any rule or person (bound by `McpService::bind_host` only
when the Crew Preview flag is on). For a Crew worker or reviewer:

1. **Ended task:** denied.
2. **Injection trip wire:** a request whose summary, target or (outside a file change) detail reads as injected
   instructions stops the task, `injection_suspected`. The ledger keeps the class, never the text.
3. **Always denied:** N1 to N10, before any approver, rule, mode or person's saved allow. The lead is never asked.
4. **Tier:** reads and writes inside the leased paths and the brief's own check commands need nobody. Network, installing
   dependencies, running a script or code the worker wrote, and CI files need two keys. The rest is the lead's.
5. **Chain:** the lead answers a [`Summary`](chain.rs) (kind, target, paths, the brief's outcome, the tier and why), never
   raw page or tool text, and must give a reason (an approval without one is a denial). Two keys: a second approver on
   another vendor (`family`) must also say yes. A disagreement is denied and logged as its own row. A person's deny rule
   still wins over an approval, and an allow rule or no rule lets an approval through.
6. **Trip wires** after each denial: 3 in a row or 20 in a run stop the task, `too_many_denials`.

Outside a Crew task only N1 (the account) is enforced, so an ordinary thread cannot call a state-changing account route.

**Modes.** `full` (default): the lead approves; without a lead, or without a second key on another vendor for a two-key
action, the action is denied (the row says `needs_second_engine`) and the work goes on. `on`: the same, but where
`full` would deny for want of an approver a person is asked. `off`: no one is approved by code, so every request goes to
a person as before, and the hard rules, the injection wire and the denial counts still apply.

## The always-denied list: what it reads, and what it cannot

It reads the kind, target and paths the gate is given, and for a command the simple commands inside it. It cannot see
what the gate is never asked: an engine's own file reads and its own network, which Orbyn does not hear about. It reads
no script's contents, so a script that does a forbidden thing inside itself passes unless it is a generated script (two
keys). It misses obfuscation and encoding (`base64 -d | sh` is caught as a pipe to a shell and two keys, not by name), aliases
and shell functions. A host in `allowed_hosts` is trusted whatever runs there. A package manager's own registry is not
checked against `allowed_hosts` unless the command names a host (installing is the two-key class). `git push origin
branch` is the repository's own remote and reaches the lead as network, two keys. N10 and N9 match by host and
program, and for tools and pages by word, so they can both miss and over-match. Tests pin the cases that matter and the
near misses that must pass (`grep -r stripe` is not a payment).

## Ledger

`crew_autopilot_events` (migration 38) is append-only: `allowed`, `denied`, `person`, `disagreement` and `tripwire`,
each with the action, who decided and the rule or reasons. Counters are read from the rows. This is not the hash-chained
ledger of slice 4c; when that lands these rows move into it.

## Seams for later slices

- **4i, blast-radius guard and data-class routing:** `tier::tier` adds a two-key reason for a destructive command against
  a production-linked target, so it needs two keys under Full too. Path classes and the router's blocking filter go in
  `crew::filters`, reading `.orbyn/crew.json` at the base commit as `hosts` does.
- **4j, autonomy by area:** `Core::crew_autopilot_mode` is the one place the mode in force is resolved. An area's
  demotion is a lower value taken there, with the person's chosen mode as the ceiling.

## Limits of Preview

- No engine-backed approver exists yet: `Approver` is the seam, and tests script it. Until one is wired, a lead that is
  not set means `full` denies and `on` asks a person.
- The lead's thread is not asked mid-turn; wiring an approver to a live lead thread and a second engine is [#456](https://github.com/orbyn-dev/orbyn-engineering/issues/456).
- The detector and the list are heuristics (above); real runs will show what they miss ([#457](https://github.com/orbyn-dev/orbyn-engineering/issues/457)).
