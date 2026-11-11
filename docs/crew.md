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
| E1 | Tag the workspace (`engine-v0.1.0` first) | The app pins the tag |
| E1 | Keep staging working for invite-only sign-up on a desktop client, not just the Planner web app | A new invited person signs in from the app against staging |
| E2 | A new account module in `orbyn-core` built on the `orbyn-device-client` trait, against a fake server until the crate lands: session, device keychain, trusted-device approval, a sync worker per zone, status events, and the D10 threat model | Unit tests with a fake server; no password in any request body |
| E2 | Settings, Account for real (sign in, devices, sync status, sign out), replacing the preview-only sign-in screen. The other preview screens stay behind the flag | Built from merged Account frames (check with the Designer first) |
| E2 | Offline-first behaviour: queue changes, show sync state, surface conflicts, never block | A test cuts the network mid-sync and the app keeps working |
| E1 + E2 | A cross-repository end-to-end test: sign in against the Engine's compose stack, change a setting on machine A, see it on machine B | It passes in the app's CI on affected paths |

**Engine order (E1):** (a) design note, `orbyn-engineering` crate and `docs/engineering.md`; (b) `orbyn-device-client`
with a fake-server test; (c) the device `app` label, Engine issue #73 and tag `engine-v0.1.0`; (d) a desktop-client run
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
  - The bus never starts a turn or writes a thread's queue. Text is handed only to a running turn that takes input
    (`live`, shown under the role `crew`); otherwise it is `held` and comes with the next prompt or in the `inbox` of
    the recipient's next bus tool result. A worker being verified is never handed anything. A message nobody ever
    collects stays `held`. Only fake engines ran, so "live" is not tested against real Claude or Codex, which may
    read it after the tool call it is in. A board write counts as a post, and a key has 16 revisions.
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
   waits for the person's review.
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
| W4 | Secret redaction | Secrets are masked in terminal blocks, in context sent to agents and in the Crew ledger. Matches known key formats and the person's own patterns | Feeds Crew's hard rule "no secrets in any object" |
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
