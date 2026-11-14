# Orbyn Crew: beta plan

> **Snapshot.** This is a copy of Products Research's plan as of 2026-11-11, kept here so engineers can read it
> after the planning workspace is cleaned up. Products Research owns the plan. Decision changes are folded in by
> later pull requests; until one lands, check with Products Research before relying on a row.
> Statements about the app's code ("already there", file and line references) are as of commit `aa909eee`.

Working name. A lead thread on any engine hands tasks to other engines (Claude Code, Codex, OpenCode and other ACP
agents). The engines talk to each other while they work. Code, not a model, routes the work, carries the messages and
decides pass or fail.

Contents:

- [Team, wiring and order of work](#team-wiring-and-order-of-work)
- [What exists today](#what-exists-today)
- [What is new](#what-is-new)
- [Design](#design)
- [Review stages](#review-stages)
- [Autopilot](#autopilot)
- [What keeps us ahead](#what-keeps-us-ahead)
- [Seeing who works on what](#seeing-who-works-on-what)
- [Workspace shell redesign](#workspace-shell-redesign)
- [Verification without limits](#verification-without-limits)
- [Prototype slices](#prototype-slices)
- [Warp features](#warp-features)
- [Beta cut](#beta-cut)
- [Open risks](#open-risks)
- [Appendices](#appendices)

The Crew Protocol (the open spec, its JSON Schemas and conformance cases) is written in its own pull request under
[docs/crew/protocol/](crew/protocol/). This file does not repeat it.

## Team, wiring and order of work

Everything here is decided (2026-11-11). The owner asked for one team: Products Research leads, two engineers build,
the Designer gates every screen. Nothing waits for a human decision unless it says "the owner".

### Who does what

| Session | Owns | In this plan |
| --- | --- | --- |
| Orbyn Products Research (lead) | This plan, decisions, order of work, cross-team questions | Decides. Engineers bring blockers here as one decision question |
| Orbyn Engineering 2 (Engineering) | The app: this repository (Rust core, `orbynd`, web UI, desktop shell) | Phase 1 app side, then Crew core, shell, Crew UI, Autopilot, parity rows, Warp features |
| Orbyn Engineering 1 (Planner and Engine) | The Engine repository `orbyn-engine` (accounts, record store, device sync, keychain, key service) and the Planner | Phase 1 Engine side. Later: connected-app grants, recovery and sharing |
| Orbyn Product UI/UX Designer | The private `orbyn-designs` repository | Gates every screen (rules below) |
| Orbyn Engineering 3 (orbyn.dev) | The site | Later: a Crew page and docs, once something ships |
| Orbyn Skills Builder 1 | Skills | Later: reviewer-lane skills (security audit and others) |
| The owner | Product owner | Staging deploy, DNS, run budget for live evals, publishing anything outside the repositories, the final read of each pull request after merge |

### Rules for working as one team

1. **One plan.** Changes to it go through Products Research. Engineers may add detail inside their own slice.
2. **The Designer gates UI.**
   - Pull `orbyn-designs` before building any screen. A frame that is **merged** there is buildable.
   - A frame that is missing, still in a pull request or still being drawn: ask the Designer directly and wait. Never
     invent UI, and never build from a frame that is only in a pull request.
   - Frame ids do not change after merge without telling Products Research.
   - Engineers ask the Designer design questions directly. Versions, scope and plan changes go to Products Research.
3. **Each repository owner merges their own green pull requests.** In the app, that is after review is done and CI is
   green. The skills library is the exception: agent-requested changes there need a human review and merge.
4. **Cross-repository order.** The Engine pull request merges and is tagged first. The app pull request then bumps its
   pin. Link the two pull requests.
5. **Engineers talk directly about technical contracts** (types, schemas, errors). Decisions about scope, order or
   product behaviour come to Products Research.
6. **Honest copy.** Preview and Planned labels stay until it works. No "first" or "only" claims.
7. **Evidence before "done".** Each slice's "Done when" must pass in CI or on the fake engine. Live evals need the
   owner's run budget (`evals.max_runs_per_pr` is still unset).
8. **Status.** After each merged pull request, send Products Research one short message: the link, what is next,
   anything blocked.
9. **Disk.** The machine is nearly full. Run the cleanup report at task closeout, as the harness says, and never empty
   the Trash.

### Phase 1: wire the Engine and Engineering (first)

Today the app has no Engine integration. The account, connect and phone screens are `preview-only` behind a
development flag, and every control says "Available with Orbyn accounts" ([design.md](design.md#planned-with-the-orbyn-platform)).
The Engine already has accounts and trusted devices, an encrypted record store with live sync, a device sync engine and
a keychain. Recovery, sharing, connected-app grants and Advanced protection are still Planned there.

The first wiring pull request's design note, [account.md](account.md), confirms or corrects each decision below.
Changes come to Products Research.

**Decisions**

| # | Decision |
| --- | --- |
| D1 | **The account is optional.** Signed out, Engineering and Crew work fully and locally. The account adds sync across devices. Sync failures never block work |
| D2 | **The beta uses the staging Engine** (`api-staging.orbyn.dev`), invite-only. Production sign-up stays off until the sign-up throttle redesign (Engine issue #42) lands, and recovery needs to ship before open sign-up |
| D3 | **The device is the machine running `orbynd`.** It links a new Engine crate, `orbyn-device-client` (native and async: sign-up with invite, email code and proof of work, sign-in, second-device approval, per-zone upload and pull, live notices, status events). That crate builds on `orbyn-auth-client` and `orbyn-client`, which are IO-free primitives (the sign-up and sign-in call sequence lives in the Planner's TypeScript today). The password never leaves the device, and device keys sit in the OS keychain. The web UI never holds account keys, so Engineering does not need `orbyn-wasm`. *Revised by Engineering 1* |
| D4 | **What syncs, as encrypted zones.** First record kind: personal settings that follow you (theme, keybindings, composer and project defaults). Next: execution profiles, workflows, prompts, env-set names and personal Crew defaults; trust scores and review memory. Conflicts use `orbyn-sync` field-by-field merge |
| D5 | **What never syncs by default:** the Crew ledger, transcripts, diffs, env-set values, provider keys, join tokens and any secret. A later opt-in can share task summaries and attestation hashes |
| D6 | **No hand-copied types.** `orbyn-contracts` is the HTTP wire only (the server sees a record type and a zone id, and payloads are encrypted on the device). Engineering's record fields live in a new domain crate, `orbyn-engineering`, with merge rules through `orbyn-sync`. The Engine tags the workspace (`engine-v0.N.0`; crates are not published) and the app pins it through git and a tag. *Revised by Engineering 1* |
| D7 | **Connected apps (Planner and Engineering) come after the beta.** The Engine's grants are still Planned. The L52 to L55 preview screens stay `preview-only` and excluded until then |
| D8 | **Crew proof signing** with an account device key comes after the beta. Until then the attestation uses a local key in the keychain |
| D9 | **Licences.** The app is PolyForm Strict and the Engine is MIT or Apache-2.0, so the app may depend on Engine crates (with notices) and the Engine must never depend on app code. The Crew Protocol spec and schemas will be Apache-2.0 in their own public repository, but publishing it is the owner's call. Until then they live in this repository under `docs/crew/protocol/` |
| D10 | **Every trusted device holds every zone key of the account.** An Engineering device can open Planner's zones and the reverse, until connected-app grants exist (D7). Copy must not imply per-app isolation. Because engines and agents run as the person, device keys and the app's data folder must not be readable by engine subprocesses: a written threat model, with the keychain access rules, is part of the first app wiring pull request, and signing in stays off until it passes review. Reading credentials is already on the Autopilot hard-denied list |
| D11 | **The Engine runs locally on the owner's Mac for development and testing, before anything goes to staging.** Both engineers use one local instance from the Engine's compose stack (`docker compose up -d`: the API at `http://127.0.0.1:8080`, Postgres on `127.0.0.1:5432`). It runs invite-only with the staging settings (`ORBYN_SIGNUP`, `orbyn invite`, `ORBYN_MAIL=log-codes`, so email codes appear in the logs). The app needs a configurable Engine address: staging by default, a development override for the local one, and plain `http` is allowed only for loopback addresses. The cross-repository end-to-end test runs against this local stack first. Staging is deployed by the owner only after it passes here, and nobody else deploys |

**Work**

| Owner | Work | Done when |
| --- | --- | --- |
| E1 | Add the `orbyn-engineering` crate (the `engineering.settings` zone first) and `docs/engineering.md`, which names every record, key and merge rule and states the D10 key-sharing fact | Schemas validate; merge tests pass |
| E1 | Build `orbyn-device-client` (D3). The device registration also gains an `app` label (`planner` or `engineering`) bound into the signed register message, which closes Engine issue #73 | A fake-server test passes; the device list shows the label |
| E1 | Tag the workspace (`engine-v0.1.0` first; `engine-v0.2.0` adds the device client and the v3 approval, see [Engine pin](account.md#engine-pin)) | The app pins the tag |
| E1 | Keep staging working for invite-only sign-up on a desktop client, not just the Planner web app | A new invited person signs in from the app against staging |
| E2 | A new account module in `orbyn-core` built on the `orbyn-device-client` trait, against a fake server until the crate lands: session, device keychain, trusted-device approval, a sync worker per zone, status events, and the D10 threat model | Unit tests with a fake server; no password in any request body |
| E2 | Settings, Account for real (sign in, devices, sync status, sign out), replacing the preview-only sign-in screen. The other preview screens stay behind the flag | Built from merged Account frames (check with the Designer first) |
| E2 | Offline-first behaviour: queue changes, show sync state, surface conflicts, never block | A test cuts the network mid-sync and the app keeps working |
| E1 + E2 | A cross-repository end-to-end test: sign in against the Engine's compose stack, change a setting on machine A, see it on machine B | It passes in the app's CI on affected paths |

**Engine order (E1):** (a) design note, `orbyn-engineering` crate and `docs/engineering.md`; (b) `orbyn-device-client`
with a fake-server test; (c) the device `app` label (a hint only, never proof), Engine issue #73 and tags `engine-v0.1.0` and `engine-v0.2.0`; (d) a desktop-client run
against the compose stack with invite-only config; (e) the cross-repository end-to-end test with Engineering 2.
Sign-up needs an invite, an 8-character email code and proof of work. Staging is not deployed yet, which is the owner's
step.

**Designer check (E2 asks directly):** do frames exist for sign-in states (invite required, wrong password,
two-factor, new-device approval), the sign-up steps (invite, 8-character email code, proof of work as a visible wait),
the sync status line (syncing, offline, conflict, paused), the device list with its app label, an honest note that
every trusted device can open all your Orbyn data, sign-out, and the signed-out state? Anything missing is drawn before
it is built.

**Not in Phase 1:** recovery, sharing, connected-app grants, Advanced protection, remote access and pairing (excluded
scope).

### Order of work

| Phase | What | Owner | Gate to start |
| --- | --- | --- | --- |
| 0 | Prepare: finish the perf pull requests (#413 to #416 are merged; finish what is open), read this plan, confirm D1 to D9 | E2, E1 | None |
| 1 | Wire the Engine and Engineering (above) | E1 + E2 | Phase 0 |
| 2 | Crew core, headless: slice 0 (protocol schemas), slices 1 to 4, review and Autopilot backends, ledger. Fake-engine tests only, no new screens, so it can run beside Phase 1 if the app owner splits work into worktrees | E2 | None |
| 3 | Shell redesign (slice 4e) | E2 + Designer | Engineer Designs v80 or later merged (done) |
| 4 | Crew UI (slice 5), review stages (4b) and Autopilot (4f) | E2 + Designer | Crew Designs v8 merged (done). Phase 2 backends in place |
| 5 | Parity rows ([Appendix A](crew/beta-rows.md#appendix-a-every-unfinished-beta-row-app-parity)), unbuilt screens ([Appendix B](crew/beta-rows.md#appendix-b-designed-screens-not-yet-built)), rows with no design ([Appendix C](crew/beta-rows.md#appendix-c-unfinished-rows-that-have-no-design-yet)), Warp features W1 to W12 | E2 + Designer | Each row's frames merged. Part C, D, E and F designs are still being drawn |
| 6 | Hardening: packaged-app checks on macOS and Windows, docs, live evals with the owner's budget, release notes, the Crew page on orbyn.dev | E2, E3, the owner | Phases 1 to 5 |

Release means the [beta cut](#beta-cut) is done. Anything not in the cut keeps its "After the beta" label.

**The owner's list:** keep Docker running on the owner's Mac for the local Engine (D11), then the staging deploy and
Resend DNS, a run budget for live evals, the audit sink account (before open sign-up, not for the beta), rebuilding the
packaged app, publishing the Crew Protocol, and reading each merged pull request.

**Designs.** The design source is the private `orbyn-designs` repository, read from a current local clone after
`git pull`, never from published links or a copy. This file names its files and frame ids and links to none of it:

- Crew: `artifacts/crew-designs.html`
- Engineering shell: `artifacts/engineer-designs.html`
- Shared components: `artifacts/components.html`
- Crew and shell motion: `artifacts/crew-motion.html` (24 demos with specs, built from the four motion tokens)
- Frame-by-frame match brief: `briefs/engineering-design-match/`

Frame ids in this plan (for example `crew-*`, `flow-*`, `changes-*`) are the ids in those files.

## What exists today

In this repository at `aa909eee`:

| Already there | Where |
| --- | --- |
| Drivers for Claude (native), Codex (app-server), ACP (OpenCode, Cursor, Kimi, Copilot and others), AG-UI and terminal, all normalised to `EngineEvent` | `crates/orbyn-core/src/agents/driver/` |
| Orbyn MCP served to every engine, including ACP agents through a stdio bridge | `tools/mcp/`, `agents/acp_mcp_bridge.rs` |
| `orbyn_delegate_task` with an `engine` field: one engine can already start a thread on another | `conversation/automations.rs:742`, `orbyn-contracts/src/automations.rs:194` |
| `orbyn_thread_send`, `orbyn_thread_wait`, `orbyn_thread_read`, `orbyn_orchestrator_capabilities`, `orbyn_worktree_handoff` | `tools/mcp/tools/definitions.rs` |
| Steering text into a running turn, for engines that accept it | `agents/driver/mod.rs:250` (`RunHandle.input`) |
| Engine health, login state, versions and usage per engine | `agents/engines/`, `agents/usage.rs` |
| A fake engine for deterministic tests | `crates/orbynd/src/bin/orbyn_fake_engine/` |

What is missing:

- Nothing picks the engine automatically.
- A delegated thread reports the tail of its transcript (`child_report`), not a typed result.
- No code checks the result.
- Peer messages have no hop, depth or budget guard.

## What is new

Prior-art survey, 2026-11-11, from primary sources where possible.

Already common: several harnesses side by side, a worktree per agent, a diff and merge view, and the person picking the
harness. Examples: GitHub Agent HQ, Conductor, Warp Oz, Vibe Kanban (shut down April 2026), Zed with ACP, Claude Code
agent teams (live messaging, Claude only), and Codex and Cursor subagents (they report to their parent only).

Protocols:

- MCP (2026-07-28) covers tool access.
- A2A 1.0.1 covers peer task delegation.
- ACP covers editor to agent.
- in-toto covers provenance.
- None defines leases, claims with evidence, shared facts, verification results or approval gates between agents. The
  closest is MCP Agent Mail, a community project with advisory file reservations; its repository is not verified.

Not found in any product or spec we surveyed. "Not found" is not proof, so we make no "first" claim publicly until we
have checked again:

1. **An open crew protocol.** Any harness joins a crew through MCP alone, with no Orbyn adapter. That means unlimited
   harnesses, including one running in your own terminal or a remote agent through A2A.
2. **Proof-carrying work.**
   - Reports are typed claims, each bound to a check. Prose that no check backs shows as "unverified".
   - The pull request carries a signed attestation of what was checked, who reviewed it (which vendors), and which
     findings were confirmed and fixed.
3. **Contracts become checks.**
   - A worker publishes a contract fact (an API shape, a schema, a CLI flag).
   - Every consumer automatically gets a check for it.
   - A changed contract alerts its dependents before they build on the old one.
4. **Disagreement is an object.**
   - When cross-vendor reviewers disagree, both positions and their evidence become one item to resolve.
   - A tie-break check or the person resolves it, and the disagreement rate feeds the router.
5. **Counterfactual fork.**
   - Re-run any stage from the ledger on another engine and compare the verified outcomes side by side.
   - Results train the router per repository, locally and privately, from verified outcomes rather than opinions.
6. **Negotiated leases, enforced.**
   - Workers request, receive and transfer file leases over the protocol.
   - A pre-commit guard and the worktree write filter enforce them.
   - Lease conflicts go to the lead.
7. **An attention budget.**
   - The crew batches questions to the person and ranks them by risk and reversibility.
   - The Needs you inbox says what waiting costs.

**Reference projects** (what each does, and what we take):

| | openbot (PolyForm NC, ideas only) | harnessrouter (Apache-2.0) | dsh-superagent (MIT) |
| --- | --- | --- | --- |
| Harnesses | Claude SDK, Codex app-server, ACP for the rest | 31 CLIs; Claude stream-json is the common event | DeepSeek Harness only |
| Who picks the harness | The person | The API client | One harness, so no choice |
| Agent to agent | Mailbox, free-text messages, delivered as the next queued turn | None | Workers report to the main session in a typed form; no peer talk |
| Verification | Whatever the agent says | Protocol conformance only | Typed Reports (`ready`, `blocked` and so on); a person reviews |
| Isolation | Folder per agent | Container per session | Directory per ticket, `git worktree` through the prepare command |
| Worth taking | Mailbox ids, reply-to, `expectsReply` | A normaliser per engine; `incomplete` when the budget runs out; a per-session uid | Closed report states, Attention for questions to a person, hand-back, a 5-second no-wait guard on the lead |

None of them has automatic routing, live mid-task messaging between different harnesses, or a code verifier. That is
the gap Crew fills. openbot is non-commercial: no code or text from it, only the ideas listed here.

## Design

**Roles**

- **Lead**: an ordinary thread on whatever engine the person picks. It gets the Crew tools. It writes briefs and
  reviews results. It does no routing maths.
- **Router** (Rust, no model): picks the engine for each brief. The decision and its reasons are stored and shown.
- **Workers**: threads on the chosen engines, each in its own worktree. Depth is 1: workers cannot start workers.
- **Crew bus** (Rust): owns runs, messages and the shared board.
- **Verifier** (Rust): runs the answer-key checks in the worker's worktree.

**Models and effort per role.** These come from the repository's harness (`harness.py resolve`: library defaults, then
the owner's file, then the repository's `.orbyn/harness.json`). Effort is medium for every role and never higher.

| Role | Claude | Codex | Notes |
| --- | --- | --- | --- |
| Lead (briefs, approves in Autopilot, merges) | Opus | Sol | The engine the person picks for the lead thread |
| Router, verifier, merge queue, watch on main | No model | No model | Code |
| Builder | Sonnet | Terra | Complex work on Codex goes to Sol (harness `engineer_complex`) |
| Code review | Opus | Sol | Lead tier, on the other vendor from the builder by default |
| Security review and finding check | Opus | Sol | Lead tier always; never lower |
| Second key (Autopilot) | Opus | Sol | Must be the other vendor from the lead |
| Scout (search, mechanical checks, cheap drafts) | Haiku | Luna | Escalates to the builder tier on failure |
| OpenCode and other ACP engines | The person's model setting | The person's model setting | The person maps each to lead, builder or scout tier in Settings, Crew; unmapped engines are offered only as builders |

The route card shows the tier, model and effort chosen, and why. A repository's harness file can only lower cost, never
raise a tier. Only the person can change a lead tier.

**Router v0**

1. Hard filters: installed, logged in, compatibility not `Broken`, and `managed` plus `approvals` for any brief that
   writes. The brief's `needs` (for example `image`, `mcp`) must match capabilities.
   - **One source for the state facts:** the first three checks and the write check read `ProviderFacts`
     (`agents/provider_facts.rs`), the same facts the Settings Providers list reads for its Not found, Can't start
     (a `Broken` version) and Unmanaged (no `managed` or no `approvals`) labels, and for the Signed out state.
     A test runs every combination of those facts through the router and the list and fails if they disagree.
     A sign-in that could not be confirmed has no designed label, so the list shows none for it while the router
     still refuses. Limited support (an `Unsupported` or `Graceful` version) is list-only and never keeps an engine out.
2. Score: pass rate for the brief's `kind` from Eval history (seeded defaults until there is data), quota headroom from
   `usage`, then the person's default engine to break ties.
   - **As built (slice 2, differs from the plan):** a full quota window is also a hard filter (`quota_exhausted`), not
     only a score input. The score is a weighted sum, 70% pass rate and 30% headroom, in integer permille, rather than
     a strict order of pass rate, then headroom. The default engine only breaks a tie. The seeded pass rate is a flat
     500 for every engine until Eval history exists. Limits are read from the usage cache only, so an engine with no
     cached report counts as not reported (neutral headroom, no quota filter).
3. A person's or lead's explicit `engine` wins but is still checked against the filters.
   - **Tagging a harness in the lead thread assigns it by hand.**
     - `@claude`, `@codex`, `@opencode`, or any installed engine's id (`@cursor`, `@kimi` and so on): the router picks
       the tier.
     - `@claude/sonnet` or `@codex/sol`: a specific model.
     - `@codex review`, `@claude security`: the tag goes to a role or reviewer lane. With no role it means builder.
     - Several tags split the work, for example "@claude build the API, @codex the CLI".
   - **Rules for tags:**
     - A tag still passes the hard filters (installed, signed in, compatible).
     - If it fails, the lead says why ("Codex isn't signed in") and never quietly swaps engines.
     - Effort stays medium.
     - A tag cannot move security review below the lead tier.
     - In Autopilot, a tag still follows the cross-vendor rule for reviewers. Tagging the author's own engine as
       reviewer is allowed but badged "Same engine as author".
   - **Typing it:** the composer suggests engines first, then the engines' own subagents (which already use mentions),
     grouped so the two never collide.
   - **Shown as:** the route card reads "Assigned by you (@codex)", and the Flow and Taskboard show the tag.
4. Output: `{engine, model, effort, reasons[], rejected[{engine, why}]}`.

`kind` is a closed set: `plan`, `implement`, `review`, `test`, `research`, `ui`, `docs`.

**Brief** (the worker sees it): `{kind, outcome, inputs, files (lease), allowed_effects, stop_when, budget {turns,
minutes}, engine?}`

**Answer key** (only the verifier sees it): `{checks: [{run: "cargo test -p x", expect: "exit 0"}, {paths_only:
[...]}, {schema: ...}]}`. The files the checks run must be on the worker's protected-path list.

**Result** (back to the lead): `{state: passed|failed|stopped|cancelled|needs_person, summary, branch, commit,
checks[], attempts, usage, route}`

Workers report through a `crew_report` tool. The verifier runs after every report. If a check fails, the worker
retries with the reason. The same failure twice means one change of approach, and then the run stops.

- **As built (slice 3, differs from the plan):**
  - The failure fingerprint is the failing checks, how each ended and the names of what failed; the output is
    left out, since it changes between runs. The checks run after the worker's turn has ended. A different fingerprint starts the count again, so a worker that makes progress is not stopped.
  - A turn that ends with no `crew_report` is a failed attempt with its own fingerprint. An engine that fails ends the
    task (`engine_error`). `budget.minutes` ends a task even mid-turn; `budget.turns` is not enforced yet.
  - The worker runs in the lead's checkout through `delegate`. A worktree of its own is not created yet.
  - Check kinds this version does not run (`schema`, `http_probe`, `preview`, `visual_compare`, `repo_required`,
    `manual`, `screenshot_only`, `accessibility_audit`, `performance_budget`, `secret_scan`, `licence_scan`) report
    `skipped` and are listed as not verified. A key with nothing it can run goes to `needs_person`.

**Harness-to-harness communication** (the new part)

| Tool | Does |
| --- | --- |
| `crew_post {to, kind: fact\|question\|handoff\|blocker, body, refs}` | Sends to a peer, the lead or `all` |
| `crew_ask {to, question, wait <= 40 s}` | Blocking question. If there is no answer in time it returns `pending`, and the answer arrives later as a post |
| `crew_board_read/write {key, value, evidence}` | Shared typed facts, such as an API shape one worker decided and another needs. A write records its author; a conflicting write goes to the lead |

Delivery:

- Claude and Codex get messages mid-turn through steering (`RunHandle.input`).
- ACP engines without steering get them at the start of their next turn.
- The UI shows which delivery each message used.

Guards, all enforced in code:

- At most 3 exchanges between two workers before the lead is told.
- 30 posts per worker per run.
- Messages are wrapped as data, never instructions.
- No secrets or permissions travel in messages.
- `crew_*` tools refuse after the run ends.

- **As built (slice 4, differs from the plan):**
  - The tools are `crew_post`, `crew_ask`, `crew_board_read` and `crew_board_write`, behind the Preview flag. The
    core stamps author, id and run; a draft that names one is refused. Facts and messages pass the secret scan and
    the answer-key check in and out. A message to the lead is not checked against the key.
  - The bus itself never starts a turn or writes a thread's queue. Text is handed only to a running turn that takes
    input (`live`, shown under the role `crew`); otherwise it is `held` and comes in the `inbox` of the recipient's
    next bus tool result, or inside a prompt the core already sends: the worker's retry prompt and the lead's result
    notice. Those prompts are admitted with the source label `user` (slice 3's channel), so held text rides in a
    prompt the queue shows as the person's, inside the data wrapper, at most 10 messages and 20,000 characters at a
    time. A worker being verified is never handed anything. A message nobody ever collects stays `held`. Only fake
    engines ran, so "live" is not tested against real Claude or Codex, which may read it after the tool call it is
    in. A board write counts as a post, and a key has 16 revisions.
  - The hop count is H4 as written: a lead message to a member starts every pair of that member again. The count
    is read from the stored messages. The lead is told once per distinct refusal by a message from the core.
  - Sizes: a body up to 4000 characters, a fact value up to 8 KiB, 128 keys per board. A repo rule can lower the
    exchange and post limits through the bus, but no rule file is read yet.

**Isolation**

- A writing worker gets its own worktree through the existing worktree handoff.
- The brief's file lease prevents two writers on the same files.
- The lead merges in order.
- Read-only workers share the lead's checkout.

**Run states:** Briefed, Routed, Running, Verifying, Passed or Failed, ChangedApproach, Stopped. Plus Cancelled and
NeedsPerson. This is the statechart of the earlier lead-and-worker sketch with Routed added.

## Review stages

Reviews are their own stages in the flow, never something hidden inside the author's engine. This follows the
workflow's "review before the PR" rule and orbyn-security-audit's independent validation, made visible.

**Stages:** Brief, Route, Build, Checks, Code review, Security review, Finding check, PR review, You (approve and
merge).

**Rules for every review stage:**

- **Its own run.**
  - A fresh thread with its own route card and a read-only checkout of the exact commit.
  - It gets the diff, the brief's purpose and the check results. It never gets the author's transcript or the author's
    verdict.
  - It reads and reports. It never edits, posts or merges.
- **Different eyes by default.**
  - The router prefers a different engine or vendor from the author: a Codex review of Claude's work, and the reverse.
  - Security review and finding checks run at the lead tier (the harness's `security` and `security_verifier` routes),
    never on a cheaper tier.
- **No limit on reviewers.** A stage holds any number of reviewer lanes running side by side, for example:
  - Correctness
  - Security
  - Performance
  - Accessibility
  - Design match against orbyn-designs frames
  - Licence and attribution
  - Docs and copy
  - Any custom reviewer the person names

  A lane can be added at any time, including after a pass. Repository rules add lanes automatically: any change
  touching auth or keys always gets Security.
- **Findings are typed:** `{severity, path:line, claim, evidence, repro}`.
- **A finding blocks only after a separate check confirms it.** That check is a code repro or an independent second
  run. Unconfirmed findings show as "unconfirmed".
- **Confirmed findings go back to Build.** The verdict travels as a fix brief. The fix then goes through Checks and the
  same reviewers again. Old findings show as fixed, still open or regressed.
- **PR review.** After the pull request opens, the same lanes can review it on GitHub or the forge. Their comments link
  back to the Crew task. Bots and agents never merge in the skills library.
- **The person reviews after the merge, not before it** (see [Autopilot](#autopilot)). The merge queue merges once the
  required stages are green and the risk tier allows it. The person's stage is the after-merge Review queue, still
  always visible. Full Autopilot is the default, so high-risk changes also merge and go to the top of the person's
  queue. They can switch to On (high risk waits for them) or Off per repository or crew.

**As built (slice 4b):**

- The tools are `crew_review` (lanes, no cap, also after a pass), `crew_finding` (a reviewer files a finding),
  `crew_check_finding` and `crew_review_status`, behind the Preview flag. See the
  [review README](../crates/orbyn-core/src/crew/review/README.md).
- A finding is confirmed by code: a repro check the lead supplies that fails, or an independent second review on the
  other vendor that reports the same file within 3 lines. Otherwise it is unconfirmed and blocks nothing. Both
  decisions are Gates at `finding_check` with Evidence the core wrote.
- A confirmed finding sends a fix brief to a builder (its key includes a check that the file changed), the lanes run again on
  the fix, and a finding shows as fixed only after such a fix passed and a repro has proved it: a finding confirmed by agreement has no repro, so it keeps
  blocking until the lead supplies one that passes after the fix (or a person clears it). A reviewer's silence clears nothing. One fix per lane and file,
  at most three per task. Reviewers are read-only by bytes, and the second review needs another vendor than the first.
- Not yet: PR review on the forge, the same-engine badge, a per-lane `review` Gate, locked-lane removal by a person.
  `crew_finding` refuses after the reviewer's run has ended (H7); the Gates are core writes and do not.

**Review decisions** (2026-11-11, Designer part 2, frames in `crew-designs.html`):

- **Required before merge by default:**
  - Checks, Code review, Security review, Finding check, and CI when the repository has CI.
  - Security is skipped automatically only for docs-only diffs.
  - PR review is required only by a rule.
- **More verification is always allowed.** A rule can require a stage or forbid skipping it, but never forbid running
  more ("Run it anyway" stays on every skipped stage).
- **Removing a required lane:**
  - Only a person can, with a recorded reason, and only if the rule does not mark it `locked`.
  - Security lanes added by security rules are locked by default.
  - Agents never remove lanes.
- **Rules:**
  - `.orbyn/crew.json` in the repository (`required`, `lanes` with `when` and `locked`, `checks` with `run` and
    `when`).
  - Repository rules can only add or tighten.
  - Personal defaults live in Settings.
- **Replay** keeps the full hash-chained event log per task. Large artifacts can be pruned after 30 days with their
  hash kept.
- **Same-engine review** happens only after asking the person, and is badged "Same engine as author" in the lane and in
  the PR proof.

## Autopilot

**Default mode: Full** (owner, 2026-11-11). The person can switch to On or Off per repository or crew, and the choice
is theirs. The hard-denied list applies in every mode.

The person's job becomes reviewing what reached main, not answering agents. Agents' requests go to the lead, not to the
person. The limits on what the lead may approve are enforced in code, because classifier-based approval can be fooled
by prompt injection. OpenAI says this about its own Auto-review, which approves about 99% of requests and is still "not
a guaranteed boundary"; a Cloud Security Alliance note shows injection beating Claude Code's auto mode.

**Approval chain for every agent request.** This extends today's single gate in `governance/approvals.rs` ("policy
first, then a person").

1. **Hard rules (code):**
   - Always denied, even in Autopilot:
     - Reading or sending secrets and credentials
     - Network to hosts the repository has not allowed
     - Writing or deleting outside the worktree
     - Force-push or history rewrite on main
     - Changing branch protection, permissions or settings
     - Changing account, device or sign-in state (added by decision D12; account routes must be refusable by this rule, see [account.md](account.md#sensitive-account-actions))
     - Editing answer keys, check files or `.orbyn/crew.json`
     - Production deploys
     - Publishing or messaging outside the repository
     - Payments
   - Always allowed: reads and writes inside the worker's leased paths and running the brief's checks.
2. **The lead decides** everything in between. It sees a structured summary of the action (kind, target, paths, the
   brief it serves), never raw page or tool text, and it must give a reason.
3. **Two keys for medium risk:** network, installing dependencies, running generated scripts, or touching CI files. An
   approver on a different vendor must also say yes. If they disagree, the action is denied, and the disagreement is
   logged.
4. **Person-only actions in Autopilot are denied, and work continues.** The Needs you item does not block anything; it
   waits for the person's review. **No agent approves; a person may approve once.** A command the chain denied is held for
   the person, who may approve that exact command once, deny it or dismiss it (below). Nothing on the always-denied list is
   ever held, and no approval covers it.
5. **Trip wires:** 3 denials in a row or 20 in a run stop the task with reason "too many denials". Text that looks like
   injected instructions stops it too.

Every approval and denial is in the ledger with its reason, and the Flow shows it.

**Risk tiers decide what merges by itself.** These follow Meta's RADAR approach of a diff risk score plus a revert-rate
lookback.

| Tier | Example | Merge |
| --- | --- | --- |
| Low | Docs, tests, small changes in well-covered code | Merges by itself |
| Medium | Normal feature work | Merges by itself, and must be reviewed after the merge within the person's review window (default 24 h) |
| High | Auth, keys, crypto, migrations, CI, dependency changes, large diffs, areas or engines with a high revert rate | Under Full (the default), merges and goes to the top of the person's queue. Under On, waits for the person before merging |

The risk score comes from paths, size, coverage of the touched code, and the 60-day revert rate of that area and that
engine. It is shown with its reasons.

**Merge queue for agents.** Nobody ships this yet.

- Parallel branches are merged in order. Each candidate is rebased on the queue and its required checks run again on the
  combined state before main moves.
- Conflicts are forecast from the dependency graph before work starts, and the router orders tasks to avoid them.

**After-merge Review queue (the person's stage):**

- Every merge to main is one item, ranked by risk. Each item has the diff, the proof (checks, reviewers by vendor,
  findings fixed), what was not verified, and the cost.
- Actions: "Looks good", "Comment" (becomes a fix brief), "Revert" (the crew opens and verifies a revert).
- **Watch on main:** if main's CI or a post-merge check breaks, the crew finds the merge that caused it by bisecting the
  queue's merges. It reverts automatically when the cause is certain, and otherwise asks the person.
- **Daily digest:** what merged, what is waiting for review, what was reverted.

**As built (slice 4f, part 1):** the always-denied list N1 to N10 (including the account routes) in code, the approval
chain with a lead and a second key on another vendor, trip wires, the ledger and the mode (`off`, `on`, `full` by
default), all behind the Preview flag and with no screen. The lead and second key are an `Approver` trait, not yet backed
by a live engine. See the [Autopilot group](../crates/orbyn-core/src/crew/autopilot/README.md).

**As built (slice 4f, part 2):** diff risk tiers, the merge queue (one merge at a time, checks again on the combined state), the
after-merge Review queue and the watch on main with a bisect revert. Local only: a merge or a revert moves a local branch that
no one has checked out, and nothing is pushed or sent to a forge. Merges are `--no-ff` merge commits rather than rebases, so
a revert is one command and a bisect walks first parents.

**As built (slice 4f, HTTP):** `orbynd` serves Autopilot's views and the person's few answers under `/api/crew`, all behind the Preview
flag. With it off every route answers 404 for every method, like a path nothing serves, and `/api/info` omits `crew_preview`. The flag is
read, never set, over HTTP. **The Preview flag stays for development only until [#441](https://github.com/orbyn-dev/orbyn-engineering/issues/441)
is fixed:** a worker runs as the person and can read the service token and the data folder, so it could reach these routes.
Reads: `GET /api/crew/preview`, `/autopilot` (modes, `confirmed`), `/autopilot/guards`, `/autopilot/risk/{task}`, `/autopilot/review`,
`/autopilot/review/{task}/revert-check` (later merges), `/autopilot/merge-queue` (with what goes next), `/autopilot/watch`,
`/autopilot/digest?hours=`, `/approvals` (the ledger, newest first), `/stops` (trip wires) and `/trust` (revert rates by area and engine).
Lists take `limit` (1 to 200) and `offset`. Changes, each through the core function that holds the rule and local only:
`PUT /autopilot/mode`, `POST /autopilot/confirm`, and `POST /autopilot/review/{task}/{looks-good,comment,revert}`.

What a change may and may not do: no route takes a command from the client or runs one, and no route pushes or merges. A revert runs
only the repository's own checks (`.orbyn/crew.json` at the commit before the merge) in a scratch checkout under the data folder, always
removed afterwards (also when the task panics), then moves the local branch by compare-and-swap. It is refused when a later merge needs
the one reverted, when the branch is checked out anywhere (a dirty checkout included) and when the branch no longer holds the merge.
The merge's repository and branch come from the stored merge, and the request names the whole merge commit it was shown (a prefix is
refused). Answers are taken one at a time and the merge is read inside that lock, so two reverts act once, and a merge that was
reverted stays reverted. Every free-text field and stored JSON value is redacted and has the task's key withheld on the way out.

The marker `crew::changes` lists the Crew routes that change state, as `account::changes` does for the account, and N1 refuses an agent's
request to any of them (or to any path under `/api/crew` that is not a listed read) in every mode. The route-table test checks the list
against the router. N1 reads a command word by word with the quotes off, and undoes `//`, `/./` and `/x/../` first; it does not see a path built from a variable, string pieces or a script the agent wrote, which is the sandbox's work (#441). The person's answers to a held command are in the next paragraph.

**As built (slice 4f, approve once):** a command the chain denied (the lead said no, or a second key is missing) is also *held*, in memory only
(a restart drops it, and the raw command, which may hold a secret, is never written to disk). `GET /api/crew/approvals/held` lists the holds
with the redacted command (the answer key's text withheld; a command too long to read whole cannot be approved). The person answers from the
app's window, local only: `POST /api/crew/approvals/{hold}/approve-once`, `/deny` and `/dismiss`. **No agent approves; a person may approve once.**

- **One command, once.** An approval is bound to a hash of the exact command text, its arguments and paths, the worker's thread and the task. It
  covers one retry of that same request from that same thread, within 10 minutes of approving (a hold waits 10 minutes for the person), and is
  used up by that retry. Another command, other arguments, another thread, a second retry or a late one gets nothing. A hold is decided once: a
  racing approve and deny have exactly one winner. A hold that expired, or whose task ended, answers 410 (`gone`); one already decided answers 409.
- **Never the always-denied list.** A command on the list (N1 to N10) is never held. The list is read again when the person approves and again when the
  worker retries, so a change in between (the checkout moved to main, say) turns the approval into a refusal. A person's deny rule still wins at the gate.
- **Proof that a person is there: local presence (Approving once: local presence).** Approving needs a grant the desktop app records after the person
  clicks Approve in the app's own native dialog; the account is not involved, so approve-once never answers `account_disabled`. The desktop shell launches
  the service with `--parent-stdin` and a piped stdin only the shell holds (an engine gets its own piped stdin from the service, never the shell's). The flow, over newline JSON on that channel:
  1. The shell sends `presence_describe {request_id, hold_id}`. The service answers `presence_hold {request_id, hold_id, thread_id, task_id,
     command_shown, command_sha256, worker_name, task_title}` or `presence_refused {request_id, code}`. The command, the worker's name and the task's title
     come from the service, redacted (the answer key's text withheld) and bounded, never from the web page. `command_sha256` is the hold's binding hash.
  2. The shell shows the dialog. When the person clicks Approve it mints a nonce (16 random bytes, hex) and sends `presence_grant {request_id, nonce,
     hold_id, thread_id, task_id, command_sha256}`. The service records one grant for the hold in memory (`PresenceBook`), only if the hold is open now
     and every bound field matches it as it stands now, and answers `presence_ready {request_id}`. The shell hands the nonce to the page only after that.
     A later grant for the same hold replaces the earlier one. A grant lives 60 seconds, is used by one approval and is cleared by a restart.
  3. The page posts `POST /api/crew/approvals/{hold}/approve-once` with the nonce in `x-orbyn-confirm`. The service checks the grant first, without
     using it (constant-time compare), then the hold is open, its task is running, the command is readable and the always-denied list again; only then
     does it use the grant up and decide. A refusal at any step leaves the grant as it was; a wrong nonce does not burn it. The ledger row is
     `approved_once` with `via: "desktop_presence"` and `decided_by: "person"`.
  The strings in `presence_hold` are made for a plain-text dialog: control, invisible, bidirectional and format characters become a visible `�`,
  a newline becomes a visible `⏎`, the command is capped at 1000 characters with `truncated` set if it was cut (the hash is of the full command), and
  a hold that cannot be read whole is refused with `unreadable`. The shell's dialog renders all of them as plain text (no markup, no HTML, no
  link detection) and shows them as given; it also refuses an answer that still holds a control character.
  Refusal codes on the shell channel: `unavailable`, `invalid`, `not_found`, `gone`, `unreadable`, `mismatch`.
  **Errors on the route:** a service started without `--parent-stdin`, or a remote one, answers `403 desktop_required`; with a desktop shell, a missing,
  wrong, used, expired or mismatched nonce is `403 presence_required`. `Origin: tauri://localhost` is not trusted for any of this.
  **What it protects.** No caller that only speaks HTTP to the service can approve once, with or without the service token. Deny and dismiss need no grant
  because they only refuse (their ledger rows say `decided_by: local_caller`, not `person`), so a token holder can deny or dismiss a hold, never approve one.
  **What a process of the same OS user can still do, so this is not OS-level presence:**
  - drive the desktop app's native dialog through Accessibility or SendInput, including a lead that is not confined;
  - read the shell's memory (on macOS the hardened runtime blocks `task_for_pid`, but signing is not checked here; on Windows `PROCESS_VM_READ`);
  - write into the shell's pipe to the service (on Linux `/proc/<pid>/fd/0`, on Windows `DuplicateHandle`).
  The devtools feature is on in release builds (`desktop/src-tauri/Cargo.toml:15`): a script in the page can call `invoke` but cannot click the native
  dialog. That is a known exposure, tracked in [issue #532](https://github.com/orbyn-dev/orbyn-engineering/issues/532).
  **Idea, not built (Linux only):** a per-launch HMAC key between the shell and the service, so a line written into the pipe by another process fails.
  **As built (PR2, the shell's dialog):** the command `crew_presence(hold_id)` in `desktop/src-tauri/src/presence.rs`. The bundled app's main window
  alone may call it (the capability is the `main` window's, and the command checks the window's label and address as well). It checks the hold id, sends
  `presence_describe` and waits up to 10 seconds for `presence_hold` (matched by request id), shows the native dialog from the service's text, and
  only on a click on Approve once mints the nonce (16 bytes from the operating system, 32 lowercase hex characters), sends `presence_grant` with the ids
  and hash exactly as the service described them, waits up to 10 seconds for `presence_ready` and returns the nonce. The dialog's title is "Approve this
  command once?", its body is the command (then an ellipsis if it was cut), "Worker: <name>", "Task: <title>" and "This approves this one command. It
  cannot be reused."; its buttons are "Approve once" and "Cancel". One approval is open at a time (a second call is `busy`). The nonce is never logged,
  never written to disk and never in an event; it reaches the page only as the command's return value. The buffers the shell owns are zeroed when dropped, which is
  best effort. Errors are stable strings: `cancelled` (nothing was sent), `busy`, `timeout`, `unavailable` (no service), `invalid_hold`,
  `invalid_answer`, `forbidden`, `dialog_failed`, `nonce_failed` and `presence_refused:<code>` for the service's refusal codes above. The page words
  them; for a refusal or a timeout the Designer's text is "Orbyn couldn't confirm it was you. Try again from the desktop app."
  **Which button is which, and what Return does.** Approve once is the dialog plugin's OK slot and Cancel its cancel slot. **Return in the native dialog
  APPROVES on macOS and Windows**, because Approve once is the first button and the default one, and the plugin has no default-button setting. Escape,
  the close box, Alt+F4, a failed dialog call and a GTK delete-event all come back as the Cancel label; only a click or Return on the Approve slot equals
  the Approve label, and the code refuses to build with the two labels equal. What each platform returns (`dismissals_never_read_as_approval` encodes this table):

  | Platform | Click Approve once | Click Cancel | Escape | Close box / Alt+F4 | Failed call | Return (default) |
  | --- | --- | --- | --- | --- | --- | --- |
  | macOS | approves | Cancel | Cancel | Cancel | Cancel | **approves** |
  | Windows | approves | Cancel | Cancel | Cancel | Cancel | **approves** |
  | Linux (GTK) | approves | Cancel | Cancel | Cancel | Cancel | not checked |

  Why this is acceptable: the person opened the dialog by clicking Approve once in the page, the dialog shows the exact command, and Return approving
  adds no capability beyond what a same-user process that can click the dialog already has (stated above). The dialog may only open as the direct result of the
  person's own click; the shell cannot tell a click from a script that calls `crew_presence` (devtools is on in release builds, issue #532), so PR3's card
  must call it only from a trusted click (`event.isTrusted`, no keydown, no automatic retry) and wait for the Enter key to be released first. In the shell,
  `crew_presence` is the only way a dialog opens; nothing retries, reopens after a refusal or queues a second one (`approve_never_reopens_the_dialog`).
  If the plugin gains a default-button setting, we revisit and make Cancel the default.
  **Manual check (the dialog cannot run in CI), on macOS, Windows and Linux (GTK):** with a held command, click Approve once in the page, then:
  1. Escape: the call answers `cancelled`; no grant is sent.
  2. Close box, and Alt+F4 on Windows and Linux: `cancelled`.
  3. Click Cancel: `cancelled`.
  4. Return: approves (expected on macOS and Windows; note what Linux does).
  5. Click Approve once: the nonce comes back and the service answers `presence_ready`.
  6. Linux: a command such as `date +%s; echo 100%` is shown exactly, with the single `%` signs.
  7. Leave it open and call again: `busy`. Minimise the window and call again: the window comes forward.
  The main window is shown, un-minimised and focused first (up to half a second for macOS's animation); if it is still not visible the answer is `unavailable`.
  The dialog is parented to it on macOS and Windows. On Linux rfd's GTK 3 backend ignores a parent, so the dialog is unparented and may open behind
  the main window. A native dialog cannot be closed from the shell, so the person has 10 minutes (a hold waits that long); after that the call ends with
  `timeout` and a late click is thrown away, but the dialog stays up and a new call answers `busy` until it is dismissed.
  **Text refused** (`invalid_answer`) in the command, worker name or task title: control characters; U+00AD, U+034F, U+061C, U+115F-1160, U+17B4-17B5,
  U+180B-180F, U+200B-200F, U+2028-202E, U+2060-206F, U+3164, U+FE00-FE0F, U+FEFF, U+FFA0, U+FFF9-FFFC, U+1BCA0-1BCA3, U+1D173-1D17A, U+E0000-E0FFF (the service's set, copied; the service now maps U+0600-0605, U+06DD, U+070F, U+08E2 and U+13430-1343F too).
  **Linux text.** rfd's GTK 3 backend reads the dialog text as a printf format, so on every platform but macOS and Windows the shell doubles each `%`
  in the text it shows (`double_percent`); on macOS and Windows the text is plain and is shown as given.
  The dialog cannot be run in CI, so its look, focus and the plain-text rendering by each platform's dialog (macOS, Windows, GTK) are checked by hand.
  **As built (PR3, the web card):** the held card's Approve once (Crew Designs v34: approval-confirm, -browser, -refused-presence, -decided, -expired and
  guard-approve-myself) calls `crew_presence(hold_id)` through one typed function, `askPresence` in `web/src/api/shell.ts`, which reads the window's Tauri
  bridge (`__TAURI_INTERNALS__`) and calls that one command by name. The page imports no Tauri package. The call is made only from a click handler that
  checks `isTrusted` and `type === "click"`; no key handler, effect, page load, retry or replay calls it, and the button is off while a dialog is open
  (the shell also answers `busy`). A script's `dispatchEvent` or `click()` is untrusted and does nothing. **A click made by pressing Enter or Space on the
  focused button is trusted** (Chromium and WebKit both report it with `detail` 0, a mouse press with 1), and it is allowed: refusing it would shut out
  keyboard and screen reader users, so "never on a key event" holds for the card's key handlers, not for the button's own activation. When the
  shell returns the nonce, the page posts the approve-once route once with it in `x-orbyn-confirm` and shows the decision. A refusal from the shell or
  `403 presence_required` / `desktop_required` from the service shows "Orbyn couldn't confirm it was you. Try again from the desktop app." and the card stays
  held; `cancelled` shows nothing; nothing asks again by itself. A plain browser tab (no shell) does not offer Approve once and says "Approving once needs
  the Orbyn desktop app."; Deny and Dismiss still work. "Approve it myself" from a denied command or a Needs you item is the same flow, with no dialog of
  the page's own. The account password flow is gone from approve-once.
  **What the click check does not cover.** It covers the page's own code paths and synthetic events. It does not stop a hostile script running in the app's
  origin (devtools stay on in release builds, issue #532): that script can call `window.__TAURI_INTERNALS__.invoke("crew_presence", {holdId})` directly and
  open the native dialog. Because Return approves on macOS and Windows, a person pressing Enter elsewhere while such a dialog opens could approve. This is a
  known exposure. **Key repeat:** when the click came from the keyboard (`detail` 0), the page waits for the Enter key to be released (up to 2 seconds) before
  it asks the shell, so a held Enter's repeats are over before the dialog opens; mouse clicks are not delayed. "Approve it myself" is offered only for a
  row that is a hold, never for a ledger row. Refusals are announced to screen readers, and focus returns to Approve once after a cancel or refusal.
- **N1 and remote viewers.** The three routes are in the Crew marker, so an agent's request to one is refused by path in every mode (the lead is never asked),
  a remote viewer is refused, and with the Preview flag off the routes answer 404 for every method.
- **Ledger.** Every step writes a row: `held`, `approved_once`, `denied_by_person`, `dismissed_by_person`, `hold_used` (the retry went through; the
  `allowed` row says `decided_by: person`) and `hold_ended` (it expired or its task ended). A row carries the hold id, who decided, the redacted command
  and a `sha256:` hash of the exact request, never the raw command.

Known limits: an unauthenticated caller gets 401, not 404, on a Crew route even with the flag off, like every protected route. The
ledger rows hold the lead's and second key's reasons but not the asker's model, a risk tier or a title; an injected text is kept as its
class only. The risk, review and queue views read the whole table and then page, which is fine for one person's repositories. A
comment is kept and is not yet a fix brief.

Not built, so not served: per-area promotion and demotion (4j), trust scores for reviewers and the crew-versus-solo baseline (4g), the
conflict forecast, bisect progress, background crews' triggers and runs (4h; their stored state is below), and the digest's send time.

**As built (slice 4i, guard):** the blast-radius guard, backend only. A *destructive* command against a *production-linked* target always needs
a second key, even in Full, and the person can always approve it alone, once. It extends the always-denied list (N8 stays above it) and the
two-key rule. Code: `crates/orbyn-core/src/crew/autopilot/blast.rs` (reading), `chain.rs` (the `Guarded` tier), `targets.rs` (the lists and the
test), `exit.rs` (the exit-status watch).

- **Order.** Trip wire, always-denied list (N1 to N10, so N3 and N8 first), the person's one-time approval, then the guard, then the chain. A
  guarded command's tier is `guard` whatever else it would be: a lease or a check command does not exempt it.
- **What the chain does with it.** Under **On** or **Off** nobody is asked: the command is denied and **held for the person** (a `held` row, the
  worker goes on); there are no two keys under On or Off. Under **Full** the lead decides, then a second key from a *different vendor family*.
  No second vendor signed in: denied for now, held, `needs_second_engine` set. The two keys disagree: denied, with a `disagreement` row. Both agree:
  allowed, and the ledger row keeps both reasons. The lead is told the category, the target and what matched, never the raw page.
- **An approval is bound to the targets.** A guard hold keeps the targets it resolved (the host list, every git push URL by hash). When the person approves, and again when the worker retries, the command is resolved afresh; if the targets differ (`git remote set-url origin ...` in between) the approval is refused or does not apply (`The target changed since this command was held`).
- **Approve once.** The held command is approved exactly as in "approve once" above: the person's click in the desktop app's own dialog, one retry, 10
  minutes, the always-denied list checked again. The row is `approved_once` with `via: "desktop_presence"` and `decided_by: "person"`; the app draws it as
  "Approved by you" with the command (`action.target`), the target (`guard.target`), the time (`at`) and the exit status (the `command_exit` row).
- **Categories (built in; no repository file can add or remove one).** File deletion (`rm -r`, `find -delete` and `find -exec rm`, `rsync --delete|--del|--remove-source-files`, `shred`,
  `truncate`, also inside `ssh host '...'`); database (`psql`, `mysql`, `mariadb`, `mongosh`, `redis-cli`, `cqlsh` with DROP, TRUNCATE, DELETE, an
  UPDATE with no WHERE, FLUSHALL, dropDatabase, Mongo `deleteMany`, `updateMany({})` or `drop()`, in `-c`/`-e`/`--eval` or on the input); cloud (`aws s3 rm|rb`,
  `aws * delete-*|terminate-*`; any `aws ... sync` is refused outright by N8, stricter than the guard, `gcloud ... delete`, `gcloud storage rm`, `az ... delete` and `delete-*`, `gsutil rm|rb` and `rsync -d`); containers (`docker rm|rmi|prune` and the
  `container|image|volume|network|system|builder|stack|service|secret|config|node|plugin` forms, and `docker compose down -v|--volumes|--rmi` and `compose rm`, against `-H`, `DOCKER_HOST`, `--context` or `DOCKER_CONTEXT`); git (`push --force`,
  `--force-with-lease`, `-f`, `--delete`, `-d`, `--mirror`, a `+refspec` or `:ref`, to a remote that is not local); and `unparsed`, a command the guard
  could not read that holds a destructive word and a non-local word. kubectl delete, terraform destroy and helm uninstall, delete and del are **N8** and stay always
  denied: they are never held and no approval can change that.
- **How a target is classified.** *Local* is loopback (`localhost`, or an IP literal in 127.0.0.0/8 or `::1`: `127.evil.com` is a host), a unix socket or an empty database host, a file or path, the
  local Docker socket or the `default` context, an endpoint URL on loopback (a local stack), or a git remote whose URL is a local path. Everything else
  is **production-linked** unless this device's `safe` list names it. The `prod` list forces a target production-linked, even a local-looking
  name, and wins when a target is in both. A command with several targets is exempt only if every one is local or safe, and a comma-separated host list (`psql -h a,b`, `postgresql://127.0.0.1:1,db.prod/app`, `host=a,b`) is local only if every host in it is. A connection string or URL host is read as well as `-h`; `service=` and `PGSERVICE` name a host the guard cannot read, so they are unknown and guarded. Targets are read from `shell::hosts`-style
  URLs, `-h`/`--host`, `PGHOST`/`MYSQL_HOST`/`DATABASE_URL`/`REDIS_URL`/`MONGODB_URI`, `--profile`/`AWS_PROFILE`, `--project`/`CLOUDSDK_CORE_PROJECT`, `--subscription`,
  `--context`/`DOCKER_CONTEXT`, `s3://` and `gs://` names (a bucket only describes the command; it matters only if the `prod` list names it) and, for
  a push, **every** push URL of the remote (hardened `git remote get-url --push --all`, which applies `pushurl`, `insteadOf` and `pushInsteadOf`): any non-local URL guards the push. A push the command itself could redirect (`-c`, `-C`, `--git-dir`, `GIT_CONFIG_*`, `GIT_DIR`) is guarded as unresolvable. SQL is read with comments removed and case folded: DROP, TRUNCATE, DELETE (with or without FROM), and an UPDATE with no WHERE outside a string or comment, statement by statement; where the reader cannot be sure (an open quote or comment, a dollar-quoted body, a backslash) any of those words, or ALTER, anywhere in the SQL guards, and SQL fed on the input (a pipe, here-doc, `-f -`) is read even beside `-c`. **Fail closed:** a target that is a variable, a tool whose default is remote
  and that names no target (`psql` with no host, `aws` with no profile, `gcloud` with no project), an unreadable command with a destructive word and a
  non-local word (an open quote, or a program that is a variable) and an `ssh` remote command that holds a destructive word the guard did not read are all guarded. An `ssh` with `-J`, `-o ProxyJump|HostName`, `-L/-R/-W` forwards is judged against every host it names, one with `-o ProxyCommand|LocalCommand` or `-F` is always guarded, and `-o RemoteCommand` is read as the remote command. A script fed to a remote shell (`echo '...' | ssh host bash`, a here-doc, `< file`, `cat f | ssh host sh`) is read for destructive words, and guarded when its source is not a literal `echo` or `printf`. Backslash-newline is joined before a command is read. This is a best-effort reading, not a proof: it can be fooled, and the person's approval is the real control.
- **The two lists live on this device**, in the table `crew_guard_targets` (schema **43**) in the data folder, never in a repository file, so a file a worker
  or teammate commits cannot loosen the guard. `.orbyn/crew.json` is not read for it. An entry is a list (`prod` or `safe`), a pattern (an exact name or
  `*.suffix`, lowercase, letters, digits, `.`, `_`, `-`, at most 253 characters; a URL, user, port, path or anything the secret scan objects to is
  refused), a kind (`host`, `profile`, `project`, `context`, `workspace`, `bucket`: an entry matches only a target of its kind) and an optional note (at most 200
  characters; a note with a secret is refused, not stored redacted). At most 200 entries in all. Add and remove write an audit row in the same transaction, before the change.
- **Routes** (Preview-gated: 404 with the flag off for every method; all are in `crew::changes`, so an agent's request to one is refused by path, N1):
  `GET /api/crew/guard/targets`; `GET /api/crew/guard/test?command=...&thread=...` (what the guard says: category, target, local or production-linked,
  the list entry that decided it, a verdict of `not_destructive`, `local`, `exempt` or `guarded`; read-only, nothing runs, the command is bounded to 2000
  characters and shown redacted); person-only writes, local only, `POST /api/crew/guard/targets/add` and `/{id}/remove`.
- **Loosening needs the person's presence.** Adding a `safe` target or removing a `prod` one loosens the guard, so it needs the same proof as approving
  once: a grant the desktop app records after the person clicks in its own dialog, sent in `x-orbyn-confirm`, bound to `PresenceFor::GuardLoosen {action, kind, pattern}`
  (`add_safe` or `remove_prod`, the entry's kind and the exact pattern), 60 seconds, used up by one change. Adding a `prod` target or removing a `safe` one tightens and needs none.
  **The shell's dialog for loosening is not built yet** (PR2 only knows approve-once), so until it is, loosening answers `403 desktop_required` (no shell) or
  `403 presence_required` (a shell, and nothing can mint the grant) for every caller. Tightening works now.
- **Ledger.** The `held`, `approved_once`, `hold_used`, `denied`, `disagreement` and `allowed` rows of a guarded command carry `guard: {category, target, matched_by, list}`
  (`list` is `prod` when the prod list forced it, else `none`; the target has no credentials and is redacted). A new kind, `command_exit`, records
  `{hold, row, exit_code, action, guard}`: when a command the guard let through (a used approval, or Full with both keys) is later reported finished by the
  engine, `exit_code` is the number; when nothing arrives in 10 minutes it is exactly `unknown`, which the app draws as "Exit status unknown". The watch starts only for a guard hold that was used or a guarded command both keys allowed, and matches the engine's record by thread and by the exact command (whitespace collapsed, one shell wrapper such as `zsh -lc` removed), never by substring. It is
  **best effort and in memory**: some engines report no exit code, and a restart drops the watch (no row is written then). One background task reaps expired watches for all of them.

**Not built:** the screens (Settings > Crew lists, the test box, the held card's guard detail and the ledger rows: slice 4i PR C); the desktop shell's
dialog for loosening; any read of `.orbyn/crew.json` for the guard (deliberately none).

**Known limits (heuristic, not secure).** Not detected, by design of a text reader: `dropdb`, `mysqladmin drop`, `git send-pack --force`, a destructive command run through `kubectl exec` or `docker exec ... -- psql`, a loopback tunnel (`ssh -L 5433:prod:5432` and then `psql -h localhost -p 5433`: the target reads as local; list `localhost` as `prod` while you use one), and static keys in a config file that override a safe-listed AWS profile (keys on the command line are guarded). `docker` with no `-H`, `--context`, `DOCKER_HOST` or `DOCKER_CONTEXT` in the command is read as the local daemon, which is wrong if `docker context use` pointed the CLI at a remote host earlier; prefix the command or list the context. An engine that wraps a command in something other than a plain shell is not matched for its exit status. **Heuristics can never be complete.** The guard reads a command's text and names what it knows; new tools, new flags and new ways to hide a command appear faster than a list can follow, so treat every rule here as a speed bump that catches the common and the careless, never as a boundary. The re-resolve of a held command's targets happens when the person approves and when the worker retries, not when the command executes, so a target that changes inside the command line after those checks is not seen (an in-command time-of-check gap). Wrappers are unwrapped for `sudo`, `doas`, `env` (including `-S`), `nohup`, `time`, `nice`, `ionice`, `exec`, `timeout`, `stdbuf`, `xargs`, `caffeinate`, `setsid`, `chroot`, `unshare`, `flock`, `watch`, `parallel`, `aws-vault exec`, `script -c` and `sh -c`; for any other program followed by the name of a tool the guard knows (`mytool --fast ssh prod ...`) the later word is read as a command, which can over-guard. Not closed: `rm -f` without `-r`; code handed to an interpreter (`ssh prod python3 -c "shutil.rmtree(...)"`), whatever it deletes; a remote shell under `sudo`, `su` or `env` is read as a shell when a pipe, here-doc or redirect feeds it, but other ways to start one are not all covered; `git push --force-with-lease` to a GitHub remote is held by default, and exempting it means safe-listing `github.com`, which also exempts a production repository there; `docker compose` with a project file that points at a remote host, `ssh` commands built from variables or `$(...)`, an `ssh` command that reads its commands from a file given by the shell, `ssh` aliases and config the guard cannot read, MySQL `#` comments (read as unsure: every keyword counts), SQL in a file (`psql -f`) and any program that runs one of these tools inside itself. It does not parse a shell wrapper it does not unwrap, an alias or a function, or a script
that a command runs (`psql -f drop.sql`, `bash cleanup.sh`, `make nuke`): the file is not read. A tool outside the categories (`sqlite3`, a custom CLI, an HTTP call to
a deletion API) is not guarded; the network rule N3 and the tiers still apply. A remote's URL is read from the repository's git config, which an earlier command
of the worker could have changed; so is an `ssh` alias that points to production under a name the person never listed. The lists decide what is local and safe,
and a wrong entry is the person's to fix. Nothing here is tamper-proof: a process of the same OS user can still read the data folder and change the table.

**As built (slice 4h, state):** background crews and their runs, stored on this device in the service's data folder (tables
`crew_background` and `crew_background_runs`, schema 41, 42), behind the Preview flag. **Decision: the state is not a file in the repository.**
The plan said `.orbyn/crew.json`, but architecture rule 4 (Orbyn writes nothing into a target repository) rules a written file out, so a
background crew is per device and is not shared through git. A repository's own `.orbyn/crew.json` (read-only for Orbyn) is unchanged.

- **A crew:** id, project, name (1 to 80 characters, unique per project without regard to case, at most 50 per project), template
  (`fix_failing_ci`, `flaky_test`, `dependency_bump`, `security_advisory`), trigger kind with its settings, run budget, an enabled flag and
  times. It is created off; turning it on is its own step. An unknown template, trigger kind or field is refused.
- **Trigger settings** are strict and belong to the kind: `ci_failure` takes `branch` and `workflow` (optional), `issue_label` needs `label`,
  `schedule` needs `every_hours` (1 to 720), `dependency_update` takes `ecosystem` (optional). Text settings are 1 to 100 characters. A
  setting of another kind is refused. Name and text settings go through the locked redactor before they are stored, and are refused when
  they hold a hidden character: zero-width, bidirectional, tag-block and other invisible format characters (a wider list than the Autopilot
  tripwire's, which keeps its own narrow one because it reads agent text),
  so `fix-ci` and `fix` plus a zero-width space plus `ci` are not two names. Names are compared lower-cased, not Unicode-normalised: two
  visibly identical names built from different code points (for example a precomposed and a combining accent) are still two names.
- **Budget:** `turns` 1 to 1000 and `minutes` 1 to 1440 per run, **set at creation and never changed**: an update takes only the name and
  the trigger, and the table refuses a change of budget, template or project, and (schema 42) a run's copy of the budget. A template and trigger kind may be combined freely.
- **Runs and state:** a run is `running`, `stopped` (with a reason: `budget`, `repeated_failure`, `engine_error`, `disabled`) or
  `needs_review`, with turns and minutes spent against the budget it was given. At most one run per crew is running, and a run opens only for
  an enabled crew. A crew's state, first match wins: `running` while a run is
  running; `needs_review` when its newest run finished with work to review; `stopped` when its newest run stopped for the budget or a
  failure (`budget`, `repeated_failure`, `engine_error`), even with older reviews waiting; `needs_review` when the crew has no run or its
  newest run was stopped as `disabled` while an older review still waits; otherwise `idle` (never ran, or the newest run was stopped by the
  person's own switch and nothing waits). Spending to either number of the budget stops the run with the reason `budget` and puts it in
  the review queue (`review_waiting`). Turning a crew off stops its run with the reason `disabled`, logged as the person. **Delete is
  refused only while a run is in progress**; a crew with waiting reviews can be deleted (nothing clears a waiting review yet), its runs
  and waiting reviews are removed with it, and the audit row records `runs_removed` and `reviews_waiting_removed`.
- **Audit:** create, update, enable, disable, delete, and each run's start, spend, stop and review write an `audit_events` row in the same
  transaction as the change (rule 6), before the row it describes changes (a spend that reaches the budget writes the spend row, then the
  stop row, then updates the run once); a failed audit row rolls the change back, and the rows stay after a crew is deleted. A person's
  disable, and the stop it causes, are logged with the person as actor and source; a run's start, spend and other ends as `crew`.
- **Routes**, all under `/api/crew/background`, behind the Preview flag like every Crew route (404 with it off, 401 without the token, writes
  refused for a remote viewer and listed in `crew::changes` so N1 refuses an agent's request): reads `GET /` (`project`, `limit`, `offset`),
  `GET /{id}`, `GET /{id}/runs`; writes, local only, `POST /create`, `/{id}/update`, `/{id}/enable`, `/{id}/disable`, `/{id}/delete`.
  There is no route that starts a run, spends, merges or reverts.

What is not built: **no trigger fires and no engine starts**, so no run exists unless the core's run functions are called (tests do; slice 4h
will). A budget-stopped run is queued as a waiting review on its own row and **is not yet an item of the after-merge Review queue**: that queue
lists merges, each with a merge commit and a revert, and a stopped run has neither. Joining the two, and the person's answer to a waiting
review, wait for the slice that brings a run's result through the merge queue, so a background crew has no merge authority today. Known
limits: removing a project leaves its crews (a later trigger must refuse a crew whose project is gone); crews are per device; a template's
prompt and checks are not defined yet.

**As built (slice 4i, routing):** a repository can say which engines may take which paths, in `classes` of `.orbyn/crew.json`. The router
uses it as a hard filter. Preview only; Orbyn reads the file and never writes it (architecture rule 4).

```json
{ "classes": { "version": 1, "rules": [
  { "name": "auth-paths", "paths": ["**/auth/**", "crates/keys/**"], "engines": ["claude"], "reason": "Sign-in code stays on one vendor." }
] } }
```

- **Read at the commit the task starts from, not from the checkout's files.** The commit is the lead checkout's HEAD, read once before
  routing, and the file is read at it through hardened git (`ls-tree` then `cat-file`). The decision and the ledger row record the commit and
  the file's blob id. A running task's routing uses the crew.json recorded when it started (the commit and blob id are in the `route_class` row); a later change has no effect on it, and an uncommitted edit of the file is not read. The `.orbyn` entry must be a tree or
  absent (a symlink there would read as a missing file), the file must be a plain blob (mode 100644: a symlink or submodule is invalid), and its
  size (64 KiB at most) is read before its content. No file, or none with `classes`, means one class, "Everything else": any signed-in engine.
- **Strict inside `classes`.** `version` must be 1; at most 64 rules, 32 paths and 16 engines per rule; `name` 1 to 40 characters of `a-z`, `0-9`,
  `-`, unique; `reason` 1 to 200 characters; unknown keys inside `classes` are refused. `engines: []` is valid and blocks every engine. An engine id this
  machine does not have is kept and shown as "not on this machine". Unknown keys at the top of the file belong to other slices: they are noted
  and ignored. A file holding a secret (the shared detector) is refused.
- **An invalid file fails closed.** If the file cannot be read, does not parse or breaks a rule above, every engine is blocked by the rule
  "crew.json invalid" with the problem in plain words, and the task waits in Needs you. It is never read as "no rules". The same file also holds
  review `lanes` and `allowed_hosts`, so a file that is not JSON at all blocks every start now, not only the review; a file whose `lanes` are wrong
  but which is valid JSON still starts and fails at the review as before. The dry run (`crew_route`) says nothing about classes for a thread with no
  commit to read, and a start there is refused on its own terms.
- **Grammar (v1).** `/`-separated parts. `*` matches any run of characters inside one part; a part that is exactly `**` matches any number of
  parts, none included. `?`, `[`, `]`, `{` and `}` are refused so a pattern means one thing everywhere. A pattern with a backslash, a leading
  `/`, a drive letter, an empty part or a `.` or `..` part is invalid. Matching ignores case and Unicode normalisation (NFC, lowercase, NFC):
  on a case-folding file system two spellings are one file, so a pattern matches the spellings the file system would, which tightens the rule.
  It is not the file system's own table: a final sigma is a sigma, but `ss` and the sharp s stay different names (as on APFS), and Turkish dotted and
  dotless i are lowercased only, so a name that is one file only under some locale or file system can be missed.
- **Which classes a brief is in.** Every pattern of `brief.files` counts, read and write alike; a brief with no files is unknown and is checked
  against every class. For a lease (`dir/**`) the glob is walked against the directory (a worker with the lease may create a matching path, so
  `**/auth/**` may match under `src/**`: narrow the lease to avoid it), and the tracked files under it, listed from git at the same commit
  (50,000 at most, more counts as unknown), are matched too. A tracked symlink under a lease is followed inside the tree and what it reaches
  is classified as leased; one that points outside the tree, is absolute or has no target counts as every class. When several classes match, the
  allowed engines are the intersection; if it is empty nothing starts.
- **In the router.** `filters::class_screen` runs inside `route()` before every other filter and before scoring. A blocked engine is
  rejected with `path_class` ("Blocked by rule: auth-paths, allowed engines: Claude. <reason>"). An explicit engine tag on a blocked engine is
  refused and never swapped. The decision gains a `path_classes` reason with the class names, the commit and the blob. The dry run
  (`crew_route`) reads the classes too, so it never says yes to what a start refuses. Engines that are not installed are also listed as blocked,
  because the class filter comes first.
- **Ledger.** Each routing appends one `route_class` row to `crew_autopilot_events` (no migration: `kind` is free text), through the ledger's
  redaction: the classes matched with their pattern and up to 20 sample paths, each blocked engine with the rule and its reason, the engine
  chosen or none, the commit and blob, and the time. On a refusal the row also keeps the whole `rejected` list, the requested engine and the
  message, which is where the Needs you item reads its structured rows. A routing whose row cannot be written does not go on.
- **Routes** (Preview, 404 with the flag off, reads only, listed in `crew::changes` `READS`): `GET /api/crew/classes?thread=` (commit, blob,
  rules, problems, notes, the worktree label, and whether the checkout's copy differs: "not in force until committed"),
  `GET /api/crew/classes/test?thread=&path=` (1 to 10 paths, `path` repeated or one per line, each validated: classes, matched pattern, allowed engines, the
  route-card sentence), `GET /api/crew/route/{task}` (the decision or the refusal, and the `route_class` rows).

What is not built: the guard that watches a worker against its classes (PR B); the Settings screens, the route card and Needs you screens
(PR C); enforcement at the gate and at verify time, so **a worker that is allowed to run shell commands can still read a file outside its lease
until #441's sandbox covers it, and a symlink a worker adds during the task is not seen**; an ask-me route and a `locked` flag on a rule
(ruling pending). Known limits: **the file is read at the checkout's HEAD at task start; an agent that can run git can move HEAD or commit a change, and the next task reads it; an
agent that can push to the base branch can change the rules for later tasks** (the N7 check only blocks a git command whose path argument is
`.orbyn/crew.json`), so a class is not a guarantee against an agent; reading a ref an agent cannot move, such as the default branch, is follow-up #535. A path or lease spelled in a different case from what is tracked (`DOCS/key`, `Docs/**`) lists nothing, so it
counts as every class. The work is bounded per brief (100,000 paths and 2,000,000 glob tests, charged at every pattern for every path, and a tenth of
that for the tester): over it the brief is unknown, which means every class, so a huge tree is slow-but-closed, never open. The matching runs off the
async runtime. A thread with no commit (a fresh `git init`, or a folder that is not a repository) is refused
with "no commit to read it at": the rules cannot be read, and not knowing is never a reason to allow. Review briefs always have
`files: []`, so they count as every class: a rule with `engines: []` blocks every review in the repository, and a class that allows one engine plus
"the reviewer differs from the builder" means no reviewer can start. The class set is whatever the committed file says, so a person who edits it uncommitted sees "not in force until
committed" and nothing else changes; the lease walk widens on purpose, so a broad lease can be tied to a class it never touches; a class names
engines by id only (an instance of an engine is its own id); the tracked-file listing is read whole before the 50,000 cap is applied.

**As built (slice 4f, UI part B):** the risk, Review queue, Merge queue, watch-on-main and digest frames of Crew Designs
v19 (34 frames) as display-only Preview screens in `web/src/features/crew/autopilot/review`, opened by
`#/preview/crew/<frame>` in dev builds only. They run on sample data: no HTTP route or generated binding exists for the
Autopilot group yet, so every action that would merge, revert, send a brief or approve is disabled and says why, and each
takes a backend-guarded handler when the routes land. Per-merge money figures are left out until the Designer settles
per-engine units.

**As built (slice 4f, UI part C):** the escape log (Crew Designs v20, 17 frames), review capacity (v21, 18 frames) and the
Settings › Crew page (v23, `settings-crew`, with its Review capacity, Independent oracle and Reviewer backtest rows) as
display-only Preview screens in `web/src/features/crew/{escape,capacity,settings}`, opened by `#/preview/crew/<frame>` in
dev builds only. They share one stage in `features/crew/shared`: the design's own markup, with its rules renamed `cx-*` in
`shared/design.css` on the app's tokens. None of them has a backend, a client or a guessed route: every button is disabled
with "Crew is not connected to this app yet", and the escape and capacity backends are separate slices. Values the design
marks Proposed (the 60-day window, the 10-merge and 2x router tie-break, a limit of 8, sampling one in four rounded up)
stay Proposed on the screen. The settings rows are drawn in the Preview frame only: in the real Settings page they would
show a state ("Not set") that nothing stores yet. The v22 units apply to the v19 frames too: the header line reads
"Codex 62% of its weekly limit", per-merge cost is tokens and minutes in the engine's own unit, and the review-reverted
card offers "Create a check".

**As built (slice 4f, UI part D):** the independent oracle (Crew Designs v22, 18 frames) and the reviewer backtest (v23, 16
frames) in `web/src/features/crew/{oracle,backtest}`, on the same stage and with the same rules as part C: drawn sample
data, no client and no route, every button disabled with "Crew is not connected to this app yet". The oracle gate fails
when the key kills no mutant, strengthens for two rounds and then asks the person, and a key from the builder's own
engine always needs the person's approval. A backtest starts only when the person chooses it and only with a budget for
each engine, in tokens and never added up; it replays the last 180 days of pull requests that are 30 days old or more,
shows a rate from 5 escapes and from 10 findings, and takes its outcomes from reverts, fixes and the escape log only.
The lane picker's backtest counts are information and never change what Auto picks. All values the design marks
Proposed stay Proposed.

**Autopilot decisions** (2026-11-11, Designer part 5, `crew-designs.html`, orbyn-designs pull requests #139 and #140):

- **Review windows under Full:** High within 4 h (top of the queue, overdue alert), Medium within 24 h, Low batched with
  no deadline.
- **Reverting with later merges on top:** the crew checks that the later merges pass without the reverted one. If any
  depend on it, they are listed and it asks the person, even under Full.
- **Automatic revert** only when a check fails with the merge, passes without it, and the failure reproduces twice.
  Otherwise it asks.
- **Second key with only one vendor signed in:** denied under Full (with a non-blocking "Sign in to a second engine");
  asked under On or Off.
- **Crew page:** opens on Review when items are waiting, otherwise on Flow.
- **Digest:** 17:00 local by default (Settings, Crew), with a notification only when something happened.
- **Trust rates:** shown from 10 merges, over a 60-day window, per repository, on this device.
- **Background crews:** budget set at creation. Hitting it means Stopped, reason "budget", and the run goes to the
  Review queue.

## What keeps us ahead

What the surveys suggest will be standard by mid-2027 (2026-11-11; several sources secondary):

- Background or cloud agents that open pull requests
- Parallel subagents in one harness
- Classifier approval instead of prompts
- Agents defined as code
- Control planes across several vendors (Agent HQ, Warp Oz, Augment Cosmos)
- Review before the pull request
- Agent usage metrics

Not found shipped anywhere:

- Merge queues for agents
- Review after merge with auto-revert
- Learning from past reviews
- Routing by measured outcomes

| Feature | Why (evidence) |
| --- | --- |
| Autopilot and two-key approvals | Auto-approval is becoming standard but can be fooled; limits in code plus a cross-vendor second key cover the gap |
| Agent merge queue and conflict forecasting | Not shipped anywhere; parallel agents conflict, with about 30% lower success when two agents collaborate (CooperBench, secondary) |
| After-merge Review queue, watch on main, auto-revert by bisect | Not shipped anywhere; matches how the owner wants to work |
| Trust scores: revert and rework rate per engine, area and reviewer | RADAR-style gating; shown to the person and used by the router |
| Review memory | Learns per repository from accepted and dismissed findings and from after-merge reviews. Measures each reviewer's precision per finding type and mutes noisy classes. Copilot memory gave +3% precision and +4% recall (vendor A/B) |
| Measured reviewer pairing | Cross-vendor review is asymmetric (Claude reviewing Codex: 71.6% to 89.7%; Codex reviewing Claude: 91.4% to 82.8%, arXiv 2607.21656), so pair reviewers by this repository's measured direction, not by assumption |
| Premise check | Wrong facts in a brief corrupt even test-based selection (arXiv 2610.05170); check the brief's stated facts before spending runs |
| Split by dependencies, plus a solo baseline | File-based splitting raised cost 60% with no gain (arXiv 2606.00953). Split by the dependency graph and show "crew vs solo" on the same task class, so the crew proves it is worth the cost |
| Background crews defined in `.orbyn/crew.json` | Triggers: CI failure, issue label, schedule, dependency update. Templates: fix failing CI, flaky test, dependency bump, security advisory. Agents-as-code is becoming standard; ours run the full Flow |
| Per-hunk provenance compatible with Agent Trace | Interoperate with Cursor's Agent Trace and Git AI (git notes) rather than invent our own; wrap it in our in-toto proof |
| Cheap draft, strong check | A cheap model drafts, and work escalates to a stronger one only on failing checks or low-confidence review; measured per repository |

## Seeing who works on what

Rule: a person can answer "who is doing what, why, and is it proven?" for any task without reading a transcript.

**Three levels of workers, one tree**

1. The lead thread.
2. Crew workers: threads on other engines.
3. Each engine's own subagents (Claude's Task agents, Codex's collab agents). The drivers already emit
   `subagent.activity` with a parent id: `driver/claude/events.rs:95`, `driver/codex/events.rs:265`.

ACP engines that report no subagents show "subagents not reported", never an empty tree that looks like nothing is
happening.

**Each node shows:** engine; model and effort; state; what it is doing now (current step or tool, in plain words);
elapsed time; usage; worktree and branch; file lease; unread messages.

**Views**

| View | Answers |
| --- | --- |
| Flow | The whole pipeline for every task, from brief to merge: every stage, every reviewer lane, every loop back to Build, and who decided each gate (code, a confirmed finding, or the person). It works like a CI graph, but for agents. Each edge carries its artifact: brief, diff, check results, findings, pull request. A scrubber replays how the task got here |
| Crew map | The live tree above: who spawned whom, and who is talking to whom |
| Taskboard | Cards by state: Queued, Routed, Running, Verifying, Needs you, Passed, Stopped or Cancelled. A card is a brief: kind, engine, owner, checks passed and total, last activity. From the card the person can reassign the engine, cancel, add a check, answer or open |
| Conversation lanes | One lane per worker over time. Messages show as arrows between lanes, each marked "live" (steered) or "next turn". The board's facts show with their author and evidence |
| Route card | Why this engine: the reasons, every rejected engine and why, and what the person can change |
| Needs you inbox | Questions, approvals, reviews and conflicting board writes, oldest first, each with the one action it needs |
| Task report | What changed (the diff), what was verified, what was not, risks, cost, and a timeline the person can replay |

**UI decisions** (2026-11-11, Designer part 1, orbyn-designs pull request #129, frames `crew-*`):

- **Cards:** they move only by what the work does. A person can cancel or reassign, but never drag a card between
  states.
- **Problems are visible:** an offline or signed-out engine turns its tasks to Needs you, never a silent fallback.
- **Colours:** amber means Needs you only, red means failed only.
- **Crew page:** shows the current project by default, with an "All projects" filter, remembered per person.
- **Map:** it becomes a grouped list automatically past 8 workers, with a Map or List toggle per crew.
- **Stale:**
  - No engine event for 5 minutes shows "Quiet for 5 min", neutral, not amber. Running checks and jobs count as events.
  - At 15 minutes the task goes to Needs you as "May be stuck", with Nudge, Cancel and Reassign.
- **Budget:**
  - Set per crew, with a default in Settings.
  - Unset shows raw usage per engine.
  - Units are never mixed into one percentage.
  - When the budget runs out the task is Stopped, reason "budget".

**Protocol UI decisions** (2026-11-11, Designer part 3, orbyn-designs pull request #134):

- **Joining:**
  - L0 members join at `http://127.0.0.1:<port>/crew/<crew-id>/mcp` with a Bearer token.
  - The app generates the snippet for each harness. Engineering checks each CLI's syntax when building.
  - The token is shown once.
  - Lifetimes: 1 h (default), 24 h, or until the crew ends.
  - Each token is single-use.
- **Lease conflicts:** the lead gets 2 minutes, then the conflict goes to Needs you.
- **Limits:** 3 exchanges between two members before the lead is told, and 30 posts per member per run. Repository rules
  can lower both.
- **Interruptions:** low-risk questions are batched for up to 10 minutes (Settings, Crew). High-risk questions
  interrupt at once.
- **Route card:** no pass rates until measured ("No verified results yet, using defaults"). A rate always shows with its
  sample size.
- **L0 members:** always show "Subagents not reported" and "Self-reported".

**Proof and fork decisions** (2026-11-11, Designer part 4, orbyn-designs pull request #137):

- **Check kinds:** the list under [Verification without limits](#verification-without-limits), plus Screenshot only,
  Accessibility audit, Performance budget, Secret scan and Licence scan.
- **Verifying the proof:** `orbyn verify <PR URL>`, or verify from a file. The predicateType is
  `https://orbyn.dev/crew-result/v0`.
- **Contract changes:** the lead briefs an affected consumer without asking, with "Step in" available. With Autopilot
  Off it asks first.
- **Fork data:** below 5 comparisons, individual results only. From 5, a rate with its sample size. The router uses the
  data from 10. Per repository, on this device.
- **Remote A2A agents (Preview):** the person provides an HTTPS endpoint, and Orbyn opens no tunnel.
- **Same-engine Security review:**
  - Never the default.
  - With On or Off, always ask.
  - With Full, when no other vendor is signed in: it runs at the lead tier, badged "Same engine as author", and the task
    is raised to High risk.

**Device, welcome and access decisions** (2026-11-11, Engineer Designs after pull requests #150 and #151, frames
`device-*`, `welcome-*`, `scopes-*`, `update-tracks*`; from parity rows H21, H31, K35, L27, P6):

- **Welcome wizard (K35):** three steps (Agents, Add project, Agent history import). Remote computer setup is excluded,
  so the computers steps are removed.
- **Scoped access (L27):** only the local checks are in the beta (route, launch token, Host and Origin). `scopes-local`
  and `scopes-refused` are buildable. The scope list, editor, request and revoke frames are Planned (remote access) and
  are not built.
- **Devices:** Orbyn installs no toolchains, including Android. It shows the vendor's steps and "Check again". Windows
  setup is "Not verified". 3D and fold views are removed.
- **Update tracks (P6):** Stable is live. Nightly is Planned until the release feed has a nightly track.

**Warp part D decisions** (2026-11-11, Engineer Designs after pull request #152, frames `block-*`, `redact-*`,
`editor-*`, `hunk-*`, `profile-*`):

- **Command blocks:** "Ask Crew to fix" works in any thread; with no crew it opens Start a crew prefilled. A
  full-screen program ends as one block with no output kept.
- **Redaction:** on for what agents read by default, switchable per project with a warning and an indicator. The ledger
  and Crew objects are locked on. A reveal hides after 2 minutes or when the person leaves the thread.
- **Editor:** 5 MB read-only and top 5,000 lines are proposed limits, and Engineering measures them. Outside-editor
  opening stays in the file menu, the editor tab menu and the Details popover.
- **Revert hunk:** Undo toast for 10 seconds, then checkpoints. It is disabled on staged hunks. Orbyn stays out of
  staging.
- **Profiles:** they pick among the four existing permission modes (Supervised, Auto edits, Auto, Full access). There is
  no read-only mode. Examples are "Careful", "Fast" and "Hands-off".
- **Start a crew dialog:** Profile, Budget and Autopilot mode (Full by default) in one dialog.
- **Settings:** Crew is in the Agents group, and Secrets sits beside Privacy and Telemetry.

## Workspace shell redesign

The app is still in beta, so the shell changes once, together with Crew, rather than shipping Crew into today's side
panel. Problems in the current panel (the owner's screenshots, 2026-11-11):

- **Details shows twice.** A Details card sits at the top of the side panel, and the same facts appear again in the tabs
  below it.
- **Tabs are all open by default.** Every tab is open even when unused.
- **The Diff is noisy.** It shows the raw git header (`diff --git`, `index`, `---` and `+++`). It shows unexplained
  numbered pills on the right edge (they look like line or comment anchors). Its scope and view controls are spread
  across two rows.
- **Tools contradicts itself.** For OpenCode the Tools tab says "Not reported yet" in one place and "Couldn't read the
  tools, Try again." in another.

**New shell**

| Area | Decision |
| --- | --- |
| Left bar | A VS Code-style explorer: project, worktrees and branches, file tree, plus Crew entries. The person can browse any project and worktree from here, and opening a file opens it as a tab. Files leave the side panel |
| Details | One place only: a popover from the thread header (branch and path, changes, PR, automations, tools, local servers, scripts). Not in the side panel |
| Side panel | Starts with **no tabs**. A launcher (+) opens what the person picks: Changes, Browser, Activity, Review and comments, Tools, Crew views. Tabs are browser-style, can be closed and reordered, and are remembered per thread |
| Changes (diff) | Fully redesigned and simpler (Codex's view is a reference for clarity only, not for its look): one header with a scope menu (Working tree, Turn, Thread, Branch, Commit) and the compare pair (`main` to `origin/main`); a changed-files list with a filter beside the diff; per file `+` and `-` counts, a rendered or source toggle for Markdown, and open in editor; line numbers in the left gutter, long lines wrapped, and no raw git header; Unified or Split and whitespace options under one menu |
| Comments on lines | No unexplained number pills. Hovering a line number in the gutter shows "+". Click or drag-select lines to comment, using the existing comment threads. In Crew, a comment can become a brief for a worker or a finding for a reviewer lane |
| Tools | One honest state per engine: "Not reported by OpenCode" (ACP engines that do not list tools), or an error with the reason and Try again, never both |

**Shell decisions** (2026-11-11, `engineer-designs.html` and `components.html`, orbyn-designs pull request #135, frames
`explorer-*`, `panel-*`, `details-popover-*`, `changes-*`, `comment-*`, `tools-*`; drawn but QA was still running.
**Build from the merged QA fixes** in the same file, not pull request #135 alone):

- **The + launcher** keeps Side chat.
- **The Review and comments tab** shows line comments as Drafts (with "Send all to engine"), Open and Resolved, plus the
  linked pull request's forge comments. Each item jumps to file:line.
- **Explorer default:** the current project and its current worktree are expanded. The rest start collapsed and are
  remembered.
- **Other worktrees** are read-only unless the thread runs there ("Start a thread here").
- **Changes** has no staging buttons. Staged and unstaged shows only when something is staged.

## Verification without limits

- **No cap on checks.** An answer key holds any number of checks, and any check can be re-run at any time. The person
  or the lead can add checks after a task passes. The loop guard limits worker retries only, never verification.
- **Check kinds:**
  - Command, test, lint, typecheck
  - File scope (only leased paths changed)
  - Diff size
  - Schema
  - HTTP probe
  - Preview check: a page loads with no console errors, plus a screenshot, using the browser tools the app already has
  - Visual compare against a design frame
  - Repository-required checks (inherited automatically)
  - A custom script
  - The person's own manual check (a checkbox with a note)
- **Model review is advisory.** Another engine's review is shown as a finding, never as the pass or fail gate.
- **Every result stores:**
  - The exact command or check
  - Exit code, duration and the commit it ran on
  - An output excerpt and the full log
  - Artifacts (screenshot, diff, report)
  - One line on **what this proves and what it does not**
- **Stale marking.** A check that ran on an older commit shows as stale until it runs again.
- **Not verified is visible.** The task report lists what no check covered, so "passed" never implies more than was
  checked.

## Prototype slices

Engineering app, one pull request each.

| # | Slice | Done when |
| --- | --- | --- |
| 0 | Crew Protocol v0 spec, JSON Schemas and conformance cases (fake engine) | Schemas validate; the conformance suite passes against our own implementation |
| 1 | Contracts: `Brief`, `AnswerKey`, `CrewResult`, `RouteDecision`, `CrewMessage`; store tables | Types generate to TS; migration tests pass |
| 2 | Router v0 and a `crew_route` dry-run tool | Unit tests on the fake engine: filters, scores, explicit override, every rejection has a reason |
| 3 | `crew_start`, `crew_report`, verifier and loop guard, built on `delegate` | Fake-engine test: fail, retry with reason, same failure twice, change approach, stop; the answer key is never sent to the worker |
| 4 | Crew bus: `crew_post`, `crew_ask`, board, steering delivery, guards | Two fake engines exchange a board fact mid-turn; the hop limit trips; a message after the run ends is refused |
| 4b | Review stages: reviewer lanes (no cap), cross-vendor routing, typed findings, finding check, fix loop, repository rules that add lanes | Fake-engine test: an unconfirmed finding does not block; a confirmed one sends a fix brief; the re-review shows it fixed; a lane added after a pass runs |
| 4c | Ledger and attestation; claims bound to checks; contract facts generate checks; Disagreement objects; L0 join by token | Replay a crew from its ledger; a pull request carries a verifiable in-toto statement; an unbacked claim shows unverified; a contract change alerts its consumer |
| 4d | Counterfactual fork: re-run a stage on another engine, compare verified outcomes, record to Eval history | Two engines' outcomes on the same stage show side by side; the router's table updates |
| 4e | Shell: left explorer (projects, worktrees, files), Details popover, empty side panel with launcher, new Changes view with gutter comments, honest Tools state | Built from the merged frames in `crew-designs.html` and `engineer-designs.html`; old Details card and file tab removed; no duplicate destinations |
| 4f | Autopilot: approval chain, two keys, trip wires, risk tiers, merge queue, after-merge Review queue, watch on main and bisect revert | Fake-engine tests: a hard-denied action is never approved even if the lead says yes; two keys disagree and it is denied; 3 denials stop the task; a high-risk diff waits; a broken main bisects to the right merge and reverts it |
| 4f-API | Routes, generated TS bindings and a web-readable Preview flag for Crew and Autopilot (modes and area promote/demote, risk report, review queue, merge queue and forecast, watch state, digest, approval log, trip wires, trust/guard settings); then the screens are wired | Every write goes through the existing backend guards and no client path merges or reverts; Opus review before merge. The display-only UI (4f part B, risk, Review queue, Merge queue, watch, digest) is merged as preview on sample data and is wired by this slice |
| 4g | Learning: trust scores, review memory, measured reviewer pairing, premise check, crew-vs-solo baseline | Needs real runs (slice 6 budget); numbers always shown with sample size |
| 4h | Background crews from `.orbyn/crew.json` triggers and templates | A CI-failure trigger starts a crew that runs the full Flow and lands in the Review queue |
| 5 | UI: Flow, Auto in the engine picker, Crew map, Taskboard, conversation lanes, route card, Needs you inbox, task report with evidence | Built from the merged frames in `crew-designs.html` and `engineer-designs.html`, every state designed |
| 6 | Live evals: 10 real tasks across 3 engines, scored by the verifier, feeding Eval history | Needs the owner's run budget (`evals.max_runs_per_pr` is unset) |

**Beta scope:** slices 1 to 5 behind a Preview flag. Manual engine choice stays, and Auto is a new option in the same
picker. Labels say Preview, and no claim goes in until slice 6 has numbers.

## Warp features

Source: the open Warp client (reference checkout `orbyn-references/warp`, commit `d7fc3ed`, 2026-11-11) and
docs.warp.dev (fetched 2026-11-11).

- **Licence:** AGPL v3, except the MIT `warpui` crates. We take ideas only: no Warp code or text, as with Macro.
- **What is closed:** Warp's server, Drive backend, Oz and Factories are proprietary. We build our versions local-first,
  stored in the repository or on the device.

**Already have:** command palette, themes and theme editor, keybindings editor, keep-awake during work, deep links, MCP
with approvals, skills, CLI agents in a terminal (engine console, G14), PR flow, worktrees, telemetry export.

**Beta (new):**

| # | Feature | What you get | Built as |
| --- | --- | --- | --- |
| W1 | Command blocks | Terminal output grouped per command with exit status and duration. Copy, rerun, filter, find, collapse, and "Send to agent" (as a context chip) or "Ask Crew to fix" (as a brief) | Terminal drawer upgrade; uses shell integration marks |
| W2 | Smart terminal input | A real multi-line editor with cursor, selection and syntax colour; history search; completions for commands, flags and paths | Completions from Fig's MIT completion specs (check licence and attribution before use); no Warp code |
| W3 | Plain English in the terminal | Type a sentence and it goes to the thread's engine as a prompt instead of the shell, with a visible toggle and undo | A local classifier, always shown, never silent |
| W4 | Secret redaction | Secrets are masked in terminal blocks and in context sent to agents (not applied yet: those consumers are not built). Matches known key formats and the person's own patterns, set in Settings › Secrets. Crew keeps its own fixed rule: its scan, ledger, evidence and every Crew object use every built-in format and read no setting, so nothing on that page can put a secret into them. The person's patterns mask ledger and evidence text only when it is shown (read routes), never in what is stored | Feeds Crew's hard rule "no secrets in any object" |
| W5 | Code editor tab | Files opened from the explorer get an editor: syntax colour, find, go to line, line comments (same comment threads as Changes), save. Language-server diagnostics come after the beta | New panel tab type |
| W6 | Changes: revert hunk, edit inline, attach to agent | Revert one hunk, edit a line in place, or send a selection to the engine | Extends the shell's Changes view |
| W7 | Batched review feedback | "Send all to engine" sends all drafts as one message, so the agent fixes everything in one pass | Already designed (Review and comments tab); confirm the behaviour |
| W8 | Execution profiles | Named presets: engine, model tier, permission mode and Autopilot mode. Picked per thread or crew, for example "Careful", "Fast", "Read-only audit" | Settings, Crew, plus a picker in the composer |
| W9 | Workflows, prompts and env sets | Saved commands with parameters, reusable prompts and environment-variable sets. Stored in `.orbyn/` in the repository so the team shares them through git, plus a personal library on the device. Ties into B63 workflow recipes | Local, no server |
| W10 | CLI-agent attention | When an engine runs in a terminal (engine console), Orbyn notices it waiting for input and shows a notification and a Needs you item | Extends G14 |
| W11 | Run page for a PR | One static page per Crew task: prompt, plan, route, checks, findings and diff, attached to the pull request next to the signed proof | Generated locally; publishing is the person's choice |
| W12 | Crew dashboard | Autonomy rate, cycle time, cost and revert rate per repository over time | Uses trust-score data |

**After the beta:**

- Language-server features in the editor (diagnostics, go to definition, references).
- Vim mode in the terminal input and editor.
- SSH and WSL sessions with the same blocks.
- Saved layouts that open a set of tabs, panes and worktrees.
- Codebase indexing for agent search. Local only, opt-in, with a privacy review first.
- Notebooks (runnable Markdown) and .ipynb viewing.
- Conversation export and import.
- Voice input on desktop.
- Agent memory across threads. Review memory (slice 4g) comes first.
- Triggers from Slack, Linear, Jira, GitHub and GitLab for background crews.
- Local control, so outside tools can drive Orbyn through the `orbyn` CLI.

**Not adding:**

- Warp's hosted parts: cloud sync of Drive, public share links, hosted cloud agents and billing. Hosted services stay
  out of scope.
- A quake-style drop-down window.
- Default-terminal registration.
- Computer use, already covered by our gated Computer action.

## Beta cut

Decided 2026-11-11. The big beta ships everything together: one release carries all three of Crew (the cut below),
every unfinished app row ([Appendix A](crew/beta-rows.md#appendix-a-every-unfinished-beta-row-app-parity), 137 rows from
[parity.md](parity.md)) and every designed screen not yet built ([Appendix B](crew/beta-rows.md#appendix-b-designed-screens-not-yet-built),
160 screens from [design-inventory.md](design-inventory.md)).

Rows the shell redesign replaces (for example C31's add-tab menu, and the Side panel tabs and Files screens) are built
to the new shell designs (`engineer-designs.html` after the QA fixes merge), not the old frames. Excluded scope
(mobile, remote pairing, hosted services) stays out unless the owner says otherwise. Not-doable rows keep their recorded
reason.

**In the Crew beta (Preview):**

- Phase 1 wiring: optional Orbyn account on staging, with personal settings syncing across devices
- Warp features W1 to W12 (above)
- Slice 0: protocol v0 spec and schemas (not yet published)
- Slices 1 to 4: contracts, router v0 on defaults, runs, verifier, bus, leases
- 4b: review stages
- 4e: shell redesign
- 4f: Autopilot (Full by default), merge queue, Review queue, watch on main
- 5: the UI for all of the above
- The ledger, kept locally

**App gaps Crew depends on** (from [parity.md](parity.md) on main; ship them with Crew):

- C31 right-panel add-tab menu (replaced by the shell's + launcher)
- C4 orchestrator summaries
- I21 and I49 OpenCode provider and its behaviour
- I31 ACP v1 and v2 negotiation
- I10 MCP elicitation approvals

**After the beta:**

- 4c remainder: signed attestation, `orbyn verify`, L0 join and A2A
- 4d: counterfactual fork
- 4g: learning (it needs real-run data, which the beta collects)
- 4h: background crews
- Premise check, dependency splitting, cheap-draft escalation
- Publishing the spec

## Worker confinement (macOS)

Crew runs on macOS in the beta; Windows later. On macOS each worker's engine process starts under a
profile that denies reading or writing Orbyn's data folder (store, service handle, other runs' files),
except its own run folder, checkout and library, and stops it starting programs through LaunchServices or signalling other processes. A worker's own file
access to Orbyn's data folder (store, `service.json`, other runs' files) is denied. It does not stop a worker
reading other processes' command lines and environment, so nothing secret is passed there: a lead's MCP endpoint
token is read from an owner-only file in the lead's run folder, never from arguments or environment. Where that is not possible the worker is refused with a plain line. See
[the architecture](architecture.md#crew-worker-confinement) and SPEC section 11.

A Claude worker also cannot read or write another session's saved Claude conversation (the lead's transcript
under `~/.claude/projects`), nor Claude's prompt history; it keeps its own session, which a first turn names with
`--session-id`. It starts without Claude's auto memory for the project (`memory/` is closed with the rest). A worker on
another engine (an ACP agent) gets the whole folder closed with no session; an ACP adapter that wraps Claude Code may be
unable to run as a worker until it can take a `--session-id`. The core makes `projects` and the project folder before launch and the worker can create nothing there but its own
session. A configuration folder, `projects` or `history.jsonl` that goes through a link, or a start folder whose Claude
name is not certain (non-ASCII, very long), refuses the worker.

Limits on this, by ruling: a worker on any other engine (an ACP agent; Codex is refused) gets Claude's `projects/` and
`history.jsonl` denied with no allowance, so an ACP adapter wrapping Claude Code cannot be a worker for now; auto memory
is closed to workers, which start without it; a start folder that does not resolve, is non-ASCII or 200 or more characters,
or a symlinked `~/.claude`, `CLAUDE_CONFIG_DIR` or any folder above it refuses the worker with a plain line. Codex's own
session files (`~/.codex/sessions`, the SQLite state and logs under `~/.codex`) are not covered, which is acceptable
only because `CODEX_WORKERS_ALLOWED` is false and stays so until a separate probe is done. A confined Claude worker can create its own `<own id>.jsonl` and `<own id>/` in any existing project folder, not only its own project's, so a session it wrote can appear in another project's resume list. The claim is narrow: the Claude
lead's transcript is denied to confined Claude workers, and nothing is said about other leads.

Built: the macOS profile, the refusals (other platforms, missing tool, nested restriction, Codex, terminal
engines), the Claude transcript denial and tests that run the real tool. Not built: a Windows or Linux restriction,
and a denial of other engines' saved conversations. Not covered: an ACP agent's or Codex's saved conversations (Codex
also keeps them in SQLite files), Claude's other per-session files (`plans/`, which can hold the lead's plan text, `paste-cache/`, `session-env/`, to-do
lists, file history, debug logs, shell snapshots, `sessions/`), the network, the loopback service, the keychain, `~/.claude.json`, other same-user files
and processes a worker leaves running. A worker can learn which project folder names exist under `projects` (their metadata may be read), not what is in them. The lead's Claude transcript is closed for a Claude worker; the answer key as a
whole is not claimed safe. Codex workers are refused until the nested-profile question is answered, and Codex's
conversation files are a second reason to keep it so.

## As built (cancel)

The person's stop button for a Crew task (Preview). Backend only: the card action that calls it is Engineering 1's.

- **What exists:** core `Core::crew_cancel_task(task, reason, actor)` and `POST /api/crew/tasks/{id}/cancel` (body empty, or
  `{ "reason": "..." }`, 1 to 200 characters, redacted). The answer is `{ task, state: "cancelled", already }`. With the Preview flag off the
  route is 404.
- **Person only, as far as it is enforced.** *Enforced:* no MCP tool and no crew tool cancels a task; a remote viewer is refused
  (`local_only` is the whole-process `--remote` flag, 403); Autopilot's N1 refuses an agent's request to the route by the *command
  text* it sees (path spelling variants are covered, it is in `crew::changes::STATE_CHANGING`); a confined macOS worker cannot read the
  launch token. *Not enforced:* an unconfined engine, the lead, or any process of the same user that has the launch token (from
  `service.json`) can `POST` it, and a path assembled at run time is invisible to N1; the audit row says `user` whoever sent it. If real
  enforcement is wanted later, approve-once's desktop grant is the model. This is not an Autopilot stop: `GET /api/crew/stops` lists
  Autopilot's, and a cancel is the person's own act.
- **Answers.** An unknown id is `not_found`. A task already `cancelled` is `already: true`: no second row, no second effect. A task that
  `passed` or `stopped` is `409 task_ended` ("This task already ended."). Every other state (briefed, routed, running, verifying,
  failed, changed_approach, needs_person) can be cancelled, as the lifecycle table says.
- **Order, failing closed.** (1) One transaction checks the task and writes the intent row `crew.task.cancel` (actor, source, task id,
  previous state, whether a verification was claimed, the reason): the row comes before every effect (architecture rule 6). (2) The
  worker's engine is stopped through `RunManager::force_stop`, which also kills the engine's process tree and closes a run that will not
  stop; its queue is emptied first, so no new turn starts, and the core checks the worker has no run and nothing queued. (3) One
  transaction moves the task to `cancelled` (clearing a verification claim) together with the row `crew.task.cancelled`; then the
  existing end of a task runs (leases released, a final `cancelled` result, the lead told).
- **Two cancels of one task** run one after the other (a lock per task): the second finds it cancelled and answers `already: true`
  with no row. If the service dies after the intent row, `crew.task.cancel` stands with no outcome row; the state is unchanged and a
  retry works (it writes its own intent row).
- **Races with a start or a retry.** A task cannot get a worker once it ended (`set_crew_worker` refuses in its transaction, the thread
  just made is removed and no engine starts), and a retry prompt is not queued for an ended task (the state is read again before and
  after queueing; a message that slipped in is taken back and its run stopped).
- **If the engine cannot be confirmed stopped,** the task keeps its state, a `crew.task.cancel_failed` row records the attempt and the
  caller gets `502 engine_unavailable`. A retry is a new attempt with its own intent row. A task that ended some other way during the
  stop (it passed, say) is `409 task_ended`, with a `cancel_failed` row.
- **Verification.** Cancelling a `verifying` task clears its claim. A verification still running stops before its next check and cannot
  settle the task: `passed`, `failed` and the loop guard refuse a cancelled task.
- **No merge.** Only a `passed` task is queued for merge, and a cancelled one is refused there (tested).
- **Not built:** the card action (Engineering 1); reassign, message into a lane, add a lane and a person's answer, which are out of the
  beta. No new event on the bus or the Autopilot ledger: the audit rows and the final `cancelled` result are the record.
- **Known limits.** A cancel cannot undo what the worker already wrote in its worktree, and commits already merged stay merged; the
  worktree itself is left as the existing end of a task leaves it. A check command already running when the cancel lands finishes
  first (its result is discarded). The stop waits up to four seconds for the engine before it closes the run by force.

## Open risks

- Steering differs by engine. A Claude message may land mid-tool-call. Test with real engines before claiming "live".
- Login and quota are per engine on the person's machine. The router must fail clearly ("Codex isn't signed in") rather
  than silently falling back.
- Cost: several engines at once multiply usage. A crew-wide budget is in slice 3, not later.
- openbot is non-commercial: no code or text from it, only the ideas listed above.

## Appendices

The row tables are in [crew/beta-rows.md](crew/beta-rows.md):

- [Appendix A](crew/beta-rows.md#appendix-a-every-unfinished-beta-row-app-parity): every unfinished beta row (app parity), 137 rows.
- [Appendix B](crew/beta-rows.md#appendix-b-designed-screens-not-yet-built): designed screens not yet built, 160 screens.
- [Appendix C](crew/beta-rows.md#appendix-c-unfinished-rows-that-have-no-design-yet): unfinished rows that have no design yet.
