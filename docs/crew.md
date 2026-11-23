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
- [Kiro Crew features](#kiro-crew-features)
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
| 5 | Parity rows ([Appendix A](crew/beta-rows.md#appendix-a-every-unfinished-beta-row-app-parity)), unbuilt screens ([Appendix B](crew/beta-rows.md#appendix-b-designed-screens-not-yet-built)), rows with no design ([Appendix C](crew/beta-rows.md#appendix-c-unfinished-rows-that-have-no-design-yet)), the Warp features in the beta cut | E2 + Designer | Each row's frames merged. Parts C to F are drawn in Engineer Designs (`artifacts/engineer-designs.html`); the part C and D decisions are recorded below, and the part E and F rulings are in the designs |
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
- **Workers**: threads on the chosen engines. Planned: each writing worker in its own worktree; today they run in the lead's checkout (see "Isolation"). Depth is 1: workers cannot start workers.
- **Crew bus** (Rust): owns runs, messages and the shared board.
- **Verifier** (Rust): runs the answer-key checks in the worker's checkout (the lead's, until worker worktrees are built).

**Models and effort per role.** These come from the repository's harness (`harness.py resolve`: library defaults, then
the owner's file, then the repository's `.orbyn/harness.json`). Effort is medium for every role and never higher.

The default tier words were checked against a model list captured from each real install, sanitized to the ids and labels: a [Windows Claude Code 2.1.317 list](../crates/orbyn-core/src/crew/tests/claude-2.1.317-windows-models.json) and a [Windows Codex 0.160.0 list](../crates/orbyn-core/src/crew/tests/codex-0.160.0-windows-models.json). The Claude list has `opus`, `sonnet` and `haiku` entries and dated ids such as `claude-opus-5`; its own `default` entry names no tier, so the lead is the `opus` alias. The Codex list has Sol, Terra and Luna; Astra has no tier word. Both defaults already matched, so no word changed. A test in [model_ids.rs](../crates/orbyn-core/src/crew/tests/model_ids.rs) fails if a word stops matching an observed id or starts matching one it should not. Each list is one account's catalog on one day, not a guarantee for every account or later CLI (#433).

| Role | Claude | Codex | Notes |
| --- | --- | --- | --- |
| Lead (briefs, approves in Autopilot, merges) | Opus | Sol | The engine the person picks for the lead thread |
| Router, verifier, merge queue, watch on main | No model | No model | Code |
| Builder | Sonnet | Terra | Complex work on Codex goes to Sol (harness `engineer_complex`) |
| Review lane (`crew_review`) | Sonnet | Terra | Worker tier by default, on the other vendor from the builder. The `security` lane, locked lanes, the `correctness` lane a merge is owed (until a merge entry says low risk) and any lane only the builder's own vendor can take stay on the lead tier |
| Code review (a review brief) | Opus | Sol | Lead tier, on the other vendor from the builder by default |
| Security review and finding check | Opus | Sol | Lead tier always; never lower |
| Second key (Autopilot) | Opus | Sol | Must be the other vendor from the lead |
| Scout (search, mechanical checks, cheap drafts) | Haiku | Luna | Escalates to the builder tier on failure |
| OpenCode and other ACP engines | The person's model setting | The person's model setting | The person maps each to lead, builder or scout tier in Settings, Crew; unmapped engines are offered only as builders |

The route card shows the tier, model and effort chosen, and why. A repository's harness file can only lower cost, never
raise a tier. Only the person can change a lead tier.

To pin a worker's model (for example a Haiku-only run), the lead sets `engine: { id, model, assigned_by }` in the brief
(`"model": "haiku"`). A review lane defaults to the worker tier (Sonnet or Terra), so it costs less than the lead. The `security` lane,
any lane a repository rule locks (`"locked": true`), the finding check and a second key stay on the lead tier, and a pinned model that is
below the lead tier is refused for them, never swapped. The `correctness` lane (`CODE_LANE`), which a medium or high risk merge is owed,
stays on the lead tier too: risk is only known once a merge entry exists, so only an entry that says low frees it, and with no entry it stays
lead. When the rules cannot be read, or the work has no base commit, a lane counts as locked. When every reviewer the router admits is the
author's own vendor (a single-vendor machine), the lane keeps the lead tier instead of lowering the only review available. A model pinned in the brief's
`engine` wins over the lane default in either direction. An engine with no tier mapping (OpenCode before Settings maps it) is a builder only,
so it never takes a lane. The reviewer is still another vendor than the builder where one is configured. The route card of a lane carries
a `review_lane_tier` reason, and the `route` in a `crew_start` or `crew_review` answer reports its `tier`, `model`, `reasons` and `rejected` (the engines left out, each with its reason).
`crew_route` (dry run) shows the engine, model, tier and role before anything runs; the `route` in a start answer reports what has already started.

**Router v0**

1. Hard filters: installed, logged in, compatibility not `Broken`, and `managed` plus `approvals` for any brief that
   writes. The brief's `needs` (for example `image`, `mcp`) must match capabilities.
   - **One source for the state facts:** the first three checks and the write check read `ProviderFacts`
     (`agents/provider_facts.rs`), the same facts the Settings Providers list reads for its Not found, Can't start
     (a `Broken` version) and Unmanaged (no `managed` or no `approvals`) labels, and for the Signed out state.
     A test runs every combination of those facts through the router and the list and fails if they disagree.
     A sign-in that could not be confirmed has no designed label, so the list shows none for it while the router
     still refuses. Limited support (an `Unsupported` or `Graceful` version) is list-only and never keeps an engine out.
   - **A worker run that would be refused is never routed to.** The run start refuses an engine that cannot take a crew
     task (a terminal engine, Codex while `CODEX_WORKERS_ALLOWED` is off) and any machine with no restriction to apply
     (`host::confine::worker_refusal`, the one function both read). The router reads its answer when the machine is
     gathered and filters on it, after the checks above, with a plain reason per engine ("Codex can't run Crew workers. ...
     Choose another engine for this crew, or do the work in a normal chat."). A brief that pins such an engine is refused
     with that reason and never swapped. When no engine is left, `crew_start` refuses up front with the reason and stores
     no task, lease, run or ledger row. This covers engine and platform refusals only: the preflight routes with no data
     classes, so a data-class rejection is still recorded after the task is stored, as before. A review lane whose engines
     are all refused takes that same later path. Approver runs are not confined, so they do not use this filter. `crew_route`
     shows the same rejections.
2. Provider, then score. With no engine named and a non-review role, the lead's own provider (engine family) comes
   first (`RouterConfig::worker_provider`, the per-project **Worker provider** setting: `same_as_lead` (default) or `any`, where `any`
   lets every allowed provider compete on score, still after the refusal and platform filters). It is stored per project in the existing
   `project_preferences` table, key `crew_worker_provider` (no migration); absent, damaged or unknown text reads as `same_as_lead`, and
   saving `same_as_lead` clears the row. `GET /api/crew/projects/{id}/worker-provider` reads it; `POST .../worker-provider/set` with
   `{"worker_provider": "same_as_lead" | "any"}` saves it. The write is a Person route behind the Crew Preview gate and `local_only`: it needs the
   person key (desktop only, so browser mode and the CLI get `403 desktop_required`), writes a `crew.worker_provider.set` audit row in the same
   transaction, and no MCP tool reaches it. `crew_start`, `crew_route` and review routing read it for the project of the lead's thread; a task
   already routed keeps its engine. Settings > Crew has the row (frames `settings-crew-worker-provider`, `-any`, `-narrow`): a Same as lead / Any provider choice for the project of the checkout Settings shows, saved through the route above. Under `same_as_lead` another provider takes the work only when the lead's is refused or not available, and the route says so
   (`lead_provider_unavailable`: "Codex workers are not available here, so Claude takes this."); otherwise the route
   carries `lead_provider`. Review lanes, second keys and finding checks keep the other-vendor rule and ignore this.
   Inside the chosen provider: score. Pass rate for the brief's `kind` from Eval history (seeded defaults until there is data), quota headroom from
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
    task (`engine_error`). `budget.minutes` ends a task even mid-turn. `budget.turns` is enforced on the next failed attempt:
    the worker's turns and the approvals reserved against the same budget are counted (`crew_turns_spent`), and once they reach
    the brief's turns the task stops with the same `budget` reason and a result that counts them; a turn that passes on the last
    one still passes. With Autopilot engine approvers each approval reserves one of the brief's turns (`reserve_crew_approval`):
    with the default 30 turns a task gets about 14 two-key actions (two reservations each), and every later action is denied.
  - **A crew-wide turn budget** (owner ruling, 2026-11-20) is a device-local number of turns
    (`crew_turn_budget`; unset or 0 is no crew budget), set in Settings › General (`Crew turn budget`). A crew's spend is the same per-task tally summed over its
    tasks, so the crew's budget and a task's own cannot disagree. Once the crew's turns are spent, a new task is
    refused with `not_authorized` and "The crew has spent its turn budget, so no new task can start."; nothing is
    stored, routed or sent. A task already running keeps its retries, approvals and verification, and may spend past
    the number. There is no reset: a new crew starts at zero because its count is read from its tasks. Every new
    task counts, including review lanes and fixes, except Autopilot's merge corrections, which create their task
    directly (deterministic, no worker turns). A task whose worker thread exists or whose first prompt is still queued
    counts at least one turn.
  - The worker runs in the lead's checkout through `delegate`. A worktree of its own is not created yet.
  - **Check kinds this version runs (#437):** besides the command kinds, `file_scope` and `diff_size`,
    `schema`, `secret_scan` and `repo_required` run in the checkout, and `http_probe` sends one
    request, each recording its Evidence:
    - `schema`: `{schema, instance}` name two files in the checkout. The instance must be JSON that validates
      against the schema (draft 2020-12). A mismatch, or an instance that is not JSON, is a finding (fail) that
      names the instance locations and never the value; a missing or unreadable file, a schema that is not JSON,
      and a schema that references another file or a URL — the schema is one self-contained document, so only
      `#` references are allowed — are errors, never a pass.
    - `secret_scan`: reads every changed path (as `file_scope` and `diff_size` read them) for the secret formats
      of the spec, all of them and always. The Evidence names the path, line and class, never the match. A changed
      file over 8 MiB, or one the scan cannot read, is an error: the scan never passes over what it did not read.
      Symlinks and other non-regular changed files hold no bytes of the change and are not scanned.
    - `repo_required`: runs the repository's own checks from `.orbyn/crew.json` (`checks[].run`), in order, each
      passing on exit 0. The file is read at the commit the task started from, as the merge queue reads it, so a
      worker's edit to that file cannot lower the bar. Every task that writes inherits one such check at its start
      (`chk-repo-required`, named `inherited` in the log), so a lead need not declare it and a worker cannot remove
      it: a key that already runs the repository's checks keeps its own, a review task runs none because it writes
      nothing, a repository that declares no check adds none (a checkout with no commit has nothing declared), and a
      file that cannot be read refuses the start. The
      commands still run in the checkout the worker left, so the scripts
      and tests they call are guarded only by the key's `protected_paths`. A task with no recorded starting commit is an
      error. `name` and `note` are labels for the log. No declared
      checks is a pass with that fact in the log; a failing check is a finding that names the check, and a check
      that cannot run is an error. `when` is not implemented: a check that declares one is refused with that reason,
      by the verifier and by the merge queue alike, so an unread condition can never pass for a condition that was
      met.
    - `http_probe`: `{url, expect_status}`. `url` is an `http://` URL on `127.0.0.1`, `localhost` or
      `[::1]` and `expect_status` is 100 to 599, so the probe never leaves the machine. One GET: the
      expected status is a pass, another status is a finding that names the answer observed, and a
      request that got no answer (refused, timed out, a broken connection) is an error, never a pass
      over what it did not see. No redirect is followed, so the status compared is the one that URL
      answered, and the request is sent once, with no retry. It has no timeout of its own: it runs
      inside the budget every check gets, and a value shorter than that is the key author's
      decision. Nothing here starts the service a `url` names; the key's author does, before
      verification.
  - `manual`: the check a person makes. No command runs: the check parks, the task holds in `needs_person`, and the
    person passes or denies it (`POST /api/crew/tasks/{id}/checks/{check}/decide`, a `Person` route). Their answer is
    recorded as their own `checks`-stage Gate and the task goes back to `verifying`, where the check passes or fails from
    that answer. A task never passes while one of its manual checks is still waiting. `name` and `note` are labels.
  - **Check kinds this version does not run** (`preview`, `visual_compare`, `screenshot_only`, `accessibility_audit`,
    `performance_budget`, `licence_scan`) report `skipped` and are listed as not verified. The page kinds need the app's
    browser; `licence_scan` needs a licence policy and an offline source of dependency licences. A key with nothing it can
    run goes to `needs_person`.

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
    exchange and post limits through `bus: { exchanges, posts }` in protected `.orbyn/crew.json`.
    Start validates it; each counted write reads committed HEAD and the accepted default branch of the
    lead's checkout, taking the stricter limits without changing the host ceiling or another project.
    Changes apply on later operations, including active tasks; uncommitted edits do not. Existing counts
    survive refresh and restart. Invalid policy refuses writes, while read tools remain available.
    See the [bus rules](../crates/orbyn-core/src/crew/bus/README.md#rules-to-keep) for bounds and failures.

**Isolation**

- **Planned:** a writing worker gets its own worktree through the existing worktree handoff.
- **Today:** workers run in the lead's checkout. There is no merge gate for a single worker, and diff checks see the lead's uncommitted changes too.
- The brief's file lease prevents two writers on the same files, and a worker's file-change tool outside it is refused
  at the gate with the lease's own words, before the write; the verifier's `file_scope` is no longer what catches it. A
  shell command is not read, so a command can still write outside the lease (issue #441's sandbox). A refused write is
  recorded as a denial and counts toward the trip wires (3 in a row, 20 in a run).
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
    never on a cheaper tier. So do locked lanes. Every other lane runs on the worker tier unless the brief pins a model.
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
  `crew_check_finding` and `crew_review_status`, behind the Preview flag. A `crew_review` call starts lanes while the
  tool call's own deadline leaves room, and is then an error listing the lanes that started, those waiting on a person,
  those refused and those not reached, with the `lanes` to ask for again. The lanes the repo's rules ask for are added on
  every call, so asking again re-runs them. See the
  [review README](../crates/orbyn-core/src/crew/review/README.md).
- A finding is confirmed by code: a repro check the lead supplies that fails, or an independent second review on the
  other vendor that reports the same file within 3 lines. Otherwise it is unconfirmed and blocks nothing. Both
  decisions are Gates at `finding_check` with Evidence the core wrote.
- A confirmed finding sends a fix brief to a builder (its key includes a check that the file changed), the lanes run again on
  the fix, and a finding shows as fixed only after such a fix passed and a repro has proved it: a finding confirmed by agreement has no repro, so it keeps
  blocking until the lead supplies one that passes after the fix (or a person clears it). A reviewer's silence clears nothing. One fix per lane and file,
  at most three per task. Reviewers are read-only by bytes, and the second review needs another vendor than the first.
- **Review integrity** (behaviour changes to know about):
  - Sparse checkouts are refused when a task starts. Admission and every verification read the Git index, and any
    `assume-unchanged` or `skip-worktree` entry, including the ones a sparse checkout sets, refuses the start or fails the
    check, because such an entry can hide an edit from the scope and size checks.
  - The changed-file check errors when any index flag is set, whether or not the flag was there at the start. A task
    that began under the old guard (which only failed flags added since the start) now fails verification on an existing
    flag. Re-run that task after clearing the flag.
  - The scope, size and changed-path checks read Git without line-ending conversion or the untracked cache, so those settings
    cannot hide a byte change. Other Git reads keep their normal settings.
- #450 adds source contracts for person once consent, a persistent same-engine route badge, per-lane code-owned
  `review` Gates and reasoned unlocked-lane removal. Migration 46 follows forge migration 45; launch and merge admission
  are covered by focused fake-engine, store and HTTP tests. The live Flow task includes the reasoned removal dialog;
  Settings, Crew includes an explicit device-local ACP tier editor with whole-map revision checks and default-model pins.
  Both controls reconcile uncertain writes before a retry; locked lanes cannot be removed by anyone.
  A copyable installation-local task locator, `orbyn://crew/task/<encoded-task-id>`, opens the existing read-only Flow task
  when Preview is on. It conveys no consent or action and needs that installation's task data. It is not a public report;
  forge clickability and native delivery remain unqualified. Lead-requested advisory PR-comment publication is integrated
  through existing Network policy and two-key admission, covered by fake-protocol, recovery and gate tests.
  Mock-backed Chromium checks cover the controls, native-select Escape behavior, dirty navigation and narrow Light, Dark and Dim layouts;
  real engines and actual forge publication remain unqualified.
  `crew_finding` refuses after the reviewer's run has ended (H7); the Gates are core writes and do not.

**Known limits of the review stages (#451, 2026-11-20).** Every limit the slice 4b review rounds named now has a decision.
A fixed item names its test; an accepted one says why and what would close it. Implementation notes and the decision table
stay in the [review README](../crates/orbyn-core/src/crew/review/README.md#decisions-for-451).

- **A reworded claim is a new finding — accepted.** The fingerprint is the lane, the file and the claim with case and spacing
  ignored, so only the label uses it. Blocking and the one-fix-per-place guard keep to the lane and file: a paraphrase never
  clears a confirmed finding and a silent lane clears nothing; a fixed place can read as "fixed" plus a new report. Exact
  semantic identity (compare by file and nearby line as the second review does, or let the lead mark two findings the same)
  is a product decision with no agreed rule. Unblocked by that decision; `the_same_place_twice_stops_the_loop` and
  `a_fingerprint_ignores_the_line_the_case_and_the_spacing` cover today's rule.
- **A reviewer shares the lead's checkout — blocked on #436.** Read-only comes from the lease, the protected-byte comparison
  and the scope check; no process or filesystem isolation exists. A reviewer's own checkout at the exact commit needs #436.
  Unblocked by #436.
- **The second blind review lane costs a whole lane run — accepted.** A finding without a repro is checked by an independent
  run on another vendor, and a cheaper check anchored on the claim would break that independence. The cost is unmeasured
  because it needs real engine runs; only fake engines are tested here. Unblocked by a real-lane measurement.
- **A finding outside the reviewed task's own scope — closed.** A fix inherits the reviewed task's checks and writes only the
  finding's file. A finding outside the task's own `file_scope` checks now sends no fix brief; the lead is told a person must
  authorise a new task (`a_fix_outside_the_tasks_scope_checks_is_refused`). Check-defining manifests, tests, CI, rule files and
  protected paths stay refused. The residual is accepted: a fix never widens the reviewed scope.
- **Real Claude and Codex yield — unqualified.** Only fake engines ran, so no real lane has shown how often reviewers file
  findings the check confirms. Unblocked by real-lane runs on signed-in engines, which need a live-run decision rather than
  code work (#451).
- **Hidden paths and index flags — index flags closed; ignored new files accepted.** Any `assume-unchanged` or
  `skip-worktree` entry, sparse checkouts included, refuses admission and fails verification whenever it appears. A new file
  hidden by `.gitignore`, `.git/info/exclude` or `core.excludesFile` stays invisible to the Git-based scope and size checks:
  counting every ignored file would charge pre-existing caches and local data to a narrow task. A crafted `.git/index` with
  matching file times, a `.git` file pointing at another folder and a writable `.git` are the same class. Unblocked by isolated
  checkouts (#436) or a task-start byte baseline; `ignored_new_files_remain_an_accepted_blind_spot`,
  `an_index_flag_does_not_hide_a_reviewers_edit` and `a_flag_that_was_there_at_the_start_refuses_admission` cover today's
  behaviour.
- **The clearing repro runs at the current checkout — blocked on #436.** It must pass now, is not shown to have failed before
  the fix (the Gate says so), and its Evidence records the commit it ran on. Fail-before/pass-after at the finding's exact
  commit needs a throwaway checkout and process ownership in #436. Unblocked by #436.
- **Byte-exact Git is scoped to the checks that decide — accepted.** Crew's scope, size and changed-path checks run
  `git_byte_exact` (line-ending conversion and the untracked cache off) and refuse a nonempty or unreadable `info/attributes`;
  `git_hardened` keeps its previous behaviour for every other caller. Adding the flags to every hardened read would change
  file explorer and merge reads too, and real Windows `core.autocrlf=true` behaviour is unverified ([handoff](handoff.md)).
  Repository metadata remains writable. Unblocked by auditing every caller and recording Windows evidence;
  `autocrlf_cannot_hide_a_byte_change_from_scope_or_size` and `only_the_byte_exact_helper_carries_the_crew_flags` cover it.
- **The touched guard and HEAD reads — closed.** `chk-fix-touched` is a core byte comparison in the immutable key, run only for
  a stored fix task, never a shell command; a missing, unreadable or nonregular file cannot pass. The two plain
  `rev-parse HEAD` reads the issue named use `git_hardened`, in `review/lanes.rs` and the start path. Covered by
  `a_fix_with_spaces_and_valid_punctuation_gets_a_core_byte_check` and the verifier tests.

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
    `when`; a check's `when` is refused until it is read, so it never passes unenforced).
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

- Parallel branches are merged in order. Each candidate is merged onto the branch tip in a scratch checkout, one at a time, and
  its required checks run again on that combined state before main moves.
- Conflicts are forecast from the dependency graph before work starts, and the router orders tasks to avoid them.

**After-merge Review queue (the person's stage):**

- Every merge to main is one item, ranked by risk. Each item has the diff, the proof (checks, reviewers by vendor,
  findings fixed), what was not verified, and the cost.
- Actions: "Looks good", "Comment" (becomes a fix brief), "Revert" (the crew opens and verifies a revert).
- **Watch on main:** if main's CI or a post-merge check breaks, the crew finds the merge that caused it by bisecting the
  queue's merges. For a local merge it reverts automatically when the cause is certain. For a forge merge it prepares and verifies a corrective PR and asks the person to choose Revert, because only a person's answer publishes to the forge; otherwise it asks the person.
- **Daily digest:** what merged, what is waiting for review, what was reverted.

**As built (slice 4f, part 1):** the always-denied list N1 to N10 (including the account routes) in code, the approval
chain with a lead and a second key on another vendor, trip wires, the ledger and the mode (`off`, `on`, `full` by
default), all behind the Preview flag and with no screen. The lead and second key are an `Approver` trait, not yet backed
by a live engine. See the [Autopilot group](../crates/orbyn-core/src/crew/autopilot/README.md).

**As built (slice 4f, part 2):** diff risk tiers, the merge queue (one merge at a time, checks again on the combined state), the
after-merge Review queue and the watch on main with a bisect revert. Local only: a merge or a revert moves a local branch that
no one has checked out, and nothing is pushed or sent to a forge. Merges are `--no-ff` merge commits rather than rebases, so
a revert is one command and a bisect walks first parents.

**As built (slice 4f, conflict forecast):** the merge queue's forecast is no longer a constant. For up to eight
queued local candidates, in queue order, the read dry-runs the candidate's merge onto the branch tip with a hardened
`git merge-tree --write-tree --name-only --no-messages` (output capped, no ref, index or worktree moves) and reports the task,
its candidate commit and at most 20 conflicting paths; `checked` says how many candidates the dry run covered and
`conflict_forecast` is true when at least one would conflict as the branch stands now. A candidate whose repository or commit
cannot be read locally, and a forge intent (its merge is tried on the forge), is left out rather than guessed at. This is a dry
run at read time, not the dependency-graph forecast the plan draws: it says that a merge would conflict now, not that two
tasks overlap before work starts.

**As built (slice 4f, bisect progress):** the watch read serves the bisect a watch is running now as `bisect`; the old
`bisect_progress` boolean is gone. It carries `phase` (`checking_tip`, `checking_boundary`, `bisecting`, `confirming` or
`checking_parent`), `tested` and `steps` (the runs finished and the most this history can need), `after` and `before` (the
last commit known to pass and the first known to fail, while bisecting), `testing` (the commit whose checks are in flight) and
`checked` (each finished run and whether the checks passed there). Each entry is a real run of the repository's checks at that
commit. It is in memory only, cleared when the watch ends, so a restart keeps the stored outcome (the merge rows and the
ledger) and shows no stale bisect.

**A local merge a crash interrupted.** A queue write names the state it expects (a compare-and-set), so a person's reject or a recovery is
never overwritten: a changed row is left as it is and each caller decides (the lane skips the entry; a reject of an entry already
`merging` is refused). A crash while a local entry is `merging` would leave it so for good, so the watch's first tick at startup, and each tick
after it, settles such entries from git (passes of at most 16 entries, 30 seconds of git each, repeated until one moves nothing, at most 8 passes; the
each recovery pass (startup and every watch tick) waits for the merge lane's lock, which a live merge holds, so it never touches one, and a pass can hold the lane for up to about
8 minutes). The watch has no periodic tick unless its interval is set, so the startup pass is the one that counts by default. The branch moves by one atomic `update-ref`, so: the branch holds the queue's own merge commit for the candidate
(the merge is recorded, flagged for a person to look at); or the branch lacks the candidate (nothing landed: the entry is `queued` again with a
note, and only the lane's own pass runs it; a scratch checkout the crash left is left for git's `worktree prune`); or git cannot say (unreadable repository, a branch that holds the candidate without a merge the
queue made in its last 256 commits, no answer in time): the entry is `waiting` with the reason, for a person. The lane rejects an entry whose candidate the branch already holds ("already on the branch"), and refuses to move the branch to any commit that is not a merge of exactly the tip and the candidate, so a released entry of that kind never records a merge. A forge entry is never read here;
its intent has its own reconcile above.

**Forge CI rule.** On a forge merge, every check in the person's `ORBYN_CREW_REQUIRED_CHECKS` list must be present on the pull request's
head and succeed; a required check that is missing, skipped, neutral or pending is not green. Any other check may succeed, be skipped
or neutral (a repository's path-scoped jobs are often skipped), but never fail, be cancelled, time out or still be pending. A failed
combined-state check blocks the entry for a person rather than re-running. The `forge_publish_requested` and `forge_merge_requested`
events record the `actor`: `lead` for the lead's tool, `local_caller` for the local HTTP caller.

**A forge merge that needs a person.** The lane skips an intent in `needs_person` and goes on with the next (each entry pins its own
base, so no later entry depends on it). The pipeline never leaves `needs_person` itself and never sends a merge request twice. An
intent that got there before any merge request was sent has nothing in flight. One that got there after a request may still have it
on the forge (a merge queue); it cannot be retried, and a reject keeps it reconcilable so a later merge is still recorded. A PR merged
by hand is read from the forge and recorded (audit `via` "lane reconcile"; for the lead's own crew only). A person answers with
`POST /api/crew/autopilot/merge-queue/{id}/resolve` (`{action: reject | retry, seen_revision}`; Person route, key enforced, local only,
audit row first, `409 stale` for an intent that moved): `reject` drops it, `retry` sends it back to `awaiting_ci` (or `prepared` when
nothing was published) so the guards run again, and is refused after a merge request, after an unclear publication and after the base
moved. The forge is read first, with a search for the PR when none is recorded. The queue view shows each entry's forge `phase` and
`revision`. See the [Autopilot README](../crates/orbyn-core/src/crew/autopilot/README.md#merge-queue-review-queue-and-watch-on-main).

**As built (slice 4f, HTTP):** `orbynd` serves Autopilot's views and the person's few answers under `/api/crew`, all behind the Preview
flag. With it off every route answers 404 for every method, like a path nothing serves, and `/api/info` omits `crew_preview`. The flag is
read, never set, over HTTP. **The Preview flag stays while Crew is unfinished, not to keep a worker out:** every route that changes state needs the
person key only the desktop shell holds (`docs/worker-token-isolation.md`, step 3), so a worker that read the service token
gets `403 person_required`. What [#441](https://github.com/orbyn-dev/orbyn-engineering/issues/441) still holds is the Windows
sandbox and the store read.
Reads: `GET /api/crew/preview`, `/autopilot` (modes, `confirmed`), `/autopilot/guards`, `/autopilot/risk/{task}`, `/autopilot/review`,
`/autopilot/review/{task}/revert-check` (later merges), `/autopilot/merge-queue` (with what goes next), `/autopilot/watch`,
`/autopilot/digest?hours=`, `/autopilot/digest/schedule` (from issue #806: the stored send time), `/approvals` (the ledger, newest first), `/stops` (trip wires), `/trust` (revert rates by area and engine; `?project=` limits it to one repository) and, from slice 4j, `/autopilot/areas`, `/autopilot/areas/history` and `/autopilot/thresholds`.
Lists take `limit` (1 to 200) and `offset`. Changes, each through the core function that holds the rule and local only:
`PUT /autopilot/mode`, `POST /autopilot/confirm`, `POST /autopilot/review/{task}/{looks-good,comment,revert}`, and, from issue #806, `POST /autopilot/digest/schedule/set`.

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
with the redacted command, its working directory and the flags that change what runs (the answer key's text withheld; a newline, a bidi or an
invisible character is shown as a visible mark; a hold whose text is not the command the engine would run, or is too long to read whole, cannot
be approved). The person answers from the app's window, local only: `POST /api/crew/approvals/{hold}/approve-once`, `/deny` and `/dismiss`.
**No agent approves; a person may approve once.**

- **One command, once.** An approval is bound to the command text, its paths, the worker's thread, the task and the engine's whole request:
  every field of its parameters except the engine's own bookkeeping (`itemId`, `turnId`, `threadId`, `startedAtMs`, `approvalId`, `callId`,
  `conversationId`, `reason`, `description`, `title`, `toolCallId`, `sessionId`, `commandActions`, `parsedCmd`, `timeout`), which changes from one
  request to the next and made a real retry never match. A field that list does not name (the working directory, the arguments, the environment,
  an effect flag, a Codex `kind` or `environmentId`, a key an agent adds) is bound, so a retry that changes, adds or drops one gets nothing. `command`
  is bound as the engine sent it, an argv array is never joined (`["echo","a b"]` and `["echo","a","b"]` are different), and an array with a word
  that is empty or holds a space is not approved once, because the text a person reads could stand for either. An ACP request is bound to the kinds
  of choice it offers, and is not approved once unless one is `allow_once`, so an approval never ends in the agent's lasting allow. It covers one
  retry of that same request from that same thread, within 10 minutes of approving (a hold waits 10 minutes for the person), and is used up by
  that retry. Another thread, a second retry or a late one gets nothing. The row and the dialog show the working directory, the arguments, the
  names (never the values) of the environment overrides and the effect flags, redacted like the command. A hold is decided once: a racing approve
  and deny have exactly one winner, in either order. A hold that expired, or whose task ended, answers
  410 (`gone`); one already decided answers 409. A hold whose target is not the command the engine would run, or is too long to show whole,
  answers `unreadable` and can never be approved once.
- **Never the always-denied list.** A command on the list (N1 to N10) is never held. The list is read again when the person approves and again when the
  worker retries, so a change in between (the checkout moved to main, say) turns the approval into a refusal. A person's deny rule still wins at the gate.
- **Proof that a person is there: local presence (Approving once: local presence).** Approving needs a grant the desktop app records after the person
  clicks Approve in the app's own native dialog; the account is not involved, so approve-once never answers `account_disabled`. The desktop shell launches
  the service with `--parent-stdin` and a piped stdin only the shell holds (an engine gets its own piped stdin from the service, never the shell's). The flow, over newline JSON on that channel:
  1. The shell sends `presence_describe {request_id, hold_id}`. The service answers `presence_hold {request_id, hold_id, thread_id, task_id,
     command_shown, command_sha256, worker_name, task_title}` or `presence_refused {request_id, code}`. The command, the worker's name and the task's title
     come from the service, redacted (the answer key's text withheld) and bounded, never from the web page. `command_sha256` is the hold's binding:
     an HMAC-SHA256, keyed with a value this process draws at start, of the task, the thread, the kind, the command, its paths and the fields
     that decide what runs. The ledger row carries the same value, so a reader of the ledger cannot test a guess at a short redacted command.
  2. The shell shows the dialog. When the person clicks Approve it mints a nonce (16 random bytes, hex) and sends `presence_grant {request_id, nonce,
     hold_id, thread_id, task_id, command_sha256}`. The service records one grant for the hold in memory (`PresenceBook`), only if the hold is open now
     and every bound field matches it as it stands now, and answers `presence_ready {request_id}`. The shell hands the nonce to the page only after that.
     A later grant for the same hold replaces the earlier one. A grant lives 60 seconds, is used by one approval and is cleared by a restart.
  3. The page posts `POST /api/crew/approvals/{hold}/approve-once` with the nonce in `x-orbyn-confirm`. The service checks the grant first, without
     using it (constant-time compare), then the hold is open, its task is running, the command is readable and the always-denied list again; only then
     does it use the grant up and decide. A refusal at any step leaves the grant as it was; a wrong nonce does not burn it. The ledger row is
     `approved_once` with `via: "desktop_presence"` and `decided_by: "person"`.
  The strings in `presence_hold` are made for a plain-text dialog: control, invisible, bidirectional and format characters become a visible `�`,
  a newline becomes a visible `⏎`, the command is capped at 1000 characters with `truncated` set if it was cut (the binding is of the full command), and
  a hold that cannot be read whole is refused with `unreadable`. The held list, the ledger row and the dialog all show the same escaped text, and the
  command's reason is escaped with it. The shell's dialog renders all of them as plain text (no markup, no HTML, no link detection) and shows them as
  given; it also refuses an answer that still holds a control character.
  Refusal codes on the shell channel: `unavailable`, `invalid`, `not_found`, `gone`, `unreadable`, `mismatch`.
  **Errors on the route:** a service started without `--parent-stdin`, or a remote one, answers `403 desktop_required`; with a desktop shell, a missing,
  wrong, used, expired or mismatched nonce is `403 presence_required`. `Origin: tauri://localhost` is not trusted for any of this.
  **What it protects.** No caller that only speaks HTTP to the service can approve once, with or without the service token. Deny and dismiss need no grant
  because they only refuse (their ledger rows say `decided_by: local_caller`, not `person`), so a token holder can deny or dismiss a hold, never approve one. Every row a person or local caller decides also carries `actor` (`user`: the person, confirmed in the desktop app; `local_caller`: a local API caller of the service, with no proof of who) in its body and in the `actor` field of the served log row.
  **What a process of the same OS user can still do, so this is not OS-level presence:**
  - drive the desktop app's native dialog through Accessibility or SendInput, including a lead that is not confined;
  - read the shell's memory (on macOS the hardened runtime blocks `task_for_pid`, but signing is not checked here; on Windows `PROCESS_VM_READ`);
  - write into the shell's pipe to the service (on Linux `/proc/<pid>/fd/0`, on Windows `DuplicateHandle`).
  Ordinary release builds omit devtools and its menu/shortcut; debug builds retain it, and technical testers can opt in at build time with the Cargo
  `devtools` feature (see [setup](setup.md#developer-tools-for-technical-testers), issue #532). This removes the shipped inspector, not the page's IPC
  authority: a hostile script in the product origin can still call its allowed `invoke` commands but cannot click the native dialog. The main window's
  capability grants only the five app commands; runtime origin checks reserve retry/log/quit for the startup document and presence/person_key for the product.
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
  person's own click; the shell cannot tell a click from a script in the product origin that calls `crew_presence`, so PR3's card
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
  origin, even with devtools disabled: that script can call `window.__TAURI_INTERNALS__.invoke("crew_presence", {holdId})` directly and
  open the native dialog. Because Return approves on macOS and Windows, a person pressing Enter elsewhere while such a dialog opens could approve. This is a
  known exposure. **Key repeat:** when the click came from the keyboard (`detail` 0), the page waits for the Enter key to be released (up to 2 seconds) before
  it asks the shell, so a held Enter's repeats are over before the dialog opens; mouse clicks are not delayed. "Approve it myself" is offered only for a
  row that is a hold, never for a ledger row. Refusals are announced to screen readers, and focus returns to Approve once after a cancel or refusal.
- **N1 and remote viewers.** The three routes are in the Crew marker, so an agent's request to one is refused by path in every mode (the lead is never asked),
  a remote viewer is refused, and with the Preview flag off the routes answer 404 for every method.
- **Ledger.** Every step writes a row: `held`, `approved_once`, `denied_by_person`, `dismissed_by_person`, `hold_used` (the retry went through; the
  `allowed` row says `decided_by: person`) and `hold_ended` (it expired or its task ended). A row carries the hold id, who decided, the redacted command
  and an `hmac-sha256:` binding of the request's chosen fields, never the raw command. A reader who knows every bound field still cannot compute the
  binding: the key lives only in the service process, and the holds it binds never outlive a restart.

Known limits: an unauthenticated caller gets 401, not 404, on a Crew route even with the flag off, like every protected route. The
ledger rows hold the lead's and second key's reasons but not the asker's model, a risk tier or a title; an injected text is kept as its
class only. The risk, review and queue views read the whole table and then page, which is fine for one person's repositories. A
comment is kept and is not yet a fix brief.

Not built, so not served: earning a higher mode per area (4j; the circuit breaker's demotion is built in shadow, see below), trust scores for reviewers and the crew-versus-solo baseline (4g), and
background crews' triggers and runs (4h; their stored state is below).

**As built (issue [#806](https://github.com/orbyn-dev/orbyn-engineering/issues/806), the digest's send time):** the daily digest is stored as
`{time: "HH:MM", utc_offset_minutes}` in the `crew_digest` setting, the scheduled-tasks shape, so it needs no table and no migration.
`GET /api/crew/autopilot/digest/schedule` reads it and `POST /api/crew/autopilot/digest/schedule/set` saves it: a Person write, local only, listed
in `crew::changes::STATE_CHANGING`. The page sends `-new Date().getTimezoneOffset()` with the time, as Settings › Scheduled tasks does. The
watch resolves the local day from the stored offset and sends the digest at most once on that day, at or after the stored time; the local day is
in the dedupe identity, so a restart the same day sends nothing twice, and a day with nothing to say sends nothing. The default is 17:00 with
no offset stored yet, and then nothing is sent: without an offset "17:00 local" cannot be resolved and a guess could send early. Settings ›
Crew draws the row, with a time field where the design's `settings-crew` frame draws the value (the frame draws no editor). The offset is
fixed at the save, like a scheduled task's, so a daylight-saving change keeps the old offset until the person saves again.

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

- **Probing the guard.** The guard acts on a command a worker *runs*. A test brief must ask the worker to run the destructive command against a listed host; a brief that only asks for a file or script holds nothing. To see the held card and the approve-once dialog, the host must be in `allowed_hosts` (N3 runs first) and in this device's `prod` list.

**Not built:** the desktop shell's dialog for loosening; any read of `.orbyn/crew.json` for the guard (deliberately none). The screens are below, "As built (slice 4i, screens)".

**Known limits (heuristic, not secure).** The guard inspects commands a worker runs, not file writes. A worker can write a destructive command into a script with its file-edit tool; the guard acts only when that script or command is later run, and it does not read the script (below). N3 runs before the guard and denies any command that names a host missing from `allowed_hosts`, so a command to an unlisted production host is denied by N3 and never reaches a hold. N3 treats a host it cannot read as unknown and denies it: a `$VAR`, `${...}`, `$(...)`, backtick or glob in the host, URL or remote position of `ssh`, `scp`, `rsync`, `sftp`, `curl`, `wget`, the other connecting programs and `git push`/`pull`/`fetch`/`clone` (`HOST=x; ssh "$HOST"`, `curl "https://$H/x"`, `scp f "$H:/p"`). Each program has its own list of options that take a value, so a flag such as `ssh -t`, `curl -s` or `git push -u` does not hide the target. Variables are not resolved, so a variable elsewhere (the remote command, a header, a URL path, a later `git push origin "$BRANCH"` word) is fine. A backtick or `$(` anywhere in a command that holds a network program is denied by N3, because the reader cannot follow what it builds. This denies some common commands (`git push origin "$(git branch --show-current)"`, `curl -H "Authorization: Bearer $(cat t)" https://...`), so a lead should avoid command substitution in network commands. Hosts named by options are read too: `ssh -J` and `-o ProxyJump`, `curl -x`, `--proxy`, `--url`, `--resolve` and `--connect-to` need an allowed host and an expansion there is denied; `ssh -o ProxyCommand` is denied outright. Not read: a proxy set by environment (`HTTPS_PROXY=$P curl ...`), by `wget -e http_proxy=...` or in `~/.curlrc` / `.wgetrc`; traffic can then go through a host nobody listed. A worker's command that needs approval on a Supervised thread is asked on that thread, not on the Crew board, and expires as a deny when nobody answers: `ORBYN_APPROVAL_TIMEOUT_SECS`, default 1800 s. Not detected, by design of a text reader: `dropdb`, `mysqladmin drop`, `git send-pack --force`, a destructive command run through `kubectl exec` or `docker exec ... -- psql`, a loopback tunnel (`ssh -L 5433:prod:5432` and then `psql -h localhost -p 5433`: the target reads as local; list `localhost` as `prod` while you use one), and static keys in a config file that override a safe-listed AWS profile (keys on the command line are guarded). `docker` with no `-H`, `--context`, `DOCKER_HOST` or `DOCKER_CONTEXT` in the command is read as the local daemon, which is wrong if `docker context use` pointed the CLI at a remote host earlier; prefix the command or list the context. An engine that wraps a command in something other than a plain shell is not matched for its exit status. **Heuristics can never be complete.** The guard reads a command's text and names what it knows; new tools, new flags and new ways to hide a command appear faster than a list can follow, so treat every rule here as a speed bump that catches the common and the careless, never as a boundary. The re-resolve of a held command's targets happens when the person approves and when the worker retries, not when the command executes, so a target that changes inside the command line after those checks is not seen (an in-command time-of-check gap). Wrappers are unwrapped for `sudo`, `doas`, `env` (including `-S`), `nohup`, `time`, `nice`, `ionice`, `exec`, `timeout`, `stdbuf`, `xargs`, `caffeinate`, `setsid`, `chroot`, `unshare`, `flock`, `watch`, `parallel`, `aws-vault exec`, `script -c` and `sh -c`; for any other program followed by the name of a tool the guard knows (`mytool --fast ssh prod ...`) the later word is read as a command, which can over-guard. Not closed: `rm -f` without `-r`; code handed to an interpreter (`ssh prod python3 -c "shutil.rmtree(...)"`), whatever it deletes; a remote shell under `sudo`, `su` or `env` is read as a shell when a pipe, here-doc or redirect feeds it, but other ways to start one are not all covered; `git push --force-with-lease` to a GitHub remote is held by default, and exempting it means safe-listing `github.com`, which also exempts a production repository there; `docker compose` with a project file that points at a remote host, `ssh` commands built from variables or `$(...)`, an `ssh` command that reads its commands from a file given by the shell, `ssh` aliases and config the guard cannot read, MySQL `#` comments (read as unsure: every keyword counts), SQL in a file (`psql -f`) and any program that runs one of these tools inside itself. It does not parse a shell wrapper it does not unwrap, an alias or a function, or a script
that a command runs (`psql -f drop.sql`, `bash cleanup.sh`, `make nuke`): the file is not read. A tool outside the categories (`sqlite3`, a custom CLI, an HTTP call to
a deletion API) is not guarded; the network rule N3 and the tiers still apply. A remote's URL is read from the repository's git config, which an earlier command
of the worker could have changed; so is an `ssh` alias that points to production under a name the person never listed. The lists decide what is local and safe,
and a wrong entry is the person's to fix. Nothing here is tamper-proof: a process of the same OS user can still read the data folder and change the table.

**As built (slice 4i, screens):** read-only web screens for the guard, the path classes and routing, from the Crew Designs frames `guard-*`, `classes-*`, `route-class-*` and
`route-log*`. Code: `web/src/features/crew/autopilot/approvals/` (`ProductionTargets.tsx`, `PathClasses.tsx`, `RouteCard.tsx`, `RoutingLog.tsx`, `ledger.ts`). `data-frame` names the frame a state is in.

- **Guard rows come from the ledger.** A row with `body.guard` is drawn as a guarded command: `allowed` by the person is "Approved by you", by two keys "Approved by two keys",
  a disagreement "Denied by the second key", `needs_second_engine` "Denied · no second key". The exit status is the `command_exit` row whose `body.row` is the allowed row:
  a number, or exactly `unknown` ("Exit status unknown", neutral), or nothing yet. The `denied` row a hold writes under On and Off (`decided_by: guard`) is not listed:
  the hold, from `GET /api/crew/approvals/held`, is the request, and takes the target from the `held` row with the same `body.hold`. A "no second key" row takes the live hold
  with the same task and command, so Approve it myself answers it; a disagreement has no hold, so it offers none.
- **Approving is the desktop app's.** The web shows state and the held card; Approve once and Approve it myself ask the shell's native dialog and send its one-time nonce in
  `x-orbyn-confirm`. There is no web path that approves without it. In a browser tab the card says "Approving once needs the Orbyn desktop app."
- **Settings › Crew** lists this device's `prod_targets` and `safe_targets` (`GET /api/crew/guard/targets`), the built-in categories and the path classes. The screen has no
  thread: it sends `project` (the project Settings shows: the checkout's project when one is selected, else the first, named on the panel). The service takes `project` as the alternative to
  `thread` in a separate change; until that merges, `GET /api/crew/classes` and `/classes/test` answer 400 "thread is required" and the panel shows the error. The test box takes a path list, one per line (1 to 10), and a command box asks `GET /api/crew/guard/test`
  (thread optional there); both read once typing stops. Engines are named by id as served. Nothing writes: no Mark a target (loosening needs the shell's dialog,
  not built), no Save, no Open in editor.
- **Route card** (`RouteCard` for a task id, `GET /api/crew/route/{task}`) has three states: an engine blocked by a class, the file invalid (with Check again) and no engine
  allowed. Where it is mounted in Flow and the Board is Engineering 1's. The Approval log has a Routing view over the `route_class` ledger rows, offered once one exists.
- **Not served, so not drawn:** a class's lock and its Sensitive or Open label (no column drawn), whether the file is in another worktree, a task's title and kind, an engine's model and vendor
  on a ledger row, the allowed engines per class in a route row, a held-for-a-second-key state while the second key is still reviewing, Split the task, Change the rule, Ask me
  to choose, Open the rule, Pin an allowed engine, Open in editor.

**As built (slice 4j, shadow):** autonomy by area, part 1: a circuit breaker per area that *records* a demotion. **This part was shadow; part 2
([enforcement](#as-built-slice-4j-enforcement), below) now makes the merge gate read it.** `crew_autopilot_mode` is still what the person chose. Code:
`crates/orbyn-core/src/crew/autopilot/areas/` (`decide` the pure rule, `breaker` the writes and the hook, `view` the reads), `store/crew_autonomy.rs`.

- **Area.** The first two folders of a path (`risk::area_of`), or the one folder, or the file at the root. The key is `(project, area)`, and the record
  counts only merges whose run's lead thread belongs to that project, so the same name in two repositories is counted for each repository on its own.
- **Rule (all Proposed, Engineering's constants in `areas/mod.rs`, served by `GET /api/crew/autopilot/thresholds` with `proposed: true`).** Window 14 days; fewer than
  5 merges in it is "not enough data" and nothing changes. **Demote** when 2 or more of the merges made in the window were reverted ("merged in the window, now
  reverted", the same meaning as the 60-day rate). It is evaluated when a revert passes through and when a merge is recorded (3 merges with 2 reverted, then 2 clean ones, trips at the fifth merge; cause `merge:<id>`). **Return** when both hold: 14 days since the recovery clock started with no revert, and 5 merges since then, none reverted.
  The clock starts at the demotion and restarts at every later revert in the area (a `recovery_reset` row). Escapes (a defect that no revert shows) are
  **not measured**: there is no source for them, `escapes` is always `null` and no count includes one. The 60-day, 10-merge, 20% rule in `merge/risk.rs` is
  unchanged and keeps raising the risk tier.
- **Ceiling.** The ceiling is the person's mode for the repository, else the default, else Full, read live. A demotion lowers an area from Full to On only; a ceiling of
  On or Off is never changed and the record is still shown (`ceiling_lower`). A crew's own choice is not read for this (a known limit).
- **Stored on this device** (schema **44**, `CREW_AUTONOMY`), never in a repository file (architecture rule 4): `crew_area_modes` (state `follows` or `demoted`, when and why,
  the recovery clock, a person's override column that nothing writes yet) and `crew_area_events`, append-only (a trigger refuses UPDATE and DELETE), unique per
  `(project, area, kind, cause)`. The record and the recovery counts are never stored: they are counted from `crew_merges` each time. Each breaker write adds its event row and
  then changes the mode row **in one transaction** (rule 6: the audit row first; a row the table refuses leaves the mode as it was). A demotion also appends an `area_demoted` row to the
  approval ledger on the task whose revert tripped it, so Flow shows it.
- **When it acts.** `revert_merge` is the one place every revert passes (the watch's bisect revert and the person's Revert). Right after the merge row says reverted, still under the merge lock, the
  breaker runs for each area the merge touched. The same revert is recorded once. A failure there is logged and never undoes a revert that already moved the branch. Queuing checks the way back only if the merge lock is free (it never waits for a long check or a bisect); recording a merge checks it too. The way back is checked
  when a merge is recorded and at the start of queuing one, so it waits for at most one merge; no GET writes, and the view says "Criteria met; returns at the next merge."
- **Routes** (Preview-gated, 404 with the flag off; all GET, all in `crew::changes::READS`, nothing writes): `GET /api/crew/autopilot/areas?project=&limit&offset` (the ceiling, `enforced: false`,
  and per area the record, a state of `follows`, `demoted` (just lowered), `recovering` (lowered, with the days and merges so far), `criteria_met` (returns at the next merge), `not_enough_data`, `ceiling_lower` or `overridden`, an `area_key` to pass to the history route, a sentence, when it was lowered and what recovery still needs),
  `GET /api/crew/autopilot/areas/history?project=&area=` (the trail, newest first, at most 100; the area is in the query because it contains `/`) and `GET /api/crew/autopilot/thresholds`.
  `GET /api/crew/trust` now sets `autonomy_by_area: true` and takes an optional `project`, so the revert rates and the risk score's `merge_rates` count one repository's merges instead of all of them.

**Not built in part 1:** enforcement and a person's override (both in part 2, below); the screens; earning a higher mode for an area; escapes.

**Known limits (4j).** Reverts and merges made outside the crew (by hand, on a forge) are not seen. The area is two folders deep, so a large folder and a small one weigh the same and a change spread over many areas counts in each. A return waits for the next
merge. Merges whose lead thread was hard-deleted drop out of the rates, the trust record and the breaker (the join needs the thread; the run rows stay). The thresholds are a proposal and may change.

**As built (slice 4j, enforcement):** autonomy by area, part 2. The merge gate reads an area's mode, a person can override it, and the effect shows in the
risk report, the Review queue and the digest. Code: `crates/orbyn-core/src/crew/autopilot/areas/` (`effective` what the gate reads, `person` the two
writes, `retry` a failed breaker), `store/crew_autonomy_person.rs`, `orbynd/src/routes/crew_areas.rs`. **No migration:** it uses the part 1 tables (schema 44).

- **What the gate reads.** The mode a merge runs under is the lower of the **ceiling** and the mode of **every area its diff touches**. The ceiling is the mode
  in force for the merge's crew (`crew_autopilot_mode`: the crew's choice, else the repository's, else the default, else Full), the same one recorded as the
  entry's `mode`; part 1 read the repository's alone, and the breaker now reads the crew's too. An area that follows its ceiling is on the ceiling; an area in state `demoted` is on **On only when the
  ceiling is Full** (a ceiling of On or Off is never changed); a person's `keep_ceiling` ignores that lowering for the area; `off` sets the area to Off (never above the ceiling: Off is
  stricter, and a person's choice). An area whose window already trips but whose next merge has not recorded it yet counts as lowered too. It is read when a merge is queued and again
  when the queue is about to merge it, so an area lowered in between holds the merge.
- **Effect.** A high-risk diff in a lowered area **waits for a person** with the rule On has (`waiting`, released by a person as any waiting merge). The risk tier does not change,
  the entry's `mode` stays the crew's, and the queue's note and the `merge_queued` row name the area, its mode and its record. A medium or low diff in the same area still merges. An area on Off holds every diff in it.
  The chain and the tier do not read an area in version 1.
- **Fails closed (rule 7).** If an area's state cannot be read, each touched area is treated as **Off**: every diff in it waits for a person, whatever its risk (Off, not On, so a failure to read can never
  let through a diff in an area the person set to Off). An `area_read_failed` row goes to the approval ledger on the task, not only the log. A breaker that fails after a revert writes `area_breaker_failed` on the merge's task (Flow shows it); the revert stands. The revert is looked at again at the next merge or revert
  (every breaker write is idempotent: one row per cause and kind; up to 200 unresolved failures are read, resolved rows filtered out in SQL), and a retry that works is marked `resolved: true` on the same kind and not tried again.
  A failure after a **merge** is written the same way but is **not retried**: the next evaluation reads the whole window, and the gate counts an area whose window trips as lowered meanwhile.
- **A person's override** (Preview-gated, `local_only`, in `crew::changes::STATE_CHANGING` and `GUARDS`, so Autopilot's N1 refuses an agent's request by path):
  `POST /api/crew/autopilot/areas/override` `{project, area, choice: "keep_ceiling" | "off", acknowledged: true, seen: {merges, reverts, tripped_at}, reason?}` and
  `POST /api/crew/autopilot/areas/follow` `{project, area, seen?}`. Both answer with the area as it now stands. Rules: only those two choices exist, so a choice never raises an area above its ceiling, and
  `keep_ceiling` is refused when the area is not lowered; `acknowledged` must be true; `seen` must match the current record (the window's merges and reverts, and when the breaker lowered the area), else **409 `stale`**
  (the person saw an old record). The comparison is made inside the write's transaction and the merge lock is held around it, like the breaker's writes, so a revert cannot land between the person's look and the write;
  if a merge is being checked the write waits up to 5 seconds, then answers `stale`. The trail's `from_mode` and `to_mode` use the ceiling of the crew that merged into the area last (the gate's own function), else the repository's; the same choice twice records nothing the second time; `follow` ends the override and changes nothing else (the breaker's state stands). **An override ends at the next trip:** a new revert in
  an area under `keep_ceiling` clears it (`override_cleared`, decided by the breaker, then the recovery clock restarts) and the area shows lowered again; a lowered area that returns also drops it, and the trail shows it (`override_cleared` before `restored`). `off` stays until the person follows.
  The audit row (`crew.area.override` or `crew.area.follow`, actor `user`) is written first, in the same transaction as the `crew_area_events` row (`override_set` or `override_cleared`, `decided_by` person, cause `person:<audit id>`) and the
  mode row; a row the table refuses leaves all three as they were. Plain local write, no presence grant. **`keep_ceiling` loosens:** it returns the area to the ceiling the crew has, so a lead that holds the service token could call it to defeat the breaker. That is no stronger than
  `PUT /autopilot/mode` setting the repository to Full, which the same token can already do, but it is a loosening and not a safeguard. An area is one or two folders (or a root file), without an empty or `..` segment.
- **Effect fields.** `GET /autopilot/risk/{task}` gains `area_modes: [{area, mode, line}]` ("Area web/app is on On: 2 reverts in 14 days · 9 merges"); each Review queue item gains the same marker for the areas
  the breaker has lowered or a person set to Off; the digest gains `areas_changed` (lowered, returned, override set or cleared, in the window). `GET /autopilot/areas` now says `enforced: true`. The Needs you line for a lowered area is assembled by
  the web from the areas read; there is no new route for it.

**Thresholds are PROPOSED** (window 14 days, 5 merges, 2 reverts, 14 days and 5 merges back; served with `proposed: true`) and may change.

**Not built (4j).** Earning a higher mode for an area (`autonomy-promoted`, `review-promoted`) is PLANNED, not built. Escapes (a defect that no revert shows)
are **not measured**: nothing counts them. The chain and the tier do not read an area.

**Known limits (4j, enforcement).** Reverts and merges made outside the crew (by hand, on a forge) are not seen. Any local process that holds the service token can post an override, the limit `set_mode` and the other Crew writes have, and the audit row
says `user`, not which process. A lowered area returns at the next merge or queuing, not at the moment its criteria are met. The area is two folders deep, so a large and a small folder weigh the same. A ledger event cannot be written when the
ledger itself is unreadable: the log then holds the only trace, though the gate still treats an unreadable area as Off. An `off` posted while merge checks are running does not stop the merge already in flight (the same as `set_mode`).

**As built (slice 4j, screens):** Settings › Crew › Autopilot by area and its Thresholds, from the Crew Designs v39 frames `autonomy-table`, `-tripped`,
`-demoted-ceiling-lowered`, `-recovering`, `-criteria-met`, `-restored`, `-lowdata`, `-empty`, `-repos`, `-entry`, `-override`, `-override-warning`,
`-thresholds` and `-narrow`, plus `autonomy-override-off-warning` (Designer ruling on #573). Code:
`web/src/features/crew/autopilot/approvals/{AutonomyAreas,AutonomyThresholds,areas,AutopilotSettings}.ts*`, routes
`#/settings/crew/areas[/thresholds]?repo=<project id>` (the first visible repository when `repo` is absent). One component, driven by the areas read; the page's
`data-frame` names the frame its data is in. **Ceiling:** the header says "Repo ceiling: X" (the project's) once and the table has no ceiling column; rows show the mode the area is on now, and
a row whose own ceiling (the service sets one per area) differs from the repo's says so in its state text. Both Keep the ceiling and Off ask first in the same dialog and send `acknowledged: true` only after the box is ticked;
Override… is on the tripped, recovering, criteria-met, ceiling-wins and restored panels and in every overridden row; Keep the ceiling is offered only when the area is below a Full ceiling; Follow the record undoes a choice (sent without `seen` when the row has no `since`, as the service stores the trip time through an override). A `stale` answer closes the warning and shows the service's words on the page. The words are the design's,
written from the served fields. No date is drawn that the service does not serve (recovery clock, who reverted, when a ceiling was lowered).
**Empty:** with no area in the window the page keeps its header, chips, as-of line and, above 560 px, the table's head row; it draws no row. At 560 px and below the head row is the whole table, so the empty table is not drawn. The design's sentence in the table is Planned and is not drawn. **Several repos:** the page is about one repo at a time (`?repo=`, else the first visible project); with more than one the Repo chip is drawn as the design's selector (bordered, with the Planned label) from the workspace's project list, and its menu is not built. **Entry:** the Settings › Crew › Autopilot section has a row under it that opens Autopilot by area, and Thresholds has the back link that returns to it with the repository kept; both are drawn without the design's Planned chip because they now work.
Not built: the repo selector's menu, `autonomy-promoted`, `autonomy-by-area`.

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
  running; `needs_review` while its newest run waits for the person's review; `stopped` when its newest run stopped for the budget or a
  failure (`budget`, `repeated_failure`, `engine_error`), even with older reviews waiting; `needs_review` when the crew has no run or its
  newest run was stopped as `disabled` while an older review still waits; otherwise `idle` (never ran, the newest run was stopped by the
  person's own switch and nothing waits, or the person acknowledged the newest run's review). Spending to either number of the budget
  stops the run with the reason `budget` and puts it in the review queue (`review_waiting`). Turning a crew off stops its run with the reason `disabled`, logged as the person. **Delete is
  refused only while a run is in progress**; a crew with waiting reviews can be deleted (a waiting review is settled by the person's acknowledgement), its runs
  and waiting reviews are removed with it, and the audit row records `runs_removed` and `reviews_waiting_removed`.
- **Audit:** create, update, enable, disable, delete, and each run's start, spend, stop, review and acknowledgement write an `audit_events`
  row in the same transaction as the change (rule 6), before the row it describes changes (a spend that reaches the budget writes the spend
  row, then the stop row, then updates the run once; an acknowledgement writes its row with the run, its state, its stop reason and the
  reason the review kept, then clears the wait); a failed audit row rolls the change back, and the rows stay after a crew is deleted. A
  person's disable, the stop it causes and an acknowledgement are logged with the person as actor and source; a run's start, spend and
  other ends as `crew`.
- **Routes**, all under `/api/crew/background`, behind the Preview flag like every Crew route (404 with it off, 401 without the token, writes
  refused for a remote viewer and listed in `crew::changes` so N1 refuses an agent's request): reads `GET /` (`project`, `limit`, `offset`),
  `GET /{id}`, `GET /{id}/runs`; writes, local only (a Person route: a remote viewer gets 403 `local_only`, an agent is refused by N1),
  `POST /create`, `/{id}/update`, `/{id}/enable`, `/{id}/disable`, `/{id}/delete`, and `/{id}/run-now`, which starts one run of an enabled
  crew by hand (the `manual` event: `crew_background_run_now`, one run and its lead thread, as a Low Crew-started task that the daily
  budget and review capacity may refuse; a refusal stores no run), and `/runs/{id}/acknowledge`, which settles one waiting review
  (`background_acknowledge_review`: one write clears `review_waiting` and returns the run with its reason, state, stop reason and spend
  unchanged; a run that is not waiting is refused plainly). There is no route that spends, merges or reverts.
- **In the Review queue.** A run that stops without a merge waits on its own run row (`needs_review`, or `stopped` for the budget or a
  failure), and `GET /api/crew/autopilot/review` lists it beside the merges as `background_runs`, oldest first, in the same `limit`/`offset`
  window as the merges, with `background_total` counting all of them (`next_offset` is set while either list has more): the run's state,
  stop reason, plain reason, spend and budget. It carries no merge, commit or risk, because it has none, and it is not listed while running or
  when the person's own switch stopped it. **The person's answer to one is acknowledge-only (owner ruling, 2026-11-20):**
  `POST /api/crew/background/runs/{id}/acknowledge` clears the run's wait and returns it with its reason, state, stop reason and spend
  unchanged, so the run keeps why it waited for the record; the crew's `reviews_waiting` drops, the same crew may run again, and the run
  leaves the queue (`background_total` no longer counts it). It changes nothing else and a run that is not waiting is refused plainly. A
  run that stopped for a failure still appears (it has nothing to review and nothing to acknowledge), and the Review screen itself is not
  in the beta.

What is not built: **a run starts** when a person presses Run now (`POST /api/crew/background/{id}/run-now`, the `manual` event of
[the engine start](crew/background-engine-start.md)), when a listener fires (next paragraph) or when a test calls the run functions. A start creates one Crew
worker task for the run; the lead thread of the run runs no engine and takes no message. Turning the crew off cancels that task. A
background crew with merge authority waits for later work (every merge of a background run waits for a person); the person's answer is
the acknowledgement above. Known limits: removing a project leaves its crews (a later trigger must refuse a crew whose project is gone);
crews are per device; a template's prompt and checks are code-owned and defined only for the four templates a run can start (`background_start.rs`);
any other template is refused.

**Watches (slice A1):** a background crew's trigger may be `watch`: `{ "kind": "watch", "command": "...", "every_hours": 6,
"on_failing": "needs_you" | "template" }`. `command` is 1 to 500 characters and follows the same text rules as every other trigger
setting (no control or invisible characters, the locked redactor; a redaction that would pass 500 characters is refused). `every_hours`
is 1 to 720, and `on_failing` is required. A project holds at most 10 watches, counted in the same transaction as the insert. The command
is screened at create and update with the always-denied list (N1 to N10) and the blast-radius guard, with the project's folder as the
worktree; a refusal names the rule in one fixed sentence and never repeats the command. `needs_you` stores the template and budgets as
given (they must be in range, and no model run follows); `template` validates them like any other crew. The command is stored redacted.
Nothing runs a watch yet: there is no listener, no command execution, no model call and no UI, and Run now refuses one.

**As built (slice 4h, listeners, migration 59):** `crew/background_listeners.rs` is one owned future, `run_crew_background_listeners`, that the
service starts behind the Preview flag **only when it sets an interval** (`ORBYN_CREW_BACKGROUND_WATCH_INTERVAL_SECS`, 60 s to 24 h; unset means no pass, not even at
startup) and awaits on shutdown. Each pass fires what is due for the crews that are on (a crew that is off is not read from the forge and spends no event):
a `schedule` crew once per `every_hours` bucket, and a `ci_failure` crew on a failed check run at its branch head (its branch setting, else the project default),
narrowed by its optional workflow filter. Every firing first claims a row in `crew_background_events` (first write wins) and then starts a run through the same seam as Run now:

| Event | Claim | Held-start key |
| --- | --- | --- |
| schedule | `schedule:<crew>:<bucket>` | `<crew>:schedule:<bucket>` |
| CI failure | `ci:<crew>:<head>:<hash>` | `<crew>:ci:<head>:<hash>` |

`<hash>` is the first 16 hex characters of the SHA-256 of the failing check's name. A repeat of an event starts nothing new, and a bucket at or before the newest claimed one
never fires again (a clock that moves back starts nothing). **One firing per crew per pass**: a head with several failing checks fires the first and leaves the others for a later pass.
A firing refused for review capacity, the daily budget, an untrusted folder, no engine or a run already in progress is **dropped, not retried**: its claim stays, capacity also leaves its held row,
and every refusal writes one `crew.background.listener.refused` audit row (the crew, the listener, a fixed reason word; the actor is the constant `crew`). **Text from outside never reaches the prompt**:
a check's name, a head sha and a `.orbyn/crew.json` trigger's name only decide whether a run starts; the brief, the worker's prompt, the lead thread and the audit stay code-owned. A stop is
looked at between firings, never inside a start, and a run a crash leaves is settled when the core opens. A trigger in the file with no stored crew is ignored. At most 64 CI crews are watched per pass, from a rotating start.
`crew_background_events` is not pruned yet.

**As built (slice 4h, triggers — parsed only):** a repository can describe the background crews it would like Orbyn to run, in
`background` of `.orbyn/crew.json`. Orbyn reads the file and never writes it (architecture rule 4).

```json
{ "background": { "version": 1,
  "templates": [{ "id": "nightly-deps", "description": "Bump one dependency a week." }],
  "triggers": [
    { "name": "fix-main-ci", "on": "ci_failed", "template": "fix_failing_ci",
      "budget": { "turns": 30, "minutes": 20 }, "engines": ["claude", "codex"],
      "description": "Fix a failing build on main.", "branch": "main", "workflow": "ci.yml" },
    { "name": "weekly-deps", "on": "schedule", "template": "nightly-deps",
      "budget": { "turns": 20, "minutes": 15 }, "engines": ["codex"],
      "description": "Bump one dependency a week.", "every_minutes": 10080 }
  ] } }
```

- **Kinds:** `ci_failed` (optional `branch` and `workflow`), `schedule` (needs `every_minutes`: a whole number of hours in minutes, a multiple of 60 from 60 to 43200, so each maps to the stored crew's `every_hours` 1 to 720) and `manual` (only
  when a person asks). A trigger takes the settings of its kind and no others.
- **Strict:** at most 64 triggers and 16 templates; `version` must be 1; unknown fields, kinds and templates are refused rather than
  ignored; a name is 1 to 80 characters and unique after trimming, lower-casing and collapsing runs of whitespace (not Unicode
  normalised, so two spellings of one letter can still look alike); a description is 1 to 280 characters; the file is 64 KiB at most;
  `branch`, `workflow` and a template id are 1 to 100 characters, and a template id is a token (1 to 64 characters, starting with a
  lowercase letter or digit, then lowercase letters, digits, `.`, `_` or `-`). A declared template may not reuse a built-in id or another
  declared id, and needs both an id and a description. `null` is refused where an array belongs, never read as absent.
- **Budget:** `turns` 1 to 1000 and `minutes` 1 to 1440, written by a person. There is no default, and a missing or over-cap budget
  refuses the trigger.
- **Text is data:** hidden characters (zero-width, bidirectional, tag block) and text that reads as an instruction are refused by the
  Autopilot tripwire; text that changes under the route's redaction (a credential-shaped value) is refused too; everything else is carried as written and is never expanded, followed or run.
  Unknown keys get the same treatment before they appear in a problem, and a key written twice inside `background` is refused, naming the key (a key repeated in another section is not this file's concern, so it is neither reported nor refused). A value or key that reads as an instruction, even once hidden characters, line separators, `_` or `-` are read as spaces, is withheld from the problem rather than quoted. A
  refusal lists at most 50 problems and then counts the rest; at most 64 triggers and 16 templates are read.
- **Templates grant nothing:** a template is an id and a sentence for the person. It cannot allow commands, because the always-denied
  list is enforced in code and nothing in this file can widen it.
- **Every problem at once:** `crew::triggers::parse_and_validate` returns all of them, each naming the field and the shape it wants, in
  the form `crew_start` uses.
- **Not wired to the stored crew:** this section is policy a later slice will read, not the record. The stored background crew above keeps
  its own vocabulary (`ci_failure`, `issue_label`, `schedule`, `dependency_update`, `watch`, with `every_hours`), and it stays the record of what a
  crew is; the file uses the names this work starts with (`ci_failed`, `schedule`, `manual`, with `every_minutes`). A later slice maps a
  file trigger to the stored crew when it starts one. Nothing here writes, starts or records anything.
  Only `ci_failed` and `schedule` have a stored-crew match (`ci_failure`, and `schedule` with `every_hours`). **Planned:** `manual`
  and declared templates have no stored-crew match yet, so the later mapping slice decides them.

What is not built (Planned): **nothing reads this section yet.** The CI-failure listener, the schedule timer, the manual start, the
templates' prompts and checks, the routes and the UI are later work; this parse starts no run, calls no model and spawns no process.

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
  **Thread or project.** Both class routes take exactly one of `thread=<thread id>` or `project=<project id>`; the Settings view has no thread. Both ids
  is a 400 ("give thread or project, not both"), neither is a 400 ("thread or project is required"), an id that fails the route id rule is a 400
  ("that id is not valid"), and an unknown thread or project is a 404. The flag is checked before the query is read, so with the flag off every request, valid or not, is the absent route. A repeated `thread` or `project` key is a 400 ("give thread once"), on both routes. A project that is not trusted is refused with `folder_untrusted`, as a thread's diff is, and a removed project with 409 `project_removed`; neither is read. (A thread keeps its older behaviour.)
  A thread reads at its own working folder. A project reads at the checkout the project is registered at (its `path`), never at a path the client sends, with
  the same hardened git reads, the same base commit and the same default-branch pin as a thread: for the same checkout the two reads are equal except for
  `thread` and `worktree`, which are absent when read by project.

What is not built: the guard that watches a worker against its classes (PR B; distinct from the 4i blast-radius guard, which is built); the route card's mount in Flow and the Board and the Needs you item
(Engineering 1's surfaces); enforcement at the gate and at verify time, so **a worker that is allowed to run shell commands can still read a file outside its lease
until #441's sandbox covers it, and a symlink a worker adds during the task is not seen**; an ask-me route and a `locked` flag on a rule
(ruling pending). Known limits: **the rules are read at two places and the stricter applies: the checkout's HEAD at task start, and the project's default
branch as of the last fetch** (`refs/remotes/origin/HEAD`, else `origin/main`, `origin/master` or `origin/trunk`, else a local `init.defaultBranch`, `main`,
`master` or `trunk`; with none of them the rules are read at HEAD alone, as before). An engine must be allowed by both, a file that cannot be read on either
side blocks every engine, and the route card says in words when the two files differ. So an agent that moves HEAD to a commit without the file, or commits a
looser one, does not loosen the next task. A remote-tracking ref is moved by `git fetch` and `git pull`, not by `checkout`, `switch`, `reset` or a commit in any
worktree; a local branch is moved by a commit or `git branch -f` on it, so it is the weaker fallback. **Neither is out of an agent's reach:** a worker runs as the person, the
macOS confinement of #441 closes the data folder and not the repository's `.git`, and Windows and Linux workers are refused, so an agent that runs git plumbing
or edits `.git` can still move either ref. N7 holds `git update-ref`, `fast-import`, `send-pack`, a writing `symbolic-ref`, any fetch or push whose refspec ends under
`refs/remotes/` (also spelled `remotes/` or `origin/`, in any case) and `fetch --stdin`, the `git remote` commands that repoint or remove the remote (`set-head`, `rename`, `remove`,
`add`, `set-url`, `set-branches`), `branch -r` with a delete, move or copy, and a checkout, restore, reset, stash, rm, add, commit and the like that names `.orbyn/crew.json` or `.orbyn`,
or a revision with a whole checkout (`git checkout <rev> .`). It reads command text, so a path built in a script, a hand edit of `.git`, or a command the parser does not
recognise is not seen; a bare `git checkout <rev>` names no path and is not held, because the default branch's copy is the check for it. **A project with an `origin` remote blocks every engine, under the rule name `default branch unreadable` (a git setup problem, not a rules file one), until a remote-tracking
default-branch ref resolves.** A local branch no longer stands in once `origin` is configured, because an agent could have deleted the ref and left the local branch to read. Two cases, with their fixes, which a person
runs (N7 holds them for an agent): nothing of `origin` is fetched, so run `git fetch origin` (for a new remote with no branches yet, push the default branch first); or `origin` has branches but none is
`main`, `master`, `trunk` or the one `origin/HEAD` names, so run `git remote set-head origin --auto` (or `git fetch origin <default>`). **A brand-new project whose remote is empty is blocked until its first push (fail closed, by
design).** **Each project remembers the default-branch ref and commit its last started task read** (in the existing
`project_preferences` table, key `crew_pinned_ref`; no migration). A start that reads the same commit or a descendant is accepted and moves the memory
to the new tip, so a later rewind is compared with the newest accepted commit, never an older one. A remembered ref that is gone (the remote removed, the
ref deleted, or only a local branch left where a remote-tracking ref was) and any move that does not descend from it (a rewritten branch) block every engine
under the rule name `default branch changed`, with the ref and both commits in plain words, until a person decides. `GET /api/crew/projects/{id}/pinned-ref` shows what the checkout reads now, what the project
remembered and whether a start would accept it; `POST .../pinned-ref/confirm` names the whole commit the view showed (a ref that moved again is refused as
stale) and `POST .../pinned-ref/reset` forgets it, after which the next start reads the ref the checkout has then. Both writes are local-only, need no
agent tool, refuse a remote viewer, and write `crew.pinned_ref.confirm` / `crew.pinned_ref.reset` audit rows in the same transaction; every `route_class` row
already records the pinned ref and commit it read. With no `origin` remote at all, the local default branch is the ref read, and with none of those either HEAD alone is read, as before; a remembered
pin from an earlier remote still blocks until a person confirms the local branch or resets it. **A person or an agent that can push to the
base branch can still change the rules for later tasks.** A rule that is tighter at HEAD applies at once, because an engine must be allowed by both; a loosening takes effect only once it is on the default branch's tip. `review` lanes, `checks`, `required` and `allowed_hosts` are still read at the base commit only. A path or lease spelled in a different case from what is tracked (`DOCS/key`, `Docs/**`) lists nothing, so it
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
Preview screens in `web/src/features/crew/{escape,capacity,settings}`, opened by `#/preview/crew/<frame>` in
dev builds only. They share one stage in `features/crew/shared`: the design's own markup, with its rules renamed `cx-*` in
`shared/design.css` on the app's tokens. They were first display-only, with every button disabled with "Crew is not connected to
this app yet". That is no longer true of all of them: the escape and capacity backends are built as separate slices (escape design:
[crew/escapes.md](crew/escapes.md); capacity: slice 4l below) and their live screens read them (22 of the 28 escape frames and the two
`capacity-live-*` screens). The live escape, capacity and backtest screens are reached from Settings › Crew behind `crew_preview` (the four entry rows and the
Coverage links, Crew Designs v87; see [escapes.md](crew/escapes.md), slice 4l and backlog B3 below); the Review-tab Escapes section
(`entry-crew-review-escapes*`) is not built, because the beta has no Review tab. The remaining escape frames (report, dashboard,
route counts, create check) and the other capacity frames stay drawn samples with disabled buttons. Values the design
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

**As built (slice 4l, store, migration 47):** `store/crew_capacity.rs` keeps a project's review-capacity settings
(`crew_capacity_settings`: limit, default 8, and sampling, default off), a task's `priority` (default `medium`) and
`crew_started` (default 0, a person's start) on `crew_tasks`, and each sampled batch in `crew_review_samples`: task
ids, chosen or not, the seed and the time, with no prompt or path. A task stored before 47 is Medium and
person-started, so it can never be paused. The start hook and the merge hook below read them; the merge hook also runs sampling (below).

**As built (slice 4l, start hook):** `crew/capacity/hooks.rs` is called by one line in `start.rs`, after the lease conflict
check and before the task is stored, and does nothing with the Crew Preview flag off. A start that Crew began itself at Low
priority is refused with `not_authorized`, and the waiting count and limit in its message, when more High and Medium merges
wait than the project's limit; at the limit, or for a person-started or Medium or High task, it goes. A new task has no stored
priority or `crew_started` yet, so the start path passes none (Medium, person-started) and nothing pauses until a later change
sets them. A failure reading the settings or the queue lets the start go.

**As built (slice 4l, held starts, migration 54):** with `pause_enabled` on, a refused Crew-started Low start is noted in
`crew_paused_starts` (`store/crew_paused_starts.rs`): one row per project, kind (`comment_fix`, `review_fix`,
`escape_check`; `background` is reserved; `unknown` marks a Crew-started launch that named no trigger) and a stable key
the caller supplies through `Links::trigger`: the merge task id for a comment fix, a hash of the task id and finding
fingerprint for a review fix, the check id for an escape check (ids and hashes only, never the brief's hash, which
changes per attempt), with the waiting count and limit at the last refusal, `first_seen` and `last_seen`. The same
trigger refused again updates its row; when the same trigger later starts, the row is marked `started`; it is held again
if refused after that. At most 200 rows per project, `started` rows are dropped before `held` ones, then the oldest by last sighting. No task text and no engine
output are stored. Nothing is retried on a timer, the refusal is still `not_authorized`, and with pausing off, for a
person's start or for High and Medium work nothing is written. `GET /api/crew/capacity` returns them as `held`.
A background crew's Run now (`crew_background_run_now`, the `manual` event) and the listeners start one crew task as Low and Crew-started with a
`PausedTrigger` of kind `Background`; `crew_background_start_run` still only records a run row.

**As built (slice 4l, merge hook):** `crew/capacity/hooks.rs` is called by one line in `merge/queue.rs`, right after the area
breaker's look at a stored merge, and does nothing with the Crew Preview flag off. It records the merged range's hunks and
note in a detached task (the merge path is synchronous, git is async), against the project's `crew_person_commits` (the list an app path
records a person's commit into: see the person signal below). A failure never fails or delays a merge: it is
logged and written to the approval ledger on the merge's task as `provenance_failed` (a new kind in `EVENT_KINDS`). Sampling runs in the same
detached task, off by default: see `crew/README.md`, `capacity`.

**As built (slice 4l, per-hunk provenance, migration 48):** `crew/capacity/provenance.rs` reads the new-file line ranges a
merge brought in (a byte-exact `git diff -U0 before merge`; added, modified and renamed-and-edited files count, deleted
and binary files and unsafe paths do not) and `store/crew_provenance.rs` keeps them in `crew_hunks` with the author: the
task's engine and model for the agent, or a person for the lines whose last-changing commit (by `git blame` of the merge
commit, one blame per file, repository ignore-revs lists cleared) is in the positive `person_commits` list the caller
passes. Anything not positively a person's is the agent's: an empty list, an unknown or rewritten id, a conflict
resolution, a failed blame (this paragraph is the first build; the person signal below replaces its agent default with a
third value, `unknown`, and reads author and committer emails). A merged hunk is split into runs of one author. The merge
hook must pass only positively identified person commits, never a guess. The first time provenance is recorded for a repository is kept once in
`crew_provenance_start`, first write wins and never moves, so an older merge recorded late reads as "Not measured" (as does a
merge with no hunk, which has no note and no `note_ref`); coverage built on this is merge-level, not line-level review. Hunks and that start go in one
transaction. Then a note in the Agent Trace shape (version, revision, per file the ranges by contributor `agent` or
`person`, engine and model ids, counts) is written on the merge commit under `refs/notes/orbyn-agent-trace`. The ref is
local only: the app's push is `git push -u origin <branch>` and its fetches name explicit refs, so no push or fetch
carries it, and a test keeps it so. A note that fails is returned for the caller to log; the merge is not failed.
Nothing calls this from the merge queue yet; the hook is a separate PR.

**As built (slice 4l, person signal, migration 55; design: `docs/crew/provenance-person-signal.md`):** a line's author is one of three
values. `agent`: the commit that last changed it (`git blame` of the merge commit) has an author or committer email that is exactly
`worker+<task id>@orbyn.invalid` for a task that exists in `crew_tasks` (the id must be the app's `sha256:` and 64 lowercase hex form), or a
message trailer `Orbyn-Task: <task id>` naming such a task. `person`: the commit id is in `crew_person_commits` for the project that led the
merge's task, and the commit has no worker address. `unknown`: everything else (a person's manual commit, history from before this change,
a bot, the app's own `Orbyn Crew <crew@orbyn.invalid>` merge, so a conflict resolution is `unknown` even if its message carries a trailer).
Reading an email can only produce `agent` or leave `unknown`, never `person`. The service stores all three values (`crew_hunks.author_kind`;
migration 55 rebuilds the table to allow `unknown` and copies old rows as they were). Coverage counts `unknown` with the agent's lines (the
wire shape is unchanged: `agent_lines` is "Agent or unconfirmed", and the screens say so). A wrong `person` label is worse than a missing one,
so nothing infers a person from a name or an email, a look-alike or upper-case address never matches, a repository `.mailmap` can only hide or
invent a worker address, and a listed commit that also carries a worker address stays `agent`.

Where the identity comes from: `RunSpec.crew_task` is set for a run whose thread is a Crew worker's, and the Claude, OpenCode (ACP) and Codex
drivers add `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME` and `GIT_COMMITTER_EMAIL` (`Orbyn Crew worker (<engine>/<model>)
<worker+<task id>@orbyn.invalid>`) to that process's own environment after `host::child_env::for_engine` (`agents/driver/worker_identity.rs`).
The engine and model in the name are cleaned (outside `[A-Za-z0-9._:/-]` becomes `_`, 64 characters at most) and are never matched on. A
process started ahead of the turn is not used for a worker, because it would lack the identity. The worker brief asks for the
`Orbyn-Task: <task id>` trailer, best effort: the person owns the repository's hooks and the app cannot enforce it. The identity is a label,
not a boundary: a worker can override it in its own command, and a shell profile that exports `GIT_*_EMAIL` wins over it for commands the
engine's shell runs (probe results are in the design note). Such a commit reads `unknown`, which counts as agent.

`crew_person_commits` (project, full commit id, time, action) is written only by `Store::record_person_commit`, in the same transaction as
its `crew.person_commit` audit row. Its one caller is project creation: `Core::new_project` records the README "Initial commit" it just made
as `project.readme`, after the project row exists, and a record that does not land never fails creation (the commit then reads `unknown`).
Nothing else commits on a person's action, so a person's manual commit is never listed; a future path must refuse a commit that contains a
worker-written change, which workers write into the person's own checkout, and only the path knows what it staged. No route was added.

**As built (slice 4n, scoring and history, no store):** `crew/backtest/` holds the pure part of the reviewer backtest. A pull request is eligible when it merged 30 to 180 days ago, both ends included. Outcomes come through an `EscapeSource` trait; until the escape log (slice 4k) is merged the only real source reports Not measured, which is not the same as no escapes. A lane catches an escape when it raised a finding on that pull request about a file the later revert or fix changed; matching is by file only because how a finding about behaviour matches a fix is not defined yet. Anything else is unmatched, never wrong. Recall shows from 5 escapes and precision from 10 findings, as a numerator, a denominator and a permille rounded down; below that only counts show. Nothing here starts a run; the store and replay follow below.

**As built (slice 4n, store and replay, migration 49):** `store/crew_backtest.rs` keeps a run (`crew_backtest_runs`: state, the person's per-engine token budgets as JSON, why it stopped), each lane's progress (`crew_backtest_lanes`: engine, model, tokens used, state, and the escapes it faced and caught once scored; empty means the escape log could not say, never zero) and each finding's outcome (`crew_backtest_results`: pull request, fingerprint, `caught`, `unmatched` or `no_escape`). Ids, counts and fingerprints only: no prompt, diff, file text, secret or money. `crew/backtest/replay.rs` replays review lanes and checks on the eligible pull requests through a `LaneRunner`; only a fake exists, so nothing live runs. The budget is tokens per engine and is never summed: a lane stops, and is recorded as stopped, when its own engine's budget is spent (lanes of one engine share it), while other engines go on. A lane whose replay breaks is recorded as failed and the others go on. A stopped lane is scored on the pull requests it replayed. With the escape source saying Not measured the run finishes and shows finding counts only. A request with a builder lane, a money-like field or name, an unknown field, or an engine without a budget is refused. A backtest starts only when a person chooses it: nothing schedules it or calls it at startup, and a test fails if code outside the backtest and its store calls the replay. The routes and the estimate are described next; the live runner is not built.

**As built (slice 4n, real escape source):** `crew/backtest/escape_log.rs` is the default `EscapeSource`, read-only over the escape log (4k's `crew_escape_page`, `crew_escapes_of_project`, `crew_escape_events`, `crew_merge`). Only `confirmed` escapes count (`possible` and `dismissed` do not), for merges in the 30 to 180 day window and found within 30 days of their merge. The pull request is the merge's record id, the same number the history uses. The files are what the escape's fix or revert commit changed, from `git diff-tree` through the hardened helper on the merge's checkout; the commit is the one the escape's `detected` event names (`revert:` or `follow_up_fix:`), and a revert falls back to the merge's `reverted_commit`. An escape with no such commit (post-merge check, a person's mark), no checkout or an unreadable commit is **unresolved**: counted per lane as `unresolved_n` (migration 53, additive, NULL when not scored) beside the score, never scored as a miss and never given a guessed file list; a finding on a pull request whose only escape is unresolved is stored `not_measured` and left out of that lane's precision. A fix that changed more than 2,000 files, or a non-UTF-8 file name, also makes an escape unresolved rather than shortening it. A log over its 5,000 cap, one damaged row or a store error is Not measured for the whole project, never a shortened list; a project with no escape row at all is Not measured too, because an empty log is not proof of no escapes. The estimate reads the store only (`measurable`); git runs only in the replay's blocking task. `NoEscapeLog` stays for tests. Nothing here writes the escape store.

**As built (slice 4n, routes, Preview):** four routes under `/api/crew/backtest`, behind the Preview flag. `GET estimate` (a project, and optionally lanes as repeated `lane`, `engine` and `model`) and `GET {id}/report` only read, and a remote viewer may use them. `POST start` and `POST {id}/stop` are a person's: local only, listed in `crew::changes::STATE_CHANGING` so Autopilot refuses an agent's request to them, and no tool, timer or scheduler reaches them. The estimate says how many merges (the project's own Crew merges, 30 to 180 days old) each lane would replay, whether outcomes can be measured, and which run is in progress; the counts are information and never change which engine the router picks (a test reads the routing sources and runs the pure router for that). A start names lanes (review or check, never a builder) and a token budget for each engine, as one entry per engine that is never summed; it carries no money field and any other field is refused. Only one run goes at a time: a second start while one runs is refused with a clear error, decided in the same store transaction that records the run. A stop only asks; the run stays running until its last replay is over, then ends as stopped and keeps what it replayed. No live replay engine is connected, so a start is refused until one is; tests use a scripted runner. The report shows each lane's state, tokens, finding count and, only when the escape log can say, its score; when the escape log cannot say (no escape logged, unreadable or over its cap) every lane reads Not measured and never a zero; a scored lane also shows `unresolved_n`, null when not scored. When the core opens, a run a crash left running becomes failed with the reason interrupted. Scoring now counts identical findings once, matches a path spelled with `./`, doubled or backslash separators or `..` segments to the same file (an absolute path matches only itself) and saturates a count past the integer range instead of wrapping.

**As built (backlog B3, the backtest screens on the 4n routes, Preview):** three of the 16 `backtest-*` frames read the routes in `web/src/features/crew/backtest/live`. `backtest-estimate` reads `GET /api/crew/backtest/estimate` and shows the project's own eligible merges, the 180-day window with the 30-day floor, whether outcomes can be measured (Not measured, never a zero, when the escape log cannot say) and any run in progress. `backtest-report` and `backtest-stopped` read a run named in the hash (`?id=`, the way an escape detail names its escape) and draw each lane's state, findings, tokens and score with its sample size; a lane the escape log cannot score reads Not measured, never a zero, a stopped run reads Partial with the service's own stop reason (`stopped by the person`, `token budget spent: …` or `interrupted`), and a failed or stopped lane keeps its own state. Nothing is written on load: the estimate and the report are reads, and no retry sends a write. No run can be created before the live replay engine (A5), so the report frames only open on a run already stored; there is none in production yet. The estimate's Start and the report's Run again stay Planned — drawn, disabled, with the reason stated. A start body needs each lane's `lane`, `engine`, `model` and its review-or-check `kind`, plus a token budget per engine; the estimate route echoes only lanes named in its query (always review lanes), the report serves a lane's name, engine, model and tokens but not its kind, and no route serves the project's backtest lane set or the person's per-engine budgets before a run exists. The app cannot compose that body without inventing it, so no start is sent and the service's refusal is not fabricated into the screen. The per-lane token estimate (A5 PR 3) and the per-lane replay counts are not in the routes either, so those read Not measured with a Planned pill rather than a number; the scores the report does carry are shown with their sample size. The other 13 frames — `backtest-settings`, `backtest-settings-unset`, `backtest-over-budget`, `backtest-running`, `backtest-vendors`, `backtest-escapes`, `backtest-unmatched`, `backtest-lane-failed`, `backtest-empty`, `backtest-low-history`, `backtest-lane-picker`, `backtest-narrow-panel` and `backtest-narrow-window` — stay drawn sample data and are tested beside the drawn frames. In Crew Designs v95 the two narrow frames show only what the report serves: one run card with the state, day, outcomes, lanes that scored and tokens per engine, and one card per lane with its state, escapes caught, findings raised with unresolved escapes and findings matched, while replays per lane stay Planned. The live screens are also reached from the app (below): the Preview route still opens them by frame id.

**As built (#760, a lane's kind and a project's start input, Preview):** a lane keeps the kind it was started as (migration 71, additive: `review` or `check`), so `GET /api/crew/backtest/{id}/report` serves it with every lane; a lane recorded before the column keeps no kind, and null there is never read as a review. `GET /api/crew/backtest/start-view?project=` serves the input `POST /api/crew/backtest/start` takes, as far as the service can compose it: the project's most recent run, its lanes with the kind, engine and model each had, and its per-engine token budgets, one entry per engine and never summed. Nothing is defaulted. A project that never ran a backtest gets `run: null` with no lane and no budget, so a screen shows "not set" instead of an invented set or a zero, and a budget the run does not hold reads unset. The read writes nothing, connects no engine and starts nothing, so the live lane runner stays A5 and a start is still refused after it. What it cannot serve is a project's first backtest: nothing stores a lane set or a budget before a run exists (the repo's `.orbyn/crew.json` names review lanes, but its `checks` are not read, and no engine, model or budget is stored beside a lane before a run), so the Start control keeps that reason until a person's settings write for those values is designed, and "Run again" on a finished run is composed from this read. The web page reads it in `web/src/features/crew/backtest/live` and draws the last run's lanes with their kinds, its per-engine budgets and its state, with no past-backtests list.

**As built (Crew entry frames, Crew Designs v87, orbyn-designs#363, behind `crew_preview`):** Settings › Crew draws four entry rows after Autopilot: Review capacity, Escapes (button "Open", read out as "Open Escapes"; opens `#/settings/crew/escapes`, the live escape log, whose header links to its Settings page, to Coverage and to the Possible tab; the design's "Defaults" value is not drawn), Independent oracle (Planned pill, its Open button drawn disabled with the reason: no code reads or writes it) and Reviewer backtest (`#/settings/crew/backtest`; its sentence says the live runner is not approved). Opening a row sends and reads nothing. The backtest page (`backtest/live/LivePage.tsx`; frames `entry-crew-backtest-page`, `-not-measured`, `-no-history`, `-running`, `-last-finished`, `-last-stopped`, `-loading`, `-error` and `-narrow-window`) reads `GET /api/crew/backtest/estimate` and `GET /api/crew/backtest/start-view`: what a backtest replays (fixed text), the eligible pull requests, whether outcomes are measured, and Open the estimate (`#/settings/crew/backtest/estimate`, disabled with its reason when none is old enough), then the most recent run's lanes with their kinds, its per-engine budgets and its state. A project that never ran one reads Not set with no lane invented, and a lane recorded before kinds were kept reads Kind not recorded, never Review. A run in progress swaps the not-approved line for a banner that opens its report (`#/settings/crew/backtest/report?id=`). The page draws no start and no stop, and there is no past-backtests list. The estimate's and the report's Settings buttons now return to this page. Each screen builds its links through `shared/nav.ts`, so the app's pages link to Settings › Crew pages and only the Preview route (`shared/preview-nav.tsx`) links to frame hashes; the routes are in `product/state/route.ts` (`crewPage`: `capacity`, `coverage`, `escapes`, `possible`, `escape-settings`, `escape`, `backtest`, `estimate`, `report`; `coverage` also carries `from`, the page that opened it).

**As built (Crew dashboard, W12, Crew Designs "23. Dashboard", behind `crew_preview`):** Settings › Crew draws a Dashboard row ("Open Dashboard") that opens `#/settings/crew/dashboard?repo=`. The page is one component (`features/crew/dashboard/`) that the twelve frames `dash-*` also draw, on a sample client, at `#/preview/crew/dash-*`. Live today: the revert rate, merges and reverts by engine and by area (`GET /api/crew/trust?project=`, 60 days), and Autopilot by area with its changes of mode (the existing areas, history and thresholds reads). A rate shows from 10 merges, otherwise the merges and "too few for a rate"; no merges at all says so and shows no zeros; cost is never added across engines. Planned on the page, drawn as a Planned mark and never as a number: autonomy rate, cycle time, cost, merges per week, the revert list, and the 30 and 90 day periods. Their read, `GET /api/crew/dashboard`, is served now (`crates/orbynd/src/routes/crew.rs`, answering to [crew-dashboard.md](crew-dashboard.md)); the page is built hidden and not wired to it yet (the production client sends nothing; the mock is `web/mock/crewDashboard.ts`). All projects reads each repository once (at most 24). The project and period are remembered per person. Not built: the Review tab that holds the dashboard in the designs (the page is a Settings page), and the Planned promotion row of `dash-area-history` (earning a higher mode is not built).

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
  Review queue. Each engine also has a daily budget for the crew (runs and turns, set at creation, reset at the person's
  midnight): a run in flight is cancelled at the cap with what the engine spent shown, and a new event is refused, never
  queued, with an audit row naming the engine and the time the day turns. The Background create form now sets the daily
  budget: Runs a day (2 by default) and Turns a day (80 by default), sent with the person's clock, and each says its
  range under the field while the value is out of range.

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
| Taskboard | Cards by state: Queued, Routed, Running, Verifying, Needs you, Passed, Stopped or Cancelled. A card is a brief: kind, engine, owner, last activity, and on a passed card the checks passed and total of its final verification (other states show none until the core keeps a record per verification attempt). From the card the person can reassign the engine, cancel, add a check, answer or open |
| Conversation lanes | One lane per worker over time. Messages show as arrows between lanes, each marked "live" (steered) or "next turn". The board's facts show with their author and evidence |
| Route card | Why this engine: the reasons, every rejected engine and why, and what the person can change |
| Needs you inbox | Questions, approvals, reviews and conflicting board writes, each with the one action it needs. Order: risk first, then the items that block nothing (a demotion) last within a risk tier, then oldest first. An **Autonomy** row appears when the circuit breaker demotes an area from Full to On; it blocks nothing, shows the trip's record, and offers Review, Override… and Dismiss (Dismiss is remembered per browser and repository, and returns only after a new trip). It reads the first Git repository only; with several repositories the inbox says so |
| Task report | What changed (the diff), what was verified, what was not, risks, cost, and a timeline the person can replay |

**As built (slice 5, the wide Map tree and the swim lanes, Crew Designs' `crew-map` and `lanes-main`):** `web/src/features/crew/map/MapView.tsx` draws the wide tree of frame `crew-map` from `GET /api/crew/flow/map`: the lead in the first column, every worker whose node names the lead as `started_by` under it with a solid line, every `FlowEdge` between members as a dashed line labelled with its kind and its newest delivery (never a message's text), and each worker's subagent report beside it (reported engines say "Subagents reported"; ACP and the rest say "Subagents not reported", "Not an empty tree: not reported."). A node shows the engine, model and effort, the task's state (the lead says Lead), Quiet or May be stuck, the engine's registry word when it is not Ready, the brief's outcome, elapsed time, the lease and unread; the details also say when it was updated. Selecting a node opens the details beside the tree (under it below 640 px) exactly as `crew-map-list` draws them. `web/src/features/crew/map/LanesView.tsx` draws the swim lanes of frame `lanes-main` from the map and message reads: one lane per member, the lead first, with the engine and model; the time axis is taken from the messages' own times (the 10-minute mark at or before the earliest and at or after the newest, widening the step rather than crowding it); every message is an arrow from its sender's lane to its receiver's, labelled with how it arrived and its fixed kind, and a message to everyone is drawn to each other lane and labelled once; each worker's brief outcome is a bar over its run. The shared board's facts stay beside them and the beta's message list stays under them. What no read carries is a Planned chip naming the missing read, never a value: per-worker tokens and a worker's branch on the tree ("no Map route reports per-worker tokens yet", "… a branch"), the subagent rows ("no Map route names a worker's subagents"), and a lane's level ("no route reports a lane's level"). The design frames `crew-map` and `lanes-main` still carry the designer's "Planned" note for the beta's nested list and message list; the next design version should drop it, since the app now builds both from the reads. The Flow task's artifacts, artifact diff and check results have their read (`GET /api/crew/flow/tasks/{id}/artifacts`, `orbyn_core::crew::flow::artifacts`): the evidence rows of the build under way, each with its check's id, kind, outcome, exit code, duration and the commit the checks read, the artifacts that row named, and the change the checks read as a file list with added and removed counts, pinned to the base commit the task started from (read-only git on the task's own checkout, tracked files only; a task with no checkout answers with the reason instead). It never carries the key's command, output, `proves` text or an Evidence id, and a check the task's key does not name has no kind. No web screen reads it yet, so the frames `flow-artifacts`, `flow-artifact-diff` and `flow-check-failed` stay samples, and three of their parts still have no source: no check producer stores an artifact, so a row names none; nothing records that an artifact's bytes were pruned, so `flow-artifact-pruned` has no answer; and a readable check name is the key's own text, so a row is labelled by its kind and id. `flow-gates` needs no new read: its gates are already in the Flow task zoom. A reviewer lane has no page of its own either: `lane-anatomy`, `lane-finding`, `lane-finding-states`, `lane-locked`, `lane-rules`, `lane-security`, `lane-add` and `lane-after-pass` need a per-lane read and route (what the lane was given: the diff and the check results; its findings' evidence, repro and confirmation) that no route serves, so a lane stays a row of the Flow zoom with its tier, the commit it read, the same-engine flag and its findings under it.

A reviewer lane has a read of its own — `GET /api/crew/flow/tasks/{id}/lanes?lane=<lane>`, `FlowLaneDetail` — but no page yet: it serves the row the zoom shows, what the lane was given (the work's purpose, the commit it read, the checks its brief named and how many of them passed) and the findings the lane filed in every round, each with the reviewer's own evidence. The pages `lane-anatomy`, `lane-finding`, `lane-finding-states`, `lane-locked`, `lane-rules`, `lane-security`, `lane-add` and `lane-after-pass` stay Planned until they are drawn, and a lane stays a row of the Flow zoom with its tier, the commit it read, the same-engine flag and its findings under it. Three parts of those pages are still not served: the diff the lane read (no commit-pinned diff read exists), and a finding's repro and confirmation (the lead's repro check is key material under H1, and a check's name and result need the artifacts route), so `lane-finding` stays Planned.

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
  - Owner reaffirmed on 2026-11-16: keep the High-risk rule. Conflicting design copy does not lower task risk.

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
  no read-only mode. The built-in profiles are "Careful", "Fast" and "Cheap".
- **Start a crew dialog:** Profile, Budget and Autopilot mode (Full by default) in one dialog.
- **Settings:** Crew and Secrets are both in the Settings Agents group, beside providers, tools, threads and tasks; there is no Privacy or Telemetry section.

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
| W1 | Command blocks | Terminal output grouped per command with exit status and duration. Copy, rerun, filter, find, collapse, and "Send to agent" (as a context chip) or "Ask Crew to fix" (as a brief) | Terminal drawer upgrade; uses shell integration marks. Web built hidden (copy, run again, collapse, exit status and duration; Filter, Find, Send to agent and Ask Crew to fix not built), behind a client that is absent in production until the service keeps per-command output: see [command-blocks](command-blocks.md) and [parity](parity.md) G17 |
| W2 | Smart terminal input | A real multi-line editor with cursor, selection and syntax colour; history search; completions for commands, flags and paths | Built (web, partial): the editor, history search and completions from a small command and flag table of our own, the person's history and the explorer's file listing. Path completion shows a loading row while a folder is read, and Shift+Ctrl+R widens history to the project's editors with the time of each row (no success mark: the terminal gives the editor no exit code). Typing straight into the terminal bypasses the editor, so the secret and engine checks apply only to what is typed in the editor. Fig's MIT completion specs are not used yet: check the licence text in the repository and keep its notice before adding any. No Warp code |
| W3 | Plain English in the terminal | Type a sentence and it goes to the thread's engine as a prompt instead of the shell, with a visible toggle and undo | Built (web, partial): a local classifier that is always shown and never silent. Unclear lines go to the shell, a secret-looking line is never sent, Undo is offered for 10 seconds and says whether it stops the run that started or takes the message out of the queue. The offline and signed-out notes have Try again and Sign in (the engine's own sign-in command is copied, as on the Welcome page). When the engine proposes a shell command, a card shows it as plain text with Run, Edit and Cancel; nothing runs without the Run click. The redacted frame waits for the Redactor work. Typing straight into the terminal bypasses the editor, so the secret and engine checks apply only to what is typed in the editor |
| W4 | Secret redaction | Secrets are masked in context sent to agents: the message, queued follow-ups, steers, review text and attached text files (up to 1 MiB, as a masked copy in the run's folder) reach the engine as `[hidden: label]`, while the person's thread keeps what was typed. Terminal blocks are masked too: the command, output and working directory of each block are redacted with the terminal switch, the recognised formats and the person's patterns before they are stored and again when they are read (`crates/orbyn-core/src/host/blocks/store.rs`, tests `a_secret_printed_across_two_reads_is_never_stored_or_answered` and `a_secret_in_the_working_directory_is_redacted`). Matches known key formats and the person's own patterns, set in Settings › Secrets. Crew keeps its own fixed rule: its scan, ledger, evidence and every Crew object use every built-in format and read no setting, so nothing on that page can put a secret into them. The person's patterns mask ledger text and its JSON keys, evidence text, and an audit row's JSON `detail`, only when it is shown (read routes), never in what is stored | Feeds Crew's hard rule "no secrets in any object" |
| W5 | Code editor tab | Files opened from the explorer get an editor: syntax colour, find, go to line, line comments (same comment threads as Changes), save. Language-server diagnostics come after the beta | New panel tab type |
| W6 | Changes: revert hunk, edit inline, attach to agent | Revert one hunk, edit a line in place, or send a selection to the engine | Extends the shell's Changes view |
| W7 | Batched review feedback | "Send all to engine" sends all drafts as one message, so the agent fixes everything in one pass | Built (beta) as the Review and comments panel tab (`features/panels/review/`). Drafts are the review chips already waiting above the composer (file and diff comments); Send all to engine asks first, then sends every draft as ONE message through the normal send path (a run when idle, the queue while a run is going), in the order written, and clears them only after the service accepts it. A failed send keeps every draft. The tab also reads the thread's linked pull requests (at most five, the open ones first) and shows their forge review threads under On the pull request and, behind a closed disclosure, Resolved; each item with a file and line has a jump button, shows the author and the first comment as plain text cut at 400 characters (the rest behind Show more), counts the comments of a thread, and follows the Filter by file select. A failed read says "Pull request comments could not be read." with Try again and leaves the Drafts and Open groups alone. Not built yet: per-comment send times and the agent reply marking comments answered (no data for them exists), and hiding secrets in the sent lines (the tab sends the lines as captured) |
| W8 | Execution profiles | Named presets: engine, model tier, permission mode and Autopilot mode. Picked per thread or crew, built in as "Careful", "Fast" and "Cheap" (there is no read-only permission mode and no built-in full-access preset). Contract written: [execution-profiles-and-library.md](execution-profiles-and-library.md). Service built: SQLite store, routes, Person classes and the `profile.*` audit rows. Web built hidden against a mock (`web/src/features/profiles`): the Settings › Crew list and editor, delete, and the composer picker with the one grant confirm; it shows only once `GET /api/profiles` answers (404 draws nothing). The web list client reads the service's bare array. Planned: Default profile, the in-use count before delete, the picker in the Start a crew dialog, the chip after a reload | Settings, Crew, plus a picker in the composer |
| W9 | Workflows, prompts and env sets | Saved commands with parameters, reusable prompts and environment-variable sets. Kept in a personal library on the device only; Orbyn never writes them into a repository, and a committed `.orbyn/` file is read-only. Environment-set values live in the OS keychain. Ties into B63 workflow recipes. Contract written: [execution-profiles-and-library.md](execution-profiles-and-library.md). Built in the service (`routes/library.rs`, the `library_*` tables, `credentials::env_sets`, suggestions from committed `.orbyn/` files, import and export, the run) and in the web (`web/src/features/library`, using the real HTTP client): a Library panel tab (From repo read-only, Personal), search, item pages, one form for all three kinds, Run and Insert through the service's render, and write-only environment values; it shows only once `GET /api/library` answers (404 draws nothing). Only the dev preview (`#/preview/library`) uses a mock. Not yet walked end to end in a browser against a real `orbynd` Planned: the last run of a workflow, a repository entry's Run (copy it to Personal first), applying a set to a shell, Reveal, Compare, import and export screens, number and yes/no parameters, multi-line values | Local, no server |
| W10 | CLI-agent attention | When an engine runs in a terminal (engine console), Orbyn notices it waiting for input and shows a notification and a Needs you item | Extends G14. Service detection built (the `CUSTOM orbyn.terminal_attention` event on the thread log, redacted before the append, with the quiet-time and fan-out decisions in [cli-agent-attention](cli-agent-attention.md)); web still built hidden (the thread's Needs you item with Open terminal and Dismiss, several terminals in one item, a secret prompt with Open alone, the toast and the system notification; Answer here, Yes and No, the terminal dot, the sidebar and header marks and the two settings are not built), behind a client that is absent in production: see [parity](parity.md) G19 |
| W11 | Run page for a PR | One static page per Crew task: prompt, plan, route, checks, findings and diff | Built (web, partial): a Run page for one Crew task, opened from the Flow task zoom, built on this device from the Flow read and the task's Run page read (`GET /api/crew/flow/tasks/{id}/run`) when the service answers it. A 404 keeps the Flow-based Planned marks; any other failure shows the existing error with Retry beside the page, not instead of it. The read fills the prompt (with its hidden and truncated notes), each check's outcome, exit code, duration, command, commit and what it proves, the not-verified lines, the commit the checks ran on, and the pull request number with the branch and commit the run published. Until the task is final the service withholds a check's command and proves text, and the page says "Withheld until the run finishes." in their place; the page treats `finished` as the signal and tolerates empty strings. Save as file downloads a static page (no script, no remote resource, everything escaped) and Copy PR comment copies capped, escaped Markdown; nothing is uploaded and neither action requests anything. The plan steps, the forge's own URL and head (and so the stale proof), the signed proof, Verify and opening from another device are Planned, and a Run tab on the pull request is built but hidden until a read says which task made it. Covered by component and unit tests against fixtures and the mock; not run against a real `orbynd`. Contract and the missing fields: [crew-run-page.md](crew-run-page.md) |
| W12 | Crew dashboard | Autonomy rate, cycle time, cost and revert rate per repository over time | Built (web, partial; row G18 in [parity.md](parity.md)): Settings › Crew › Dashboard draws the revert rate, merges and reverts by engine and area, and the mode each area is on with its changes, from `trust` and the Autopilot by area reads. A rate shows from 10 merges, cost stays per engine in its own unit, and a repository with no merges says so. Autonomy, cycle time, cost and the weekly bars are Planned until the read in [crew-dashboard.md](crew-dashboard.md) exists (Eng 2); the page draws a Planned mark, never a made-up figure or a zero |

**After the beta:**

- Language-server features in the editor (diagnostics, go to definition, references).
- Vim mode in the terminal input and editor.
- SSH and WSL sessions with the same blocks.
- Saved layouts that open a set of tabs, panes and worktrees.
- Codebase indexing for agent search. Local only, opt-in, with a privacy review first.
- Notebooks (runnable Markdown) and .ipynb viewing.
- Conversation export and import.
- Voice input on desktop.
- Agent memory across threads. Review memory (slice 4g) comes first; Crew lessons (KC5) are the Crew-only part, in the beta behind Crew Preview.
- Triggers from Slack, Linear, Jira, GitHub and GitLab for background crews.
- Local control, so outside tools can drive Orbyn through the `orbyn` CLI.

**Not adding:**

- Warp's hosted parts: cloud sync of Drive, public share links, hosted cloud agents and billing. Hosted services stay
  out of scope.
- A quake-style drop-down window.
- Default-terminal registration.
- Computer use, already covered by our gated Computer action.

## Kiro Crew features

Source: Kiro Crew's README and its crew-mode, dev-fleet and shared-crew specs (github.com/kirodotdev/KiroCrew, read
2026-11-23). Owner ruling (2026-11-23): take the ideas, copy no code or text, and Crew runs on **this device only**.

- **Licence:** Apache-2.0. We take ideas only; a pull request that uses one says "Sources: Kiro Crew docs, reviewed for
  ideas, nothing copied".
- **Already have:** scheduled and CI-failure background crews with budgets and a daily per-engine cap, Run now, live
  steering on the bus, a person's cancel, restart recovery, the dashboard, review stages, Autopilot and the escape log.
  Kiro has no hidden-key verification, no multi-vendor router and no merge trust record.

Every row stays behind Crew Preview, keeps the Crew rules (code decides pass or fail, the answer key never reaches a
worker, the secret scan on the way in and out) and builds no screen before its frame is merged.

| # | Feature | What you get | Owner | Parity |
| --- | --- | --- | --- | --- |
| KC1 | Steer, never queue | A lead's follow-up to a busy worker reaches its running turn. The bus already does this (`hand_over` to `steer_live`, `held` otherwise); the work is tests per message kind | E2 | M21 |
| KC2 | Closing the lead closes its workers | Closing or archiving a lead thread runs the person's cancel for each live task it started; the answer lists what closed and what did not | E3, E2 | M22 |
| KC3 | One "needs you" | One service field says whether a task, run or merge waits for the person; the board, inbox, Review queue and dashboard read only it. A worker that owns live work reads Working | E3, E1 | M23 |
| KC4 | Disk guard | A worker start is refused when the volume holding the lead's folder is under a floor (5 GB, person-settable), with the free space in the reason. No prune: tasks run in the lead's folder and correction checkouts are already retired | E3 | M24 |
| KC5 | Lessons | Repository or global lessons from a repeated failure fingerprint (2 tasks in 60 days), a confirmed escape or revert, and a person's typed correction. The person sees, edits, disables, deletes and promotes them; a "use lessons" switch per repository. At most 5 reach a brief as data; the key, route, lease and checks are proven unchanged. Recorded in `EvalHistory`, which the router does not read yet. Design: Eng 2's note, ruled 2026-11-23 | E2 | M25 |
| KC6 | Watches | A background crew kind `watch`: a fixed command on a schedule with no model call; it acts once when the result turns failing | E4 | M26 |
| KC7 | The missing listeners | `issue_label` and `dependency_update` background crews get listeners (refused today) | E4 | M27 |
| KC8 | Resume after restart | A task whose worker died in a restart resumes once from its last verified round with the same key and budget, then stops as today | E2 | M28 |
| KC9 | Skill proposals | Three lessons with the same pattern propose a skill in the skills workspace; nothing installs without the person | E2, E4 | M29 |
| KC10 | Device notifications | A desktop notification the first time something needs the person, once per item | E3, E1 | M30 |

**Resume after restart (KC8).** A builder or fix task left `running` (or `changed_approach`) after a restart
whose worker has no run and nothing queued resumes once in its life: it keeps the same worker thread, answer
key and budget, its lost turn counts as an attempt, and it is told to read the repository and continue from
its last verified round. Reviewer lanes never resume, and a second death or restart stops the task
`engine_error` exactly as before. Time the app was off spends no budget: the timer counts only up to the
task's last recorded event, and `downtime_secs` keeps that off time out of later budget checks. The resume
writes `crew.task.resumed` in the same transaction as the state, attempt and resume counters; a stop writes
`crew.task.restart_stopped` with `why` (`reviewer`, `already_resumed`, `not_resumable` or `send_failed`) in
the same transaction as its `stopped`/`engine_error` move.

**As built on 2026-11-23 (main `a48e69e06`):** KC1 tests merged (the bus already steered). KC4 built end to end except the background history row (service #1330, web #1339). KC3 core read merged (#1346); its board, digest and dashboard switch and web are not built. KC8 core merged (#1344) with the web card line (#1340). KC2 web is built and hidden (#1342) and its service side is built (not run against a real service). KC5 has its contract (#1341) and the web list (#1345); its core is not built. KC6 has slice 1 of 3 (#1343). KC7, KC9 and KC10 are not built. All frames are drawn and merged (Crew designs v121). Per-row detail is in [parity.md](parity.md) M21 to M30.

**Not adding:** Docker, a remote host, a crew shared between people, Slack or Discord, inbound webhooks, model fallback
(our router never swaps an engine) and release channels.

## Beta cut

Decided 2026-11-11. The big beta ships everything together: one release carries all three of Crew (the cut below),
every unfinished app row ([Appendix A](crew/beta-rows.md#appendix-a-every-unfinished-beta-row-app-parity), 137 rows from
[parity.md](parity.md)) and every designed screen not yet built ([Appendix B](crew/beta-rows.md#appendix-b-designed-screens-not-yet-built);
its list is a 2026-11-11 snapshot, and the 2026-11-20 recount in [design-inventory.md](design-inventory.md)
finds 44 of the 231 Phase 0 screens still unimplemented: 38 missing, 4 preview-only, 2 unverified).

Rows the shell redesign replaces (for example C31's add-tab menu, and the Side panel tabs and Files screens) are built
to the new shell designs (`engineer-designs.html` after the QA fixes merge), not the old frames. Excluded scope
(mobile, remote pairing, hosted services) stays out unless the owner says otherwise. Not-doable rows keep their recorded
reason.

**In the Crew beta (Preview):**

- Phase 1 wiring: optional Orbyn account on staging, with personal settings syncing across devices
- all twelve Warp features W1 to W12 (above); W2 and W3 are partial (see [parity.md](parity.md) G15 and G16)
- Slice 0: protocol v0 spec and schemas (not yet published)
- Slices 1 to 4: contracts, router v0 on defaults, runs, verifier, bus, leases
- 4b: review stages
- 4e: shell redesign
- 4f: Autopilot (Full by default), merge queue and watch on main; the after-merge Review queue's backend ships, but its screen is not in the beta (the Designer's ruling, in orbyn-designs `audit/pass-19-capacity-notes.md:24`; see also "the Review queue is not in the beta" under the slice 4l note below)
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

## As built (re-run checks)

The person's "Re-run checks" for a Crew task (Preview). Backend only: the card that calls it is Engineering 1's.

- **What exists:** core `Core::crew_rerun_checks(task, reason, actor)` and `POST /api/crew/tasks/{id}/rerun-checks` (body empty, or
  `{ "reason": "..." }`, 1 to 200 characters, redacted). It runs the answer key's checks again through the verifier (`Core::judge`, the
  code behind `crew_verify`) and changes no state (SPEC.md H7). With the Preview flag off the route is 404 and the body is not read.
- **It stores no Evidence and no Gate.** Review counts every Evidence of a task as its own verification ("N of M checks passed on the
  work", and the reviewer's commit is the last Evidence's), the merge proof reads the same rows, and the protocol's Evidence has no
  field that says "a person's re-run", which this PR does not change. So a re-run's record is its two audit rows, counts only; its
  logs are not kept. A later Flow route that shows a re-run needs its own table or a protocol field.
- **What runs.** The key's checks only, in the task's own checkout: the command kinds, `file_scope`, `diff_size`,
  `schema`, `secret_scan`, `repo_required` and `http_probe`; any other kind is `skipped`, as in a live verification. A `repo_required`
  check runs the repository's own checks (`.orbyn/crew.json`) unscreened, as the merge queue runs them; the re-run does not
  otherwise touch the merge queue. A key with protected paths is compared with the record taken when the task started, so a checkout whose protected
  files changed runs nothing, as a live verification does (N7, H9).
- **Which tasks.** An unknown id is `404 not_found`. A task with no key or no checkout is `422 unsupported`. Allowed: a `passed` task,
  and a `stopped` task whose reason is `repeated_failure`, `budget` or `engine_error` **and** whose checks ran live at least once (it
  has Evidence). Refused with `409 task_busy` and a plain reason, and nothing runs: a task stopped for `injection_suspected` or
  `too_many_denials` (Autopilot stops a task for those from `running`, so the key never ran in that checkout and the worker may have
  planted what the key's commands execute), for `lead_stopped` (the lead's reason is not known), for any reason not on the allow-list,
  or never verified; a task being worked on, being verified, `cancelled`, or whose worker still has an engine run open; and one whose
  checks are being run again. The table admits six stop reasons today; the allow-list means a seventh is refused until someone names it.
- **Limits against spam.** At most one re-run at a time on the device, across all tasks, and 10 seconds between the end of one re-run
  of a task and the next (`409 task_busy`, "Try again in N seconds"). The claim is `crew_runs.verifying_since`, the column a live
  verification uses, so a re-run and a verification never run together; a claim on a `passed` or `stopped` task can only be a re-run's,
  so the device-wide check is one query. A start of the service clears every claim. This is not a presence grant: none was asked for.
- **Order, failing closed.** (1) One transaction checks the task, takes the claim and writes the intent row `crew.task.rerun_checks`
  (actor, task id, state, the redacted reason): the row comes before any command (architecture rule 6), and a refused request writes
  nothing. (2) Every command of the key is **screened**. (3) The verifier runs the rest. (4) One transaction writes
  `crew.task.rerun_checks_done` (counts only: total, ran, passed, refused, skipped, the gate) or `crew.task.rerun_checks_failed` (the
  error code) and lets the claim go. The run is a task inside a task: a panic in it ends as the `_failed` row and a free claim, and a
  caller that hangs up changes nothing. A crash of the process after (1) leaves the intent row with no outcome row, and the next start
  frees the claim.
- **Command screen (new).** Before this, the core's own check commands were not screened: the verifier ran a key's commands as written,
  and the merge queue still runs a repository's. The re-run screens each command of the key with the same reading a worker's command
  gets at the gate: the always-denied list N1 to N10 (`autopilot::screen`, which calls `refuse_in`), then the blast-radius guard
  (`guard_of`, with this device's two lists). A command that fails either is **not run**; its check is an `error`, so the verdict cannot
  pass on the others. It fails closed: an empty command, one with a quote left open, one the parser finds no command in, or a guard list
  that cannot be read is refused. The screen is heuristic like the gate (see "As built (slice 4i, guard)"): a script the command runs,
  an alias or a `psql -f` file is not read. There is no approve-once for a refused check.
- **Answer.** `{ task, state, total, ran, passed, refused, skipped, gate: pass|fail|none, refused_checks? }`. No check command, output,
  excerpt, `proves` or `does_not_prove`, no rule name and no Evidence id is in it (H1). `refused_checks` is the ids of the refused checks,
  only for a task that is final (every task allowed today is); the count is always there. `gate` is the verdict of this run, or `none`
  when it decided nothing (every check skipped, or a protected file changed). The state is the task's own, unchanged.
- **Bounds.** The whole run gets the shorter of the task's time budget (fresh, not what was left when it ended) and 30 minutes
  (`RERUN_MAX`); a check past it is stopped and counts as an `error`. Nothing of a check's output leaves the verifier.
- **Not built:** the card (Engineering 1); reassign, message into a lane, add a lane and a person's answer, which are out of the beta;
  showing a re-run in Review or the Flow routes; re-running the repository's own checks; re-running a task that is not over.
- **Known limits.** *A person is not required.* The route is `local_only` (a remote viewer is 403) and in the N1 marker, but any local
  process holding the launch token (the lead, an unconfined engine) can `POST` it, as with cancel; the audit row says `user` whoever
  sent it. The device-wide and per-task limits bound it; the presence grant is not used. *Worker-written files.* A `passed` task's
  checkout, and a `stopped` one stopped for `budget`, `repeated_failure` or `engine_error` (a worker can bring those stops about on purpose after one live verification and then write files the key's commands execute), can still hold files the worker wrote outside the protected paths that the key's commands execute (`package.json` scripts,
  `conftest.py`, `build.rs`, a Makefile target): the live verification had the same exposure, and the re-run adds the person's trigger,
  not a new kind. If the outcome row cannot be written, the claim stays set and blocks every re-run on this device until the service restarts (rare: it needs a database write error; a warning is logged). The screen narrows what a command may do and does not sandbox it; the commands run with the person's permissions in
  the worker's checkout, and may write there. *Cancel.* The cancel route refuses a task that ended (`409 task_ended`), so a cancel cannot
  land on a task being re-run; a task being verified or worked on is not re-run. A re-run does not re-decide a task: a `stopped` task
  whose checks now pass is still `stopped`, and merge still needs a `passed` one.
- **The web card.** A page reload during a re-run loses what the window shows (its busy state and the answer) while the service goes on running the
  checks: the answer is not kept, and a window that asks again within the gap, or while a re-run runs, gets `409 task_busy`.

## Worker approvals

- **Approvals wait up to the approval timeout (30 minutes by default).** Claude Code asks Orbyn through one long MCP call and drops an HTTP MCP request that has no response after 60 s unless the server's `timeout` is higher, so a worker's or reviewer's approval used to be expired as `system:abandoned` after about a minute. Orbyn now writes `timeout` (approval timeout plus 60 s) into each run's `--mcp-config`.
- **Full access on the lead thread does not cover crew workers.** Crew mode governs a worker's commands, so a worker can still ask for approval while the thread it came from is on Full access.

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

A Codex worker runs inside Orbyn's profile: Codex's own sandbox is turned off (`danger-full-access` on the
thread and each turn), because a Seatbelt profile cannot start inside another. The saved approval policy is
unchanged, so Codex's command and file approvals still reach Orbyn's ApprovalGate and the crew write-scope rule.
Each crew task gets a private Codex home at `<data>/crew/codex-homes/<task id>`, mode 0700, at launch holding only a
mode-0600 copy of `auth.json` from the real Codex home; no config, history, sessions or SQLite file is copied.
Its later turns reuse the home, so its own session continues, and the home is removed when the task ends and
swept at core start when its task is not live. The real Codex home (`CODEX_HOME` or `~/.codex`, plus the
account's home from the user database when it differs) is closed to the worker. The beta allows this only
when the login is an API key: a ChatGPT sign-in, or an unknown or malformed auth document, is refused with
"Codex workers need an API key; your ChatGPT sign-in would be shared with the worker." and no private home is
made. A separate worker login is post-beta.

Limits on this, by ruling: a worker on any other engine (an ACP agent) gets Claude's `projects/` and
`history.jsonl` denied with no allowance, so an ACP adapter wrapping Claude Code cannot be a worker for now; auto memory
is closed to workers, which start without it; a start folder that does not resolve, is non-ASCII or 200 or more characters,
or a symlinked `~/.claude`, `CLAUDE_CONFIG_DIR` or any folder above it refuses the worker with a plain line. Codex's own
saved conversations are covered by the closed real home, and a worker's own home holds only its own task's session;
other engines' saved conversations are not covered. A confined Claude worker can create its own `<own id>.jsonl` and `<own id>/` in any existing project folder, not only its own project's, so a session it wrote can appear in another project's resume list. The claim is narrow: the Claude
lead's transcript is denied to confined Claude workers, and nothing is said about other leads.

Built: the macOS profile, the refusals (other platforms, missing tool, nested restriction, Codex while
`CODEX_WORKERS_ALLOWED` is false, terminal engines), the Claude transcript denial, and the Codex design behind
that one constant: nested sandbox off, a private API-key home per task, the real home closed, and tests that run
the real tool. Not built: a Windows or Linux restriction, and a denial of other engines' saved conversations. Not
covered: an ACP agent's saved conversations, Claude's other per-session files (`plans/`, which can hold the lead's plan text, `paste-cache/`, `session-env/`, to-do
lists, file history, debug logs, shell snapshots, `sessions/`), the network, the loopback service, the keychain, `~/.claude.json`, other same-user files
and processes a worker leaves running. A worker can learn which project folder names exist under `projects` (their metadata may be read), not what is in them. The lead's Claude transcript is closed for a Claude worker; the answer key as a
whole is not claimed safe. Codex workers stay refused (`CODEX_WORKERS_ALLOWED` is false) until the real-Mac
probe passes; the design above is built and tested behind that one line.

**Codex probe, macOS (2026-11-22, codex-cli 0.166.0, Mac OS 27.2.1 arm64).** Run by hand:
`cargo test -p orbyn-core --lib codex_app_server_starts_under_the_profile -- --ignored --nocapture`. Observed: the real
`codex app-server`, started under the worker profile with only its private `CODEX_HOME`, answers `initialize` and reports
that private folder as its `codexHome`; the real home and its files are untouched; the sandbox-exec tests show the real home,
another task's home and the data folder unreadable from inside the profile while the worker's own home is readable and
writable. **Not observed:** a model turn that runs a command with Codex's own sandbox off (needs an API key and the network),
so that Codex starts no nested Seatbelt of its own is still unproven, and no Linux or Windows run exists.

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
- **A second stop after the state change.** Start and retry do not take the cancel's lock, so a worker may have been recorded, or a
  message queued, after the cancel read the task. Once the task is `cancelled` the worker is read again and stopped once more (with
  its queue emptied); a later start or retry meets its own final-state guard. If that second stop fails, **the task stays
  `cancelled` with a worker that may still run**: a `crew.task.cancel_failed` row flags it and the caller gets `502
  engine_unavailable` ("The task is cancelled, but its worker could not be stopped"). Repeating the cancel answers `already: true`,
  so the person stops that worker from its thread.
- **Wording.** When the worker's run end moves the task first, its final result still says the person cancelled it if the cancel's
  intent row exists.
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

## As built (close lead)

Closing or archiving a lead thread cancels the live Crew work it leads (KC2, Crew Preview). Backend; the web dialog, toasts and
the person key were merged first (#1342).

- **What exists.** `Core::crew_close_lead(thread_id, actor)` -> `{ closed, failed }`, ids only, and
  `Core::crew_lead_live(thread_id)` -> `{ tasks: [{ id, name, state }] }`. Over HTTP: `GET /api/crew/lead/{thread_id}/live` (a read,
  open to a remote viewer) and `POST /api/crew/lead/{thread_id}/close` (`Person`, behind `local_only`). Both hide behind the Preview
  flag exactly as every other Crew route. The close is audited by the cancels it runs, one intent row and one outcome row per task.
- **What a close tries.** The non-final tasks (`briefed`, `routed`, `running`, `verifying`, `failed`, `changed_approach`,
  `needs_person`) whose `crew_runs.lead_thread_id` is this thread, oldest first. A task another thread leads is never one and a task
  that already ended (`passed`, `stopped`, `cancelled`) is skipped. One private function chooses that list for both routes, so the
  confirm's count is what the close tries when nothing starts or ends in between (a task that ended meanwhile is skipped, and a
  task it tries may fail to stop: `closed` and `failed` answer which). The close chooses again when it runs: a task that starts on
  the thread after the confirm was read is cancelled too (`a_task_started_after_the_confirm_is_cancelled_by_the_close`).
- **One by one, the audited cancel.** Each task runs the existing person's cancel (`crew_cancel_task`) with the reason
  "The lead thread was closed.": the intent row first, the worker's engine stopped and confirmed, then the state and its row, and the
  task's leases, final `cancelled` result and end all as any cancel. An `Ok` is a task closed; `TaskEnded` (it ended between the read
  and the try) is skipped; any other refusal is a task named in `failed` and the loop goes on, so one stuck worker never stops the rest.
- **The thread closes regardless.** The thread's own settle or archive is `PATCH /api/threads/{id}` (`settled` or `archived`
  true). It is not a Person route and cancels nothing: a token-only caller must not be able to stop a task by settling a thread.
  The web dialog POSTs `/close` first and then sends its PATCH, whatever the close answered. A task that could not be stopped keeps
  its state, so its worker may still run.
- **The lead is not told.** A cancelled result normally queues a note on the lead thread, and a note on an idle thread starts it: a
  close therefore tells the lead nothing. `tell_lead` skips a cancelled result whose latest cancel intent row says
  "The lead thread was closed."; a cancel with another reason still tells it. Without this the note would start the thread the
  person is closing and the patch would answer `409 thread_busy`.
- **Could not stop.** A task whose last cancel attempt failed is an item of the person's *needs you* inbox with kind
  `could_not_stop` (not dismissible): the read joins non-final tasks to their latest `crew.task.cancel_failed` row with no
  `crew.task.cancelled` row after it, and shows that row's time as `since`. It adds no storage and moves nothing. The item clears when
  the person stops the task again and it ends `cancelled`.
- **Not run against a real service.** The routes, the agreement between the live read and the close, the failed-task path, the inbox
  item and the thread's own close are covered by fake-engine and route tests; the web half hides until the Preview flag is on.

## Open risks

- Steering differs by engine. A Claude message may land mid-tool-call. Test with real engines before claiming "live".
- Login and quota are per engine on the person's machine. The router must fail clearly ("Codex isn't signed in") rather
  than silently falling back.
- Cost: several engines at once multiply usage. The crew-wide turn budget (slice 3 above) refuses new tasks and
  never stops in-flight work, so a crew over its number finishes what it started.
- openbot is non-commercial: no code or text from it, only the ideas listed above.

## Appendices

The row tables are in [crew/beta-rows.md](crew/beta-rows.md):

- [Appendix A](crew/beta-rows.md#appendix-a-every-unfinished-beta-row-app-parity): every unfinished beta row (app parity), 137 rows.
- [Appendix B](crew/beta-rows.md#appendix-b-designed-screens-not-yet-built): designed screens not yet built, from a 2026-11-11 snapshot; the 2026-11-20 recount in [design-inventory.md](design-inventory.md) finds 44 still unimplemented.
- [Appendix C](crew/beta-rows.md#appendix-c-unfinished-rows-that-have-no-design-yet): unfinished rows that have no design yet.

**As built (slice 4l, coverage, contracts and routes, migration 50):** `crew/capacity/coverage.rs` counts the lines agents wrote in merges, reviewed when the merge is Looks good or Commented (merge-level, never line-level), in the queue when it waits with High or Medium risk. A merge before its repository's provenance start, with no hunk or no note, is Not measured and adds no line. Migration 50 adds `pause_enabled` (default off): `capacity_start_check` lets every start go until a person turns it on. Review fix tasks and merge-comment fixes set `Links::crew_started`, stored as Low and Crew-started; a person's start and review lanes stay Medium and person-started (background crews have no start path yet). Routes: `GET /api/crew/capacity` and `GET /api/crew/capacity/coverage` are Local reads; `POST /api/crew/capacity/settings` and `POST /api/crew/capacity/tasks/{id}` are Person writes behind `local_only`. The refused starts are listed since migration 54 (below). The screens are wired in the next paragraph.

**As built (slice 4l, web wiring, matched to Crew Designs v77):** `web/src/features/crew/capacity/live/` draws two screens from the two Local reads, at `#/preview/crew/capacity-live-settings` and `capacity-live-coverage` in a Preview build, only while the service's Crew Preview is on. The screens read the project list themselves, because a Preview hash opens before the app shell loads the workspace: "Reading projects" while it is out, "Can't read the projects" with Retry when it fails and "No project to show" (nothing else) when it is empty; no capacity route is read until a project is known. The settings sheet is `capacity-settings` (its "What the limit does and doesn't do" box only once a limit is saved), with its `-unset`, `-saved-off`, `-paused-banner`, `-loading`, `-error` and `-no-plan` states; the paused banner is on that sheet, and `capacity-over` is not built (the Review queue is not in the beta). A saved limit with pausing off reads `Off` and `Review every batch` (the `· default` suffix and the shorter pause and low-risk help text are only for the page before a limit is saved). The coverage map is `capacity-coverage`, with `-empty`, `-not-begun`, `-gaps` (partial: the banner only, no button, no lists and no split by reason, because the service sends one Not measured total), `-gaps-narrow` (partial: the banner wraps; the stacked lists are Planned), `-low`, `-error`, `-loading`, `-picker` and `-picker-narrow` (partial: the chip and menu are built; the chip on its own row and the map as cards are not). Coverage shows the exact counts and, whenever any merge is Not measured, the gaps banner; there is no low-sample cutoff and no "thin" or "rests on" wording (the cutoff is an open product question). A failed read draws the red banner, the raw error line and one dash tile. A person with several projects picks one from the chip: choosing reads that project's coverage and sends nothing else; with one project the chip is plain text. Engine labels are the service's own, `claude · Sonnet 5.5`. An editor on that sheet, the design's `capacity-settings-editor` with `-editor-narrow` and `-editor-error`, writes the limit, the low-risk sampling and the pause switch: `Change the limit` (or `Set a limit` before one is saved) turns the three rows into the design's own controls (a stepper for the limit, the pause switch and the two low-risk choices) with `Cancel` and `Save` under them, and the coverage row stays read-only. Nothing is sent before `Save`, and only a person's own click presses it, through the Person-only `POST /api/crew/capacity/settings` in `live/api.ts`; a load, a refresh or a retry of a read never sends it. After a save the rows return to their read-only values from the service's answer. A failed save keeps what the person entered and says why in one banner, `Can't save review capacity`: the service's own message for a refusal (a limit of 0 is refused by the service, and the page shows its words rather than inventing a minimum) or the network's words when the service is not there, and `Try again` sends the same values once more. The editor stacks each control under its row's text below the design's 640 px window. No control writes a task's priority: `POST /api/crew/capacity/tasks/{id}` has a client and no designed beta screen sends it. **Reached from the app:** with the service's Crew Preview on, Settings › Crew has a Review capacity row (title, what it is for, "Open Review capacity", as the `settings-crew` frame draws it) that opens `#/settings/crew/capacity`, the live settings sheet; the design's value on the row ("Not set") is not drawn because the page picks its own project. With Preview off the Crew category is not there and no capacity route is read. The Review capacity sheet's Review coverage row has a Coverage link (Crew Designs v87, `entry-crew-coverage-link-capacity`, with its `-narrow`) that opens the coverage map at `#/settings/crew/capacity/coverage?repo=<project>`, for the same project; the escape log header has the same link (`entry-crew-coverage-link-log` and its `-narrow-window` and `-narrow-panel`). The "Back to" controls the Designer is still drawing are not built; the browser's Back returns to the page the person came from. Planned, drawn as a Planned pill beside a disabled control or as a heading with no rows, never with invented data: the lists of Not measured merges and `Try the 2 again` (the coverage read sends one Not measured total and no per-merge row, so the lists cannot be assembled honestly; nothing retries a note, so the retry stays Planned whatever the count), the link from an area to its files (the read carries an area's counts, not its files) and `Open the Review queue` (the Review queue is not in the beta). Missing data reads "Not measured" (provenance never began; an area with no lines, in every cell; an engine with no model); coverage stays merge-level. Not drawn: the page head, tabs and breadcrumb the frames put around the coverage page (the app draws the page alone), and the narrow-window and panel layouts of the map as cards. The paused-board, paused-card, resumed, sampling and digest frames have no real data and stay sample-only; the sample frames `capacity-settings`, `-settings-unset`, `-coverage`, `-coverage-gaps`, `-coverage-empty`, `-coverage-low` and `-coverage-error` follow v77 wording.
