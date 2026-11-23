# Autopilot

Slice 4f of [docs/crew.md](../../../../../docs/crew.md#autopilot), behind `CoreConfig::crew_preview`. Backend only: no
screen is built from it. The lead approves what an agent asks for; code, not the lead, limits what it may approve.
The second half of the slice is the merge queue, the after-merge Review queue and the watch on main, in [`merge/`](merge).

| Module | Owns |
| --- | --- |
| `mod` | `Mode`: `off`, `on` or `full` (the default). The person chooses it per crew, repository or as the default |
| `hard` | The always-denied list N1 to N10 ([SPEC.md 7.1](../../../../../docs/crew/protocol/SPEC.md#71-never-allowed)) and its order |
| `hard_tools` | N5, N6, N8, N9 and N10: tables of programs and sub-commands |
| `shell` | Reads a command: simple commands, wrappers, `sh -c`, redirects, paths, hosts. Not a shell |
| `hosts` | `allowed_hosts` of `.orbyn/crew.json`, read at the commit the work started from |
| `tier` | What an action needs: nobody, the lead, or the lead and a second key |
| `chain` | The approval chain over the `Approver` trait: the lead, then a second key on another vendor |
| `notices` | Server-persistent missing-second-key notices, cursor paging and idempotent dismissal or sign-in resolution |
| `engine` | Per-request lead selection and a lead-tier second vendor through the registry, filters and data classes; strict JSON answers from bounded tool-free runs |
| `comment` | Saved lead-requested advisory review-comment claims through the same hard/host rules and two-key chain, before ordinary gate policy; no worker resurrection or held-command grant |
| `tripwire` | 3 denials in a row, 20 in a run, and the injection detector |
| `ledger` | Writes the approval ledger rows: redacted, the key's text withheld, scanned |
| `held` | A command the chain denied, held in memory for a person: approve once, deny, dismiss. No agent approves; a person may approve once |
| `presence` | The shell messages that carry a person's approval of a hold (`presence_describe`, `presence_grant`) and what they check |
| `review` | `Core`'s side: the mode, the approvers, and `ActionReview` for the gate |
| `areas` | Autonomy by area (slice 4j): the circuit breaker's pure rule and its writes, what the merge gate reads (`effective`), a person's override and follow (`person`), a failed breaker's retry (`retry`) and the reads |
| `merge/risk` | The risk tier of a diff, with its reasons. Pure |
| `merge/holds`, `merge/guard` | What holds a merge: the person-only diffs, the owed lanes, the mode now |
| `merge/queue` | The merge queue: score, mode, one merge at a time, checks again on the combined state |
| `merge/forge`, `merge/evidence` | Durable forge intent, current-head CI receipts and the code-written merge Gate |
| `merge/tool`, `merge/comment` | Lead requests and the person's Comment through ordinary scoped task admission |
| `merge/correction` | Core-owned deterministic rollback tasks, retained candidate checkouts and normal reviewed forge publication |
| `merge/forge_watch` | Remote-tip checks, an optional owned watch future, and durable digest/overdue/break messages; the digest is due once a local day at the time `merge/digest` reads |
| `merge/digest` | The daily digest's send time (issue #806): the stored `{time, utc_offset_minutes}`, the one local day it may fire on, and the person's save |
| `merge/review_queue` | The after-merge Review queue and the person's answers, plus the background runs that stopped without a merge and wait for the person. It only reads them; the person settles one by acknowledging it through `crew::background::background_acknowledge_review`, and a settled review leaves the queue. Since KC3 it also reads the one needs-you field for the waits it does not answer (`waiting_elsewhere`, by kind), so the Queue's page and the Inbox agree on what else waits |
| `merge/watch` | Watch on main: the repository's checks, the bisect, the revert |
| `merge/api` | The views the HTTP routes read, the shape checks of the person's answers, and the Preview gate. Decides nothing |
| `merge/gitops` | The git behind them: scratch checkouts, merge, revert, compare-and-swap of the branch |

The Crew marker is [`crew::changes`](../changes.rs): the Crew routes that change state, which N1 refuses by path too (any path under `/api/crew` that is not a listed read).
The account marker is [`account::changes`](../../account/changes.rs): the list of account routes that change state,
which N1 refuses by path. `orbynd`'s route-table test fails when a route under `/api/account` is in neither list.

## How an action goes

`ApprovalGate::request_inner` asks `ActionReview` before any rule or person (bound by `McpService::bind_host` only
when the Crew Preview flag is on). For a Crew worker or reviewer:

1. **Ended task:** denied.
2. **Injection trip wire:** a request whose summary, target or (outside a file change) detail reads as injected
   instructions stops the task, `injection_suspected`. The ledger keeps the class, never the text.
3. **Always denied:** N1 to N10, before any approver, rule, mode or person's saved allow. The lead is never asked.
4. **Lease:** every path a file change names must lie in the lease the brief granted this worker; a refusal is the lease's
   own (H10) and refuses the write, never the task. It is recorded as a denial (`decided_by: lease`) and counts toward the
   trip wires (rule 7), so a worker cannot retry out-of-lease writes without limit.
5. **Tier:** reads and writes inside the leased paths and the brief's own check commands need nobody. Network, installing
   dependencies, running a script or code the worker wrote, and CI files need two keys. The rest is the lead's.
6. **Chain:** the lead answers a [`Summary`](chain.rs) (kind, target, paths, the brief's outcome, the tier and why), never
   raw page or tool text, and must give a reason (an approval without one is a denial). Two keys: a second approver on
   another vendor (`family`) must also say yes. A disagreement is denied and logged as its own row. A person's deny rule
   still wins over an approval, and an allow rule or no rule lets an approval through.
6b. **Held for a person:** a denied command is held ([`held`](held.rs)). The person may approve that exact command once (bound to its
   text, arguments, paths, thread and task; one retry; 10 minutes), deny it or dismiss it, from the app's window only. The retry is checked
   against the always-denied list first and then lets the one approval through, with a ledger row (`decided_by: person`). Approving needs
   the person's click in the desktop app's own dialog (`presence`). Nothing on the always-denied list is ever held.
6c. **Blast-radius guard** ([`blast`](blast.rs), [`targets`](targets.rs), [`exit`](exit.rs)): a destructive command (file deletion, database, cloud, containers, git) against a
   production-linked target is tier `guard`, above the check commands and leases and below N3 and N8. Under On or Off it is denied and held for the person; under Full the lead and a
   second key on another vendor must both say yes. The person's one-time approval covers it. The lists of `prod` and `safe` targets are a table on this device, never a repository
   file; loosening them needs the desktop presence grant (the dialog is not built yet). A `command_exit` row records the exit status, or `unknown`. See docs/crew.md, "As built (slice 4i, guard)".
7. **Trip wires** after each denial: 3 in a row or 20 in a run stop the task, `too_many_denials`. "In a run" is counted per task
   (a task has one run). Only an approval by the lead or by two keys ends a row of denials: a read that needs nobody does not,
   so a worker cannot put `ls` between its denials.

Outside a Crew task only N1 (the account, and the Crew routes that change state) is enforced, so an ordinary thread cannot call a state-changing account or Crew route. N1 looks
at requests, not at files: a network, browser or tool action that names a route, or a command run by a connecting program
(curl, wget, an interpreter) that does. A file change, a search or a read of a document that merely names `/api/account/...`
is not refused, so ordinary work does not trip the wires. N1 reads spelling only: `//`, `/./`, `/x/../` and quotes inside a word are undone, but a path built from a variable, string pieces or a script the agent wrote is not seen; that is the sandbox's work (#441).

Under `off` every request goes to a person, always-allowed reads and leased writes included: that is "every request", and the hard
rules still refuse first.

N1 reads a command as a whole when a connecting program (curl, wget, nc, socat, openssl, gh, glab, an interpreter, ...) is in it,
so a route that comes through a variable or a pipe is seen. `gh` and `glab` are network, two keys; `gh pr merge`, `glab mr merge`
and a `gh api` write to git refs, merges, contents or branches are N5 (a person's), and to protection or settings N6. A write is read the way gh
reads its flags (`-XPATCH`, `-fsha=x`, `--field=x`, `--input=f`). A GraphQL call with a `mutation`, or a query read from a file, is N5. `gh alias set|import`
(an alias hides `pr merge`) and `gh extension install|upgrade|exec` (third-party code) are N6.

**Pushing to main is a person's.** Any `git push` to main, master or trunk (`HEAD:main`, `+main`, a bare `push` while on it) is
refused in every mode (N5), the repository's default branch (`origin/HEAD`) included, `--all` and `--mirror`, a bare push through `-C` or after a
`cd` (read as main), and any git `-c` override (an alias hides the sub-command; N6), since it would bypass the merge queue and the Review queue. The queue merges locally and pushes
nothing.

**Modes.** `full` (default): the lead approves; without a lead, or without a second key on another vendor for a two-key
action, the action is denied (the row says `needs_second_engine`) and the work goes on. `on`: the same, but where
`full` would deny for want of an approver a person is asked. `off`: no one is approved by code, so every request goes to
a person as before (the thread's own permission mode does not accept it for them, though a person's saved rule still can), and
the hard rules, the injection wire and the denial counts still apply.

## Merge queue, Review queue and watch on main

**Forge final writer.** The lead's `crew_queue_merge` and `crew_process_merges` request Core-owned GitHub/GitLab publication,
CI observation and merge commits. Commands still pass through the existing forge process owner; no credential is read by Crew.
Person-owned `CoreConfig::crew_merge_required_checks` must be nonempty. CI is green only when every required check is present
and succeeded at the pinned head. Any other check may succeed, be skipped or neutral, but a failed, cancelled, timed-out,
pending or wrong-head one refuses merging, and so does a required check that is missing, skipped or neutral. A failed
combined-state check blocks the entry for a person instead of re-running on every call. The `forge_publish_requested` and
`forge_merge_requested` events record the `actor` (`lead` for the lead's tool, `local_caller` for the local HTTP caller). Head and base are pinned, combined-state checks run in scratch,
and holds, lanes, actual PR identity and CI are read again before merging. Migration 45 persists intent before each effect;
uncertain results reconcile identity/head/merge commit rather than repeating an effect. The actual merged tree/parents are checked.
Canonical origin host/repository identity is selected explicitly for every API/create/merge call and checked against receipts;
CLI repository/host defaults cannot redirect it. Startup reconciles unfinished effects and confirmed receipts before watching.
The separate local compatibility API keeps its no-checked-out-branch and compare-and-swap rules and refuses forge intents;
authentication failure never selects it. Ordinary agent main-push and forge-merge commands remain N5 denied.

**A forge merge that needs a person.** A forge intent reaches `needs_person` when the pipeline cannot go on safely (the target or PR
changed, publication is unknown, the combined-state check failed, a withdrawn escape check was found late). The lane **skips** it
and goes on with later entries. Ordering rule: merges are ordered per repository, a published or uncertain entry keeps the lane, an
entry waiting on a person does not. Skipping is safe because each entry pins its own base and is checked against the forge's base,
so no later entry depends on it; a later merge only moves the base, which the stuck entry would have to rebuild on anyway. Entries
waiting on CI, a hold or a Gate keep their place. **What is and is not guaranteed.** The pipeline never leaves `needs_person`
by itself and never sends a second merge request. An intent that reached it with no request sent (`merge_sent` false) has nothing
of it in flight. One that reached it after a merge request (`merge_sent`, saved just before the request) may still have that request
on the forge (a GitHub merge queue; `merge_in` never passes `--auto`/`--auto-merge`, but whether `glab` queues a merge while a
pipeline runs is not verified here, so it is treated as possible). Such an intent can only be rejected, never retried, and a reject
keeps it in the reconcile set (`rejected` with `merge_sent`): a merge that lands later is read from the forge and recorded, and a
PR seen closed without a merge settles it. With a publication attempt but no PR number the forge is searched first (found and
merged: recorded as merged; found open: kept; none: the reject note says the branch may have been pushed).

Ways out of `needs_person`: a PR merged by hand (read-only from the forge, audit `actor` `core`, `via` "lane reconcile", for the
lane's own crew scope only; the lead's `crew_process_merges` can therefore record a merge the forge already shows, nothing else),
and a person's `POST /api/crew/autopilot/merge-queue/{id}/resolve` with `{action, seen_revision}`: `reject` (intent and entry
`rejected`; an open PR is the person's to close) or `retry` (back to `awaiting_ci` for a PR still open with the pinned identity and
the pinned base still the forge's, or `prepared` when nothing was published; the guards, `merge_blocker` and the escape-check guard,
run again; refused after a merge request, after a publication that may have happened without a PR number, and after the base moved:
reject and queue again). The route reads the forge first: a PR already merged is recorded and the asked-for answer is not applied;
an unreadable PR refuses the answer. It is a Person route (key enforced), local only, behind the Preview flag, in `STATE_CHANGING`;
no tool, CLI command or browser path can reject or retry. The audit row `crew.merge.forge.resolve` (with `via`) is written first,
in the same transaction as the intent and the queue entry; a crash leaves the old, still resolvable state or the whole new one, and a
crash after the `merged` write is finished by recovery. The queue's own `crew_reject_merge` (used by the escape-check guard) sends
a waiting intent through the same audited reject (`via` "queue reject"), so one with a merge request stays reconcilable too; for an
intent in any other phase it only turns the entry down and writes an audit row naming that path (`merge_requested` is already in
recovery). A rejected intent with `merge_sent` is read each lane pass and at startup (kept apart from the unfinished-effects list, so
a long list of them never stops other recovery, and bounded at 128): a merge that landed is recorded; a PR closed without a merge, or
no longer the pinned one (the request named the pinned head), settles it; a PR that stays open is read again each pass until a person
closes it on the forge. The queue view lists each entry's `forge` phase and `revision`. The transition table of `save_crew_forge_merge` is unchanged:
the way out lives in `resolve_crew_forge_merge` alone.

- **Risk.** `merge::score` reads paths (CI, migrations, dependency files, and auth, keys or crypto as whole words in a path),
  size (over 20 files or 400 lines is large) and the 60-day revert rate of each area and of the engine, which counts from 10
  merges and at 20 percent. High if any applies; low only for small docs or tests; medium otherwise. Test coverage is not
  measured, and every score says so. Words in a path match plurals and camelCase (`tokens`, `authService`, `authn`). The size adds the
  last 5 merges within a day, so splitting a change does not help (60 files or 1200 lines in all is large). Every score carries the
  sample sizes it read. An area is the first two folders of a path.
- **Held for a person, whatever the mode.** A diff that touches `.orbyn/`, a file the answer key protects, CI config, or a file one of
  the repository's checks name (a bare word is a folder, a glob is matched as a pattern, a file that does not exist yet included), or a config file of a runner the
  checks use (`package.json` for npm, `Makefile` for make, `conftest.py` and `pyproject.toml` for pytest, `.cargo/config.toml` and `build.rs` for
  cargo, `go.mod`, `justfile`, the jest, vitest, eslint, tsc, ruff, clippy, rustfmt and golangci configs), waits, at the queue and again when it merges. This is
  the backstop the always-denied list cannot be: a patch applied by `git apply` or `patch` gets past N7, and a merge that
  rewrote `.orbyn/crew.json` (the allowed hosts, the checks) or the script a check calls would stay green. A check's text is read
  from a trusted commit, but the file it runs comes from the merged tree. Not caught: a runner this list does not know, a script that calls a runner from inside itself (the script is named, its runner's config is not), and a check that
  reaches a file only through something another file says (an include, an import). The watch reads its checks from a trusted commit too. If the repository's checks cannot be read, the diff is held.
- **Mode.** `off`: every merge waits for a person. `on`: a high-risk diff waits. `full`: everything merges, and a high-risk
  one goes to the top of the Review queue. A person's release counts as the review and lasts for the entry: it skips the owed lanes as well as the holds below (by design: the person is the review), and a later block or retry keeps it.
  The mode is read again when the merge happens, so entries queued before the person switched to `off` (or to `on`, for a high-risk
  one) wait.
- **Queue.** `crew_queue_merge` takes a passed task (its commit is the passed result's), `crew_process_merges` merges the
  queued entries one at a time in the order queued: a `--no-ff` merge in a scratch checkout, then the task's own command
  checks and the repository's `checks` (in `.orbyn/crew.json` at the branch's tip) run again on that combined state, and only
  then does the branch move. The lanes owed must have run, in their latest round, unless a person released it: by default `CODE_LANE` (`correctness`, the lane token the
  review tool names, one constant shared with slice 4b) for medium and high, and `SECURITY_LANE` (`security`) too for high (`MergeTarget::owed`
  overrides it). A lane that never ran blocks the entry and writes one `merge_waiting` row. A conflict or a failing check rejects the entry; a branch that moved is retried; a confirmed
  finding or a running review lane blocks it until it clears. A merge commit makes a revert one `git revert -m 1`.
- **Review queue.** Every merge is one item: the diff size, the proof (checks, reviewers by engine, findings), what was not
  verified and the cost the result recorded. Ranked: a break that needs a decision, then high, medium, low, then oldest.
  Windows: high 4 hours, medium 24, low none. Answers: looks good, comment (a normal scoped fix using the original grants, effects,
  protected checks and a scope check; a stale checkout asks a person), revert in local compatibility mode (made in a
  scratch checkout, the checks run on the result, then the branch moves, and it is refused if a later merge needs the one reverted).
- **Watch.** `crew_watch_main` runs the repository's checks, read at the last commit known good (or where the first watched
  merge began), so a merge cannot remove its own check. If they fail it bisects the first-parent commits to the first that fails.
  It reverts only when the culprit is a merge the crew made, the checks fail with it twice and pass without it, and they pass
  after the revert (so no later merge needs it). Otherwise it records the break, flags the item to the top of the Review queue
  and tells the person (a `person_told` row). Under `off` it only asks. A revert in a repository that defines no checks runs none and is recorded `verified: false`. A repository with no checks is reported `NoChecks`,
  never healthy. Forge merges trigger a remote-tip scratch watch after a confirmed receipt; a certain break prepares and verifies a corrective
  task locally and flags the merge with "Choose Revert to publish it". Only a person's Revert queues it, and the watch never
  pushes, opens, comments on or merges anything on the forge, nor moves local main. A repository/commit claim is durable across restart. Digest, overdue and break messages are
  visible chat messages with atomic dedupe receipts, without provider turns. The optional owned `run_crew_merge_watch` future
  is off by default, bounded to 60s..24h and 32 targets, and cancellation-owned; it only reads effects already requested and never schedules publication or merging.

## The always-denied list: what it reads, and what it cannot

`allowed_hosts` is read only at the task's base commit, with the existing 64 KiB file limit.
Each entry must be an ASCII DNS name (nonempty labels of at most 63 bytes, no leading or trailing
hyphens, at most 253 bytes), an IP literal, or `*.` followed by a DNS name within the same total
limit. Case and one terminal DNS dot are normalized; IPv6 literals may have one bracket pair.
Wildcards keep matching the apex and its subdomains, but never an IP literal. Malformed entries
are dropped individually; valid entries in the same file remain usable. Ports, URLs, ambiguous
numeric address spellings and malformed JSON grant nothing. Matching validates entries again,
including those supplied directly to the detector. Loopback remains allowed, and explicitly
listed private addresses, local collectors and internal names remain supported. These are syntax
checks: they do not resolve DNS, prove destination ownership or prevent rebinding, redirects or tunnels.
They apply only to hosts the text reader extracts: bare single-label, numeric shorthand and IPv6
arguments of connecting programs are not consistently extracted. The synthetic N3 controls use
explicit URL authorities; this admission change does not qualify those other command forms.

The repository validation view reports invalid entries without their submitted values, using
zero-based array indices. These notes do not block engines or change admission: a wrong JSON
type still denies the whole configured list, while an invalid string is dropped individually.
A one-label wildcard base (including `*.com` and `*.internal`) warns but still grants its base
and all descendants. There is no public-suffix check; `*.co.uk` is not distinguished from other
multi-label bases. The producer caps host notes at 16 plus an omission summary. Settings displays
generic Rule notes with a separate 16-note cap and summary, and Check again reads the selected
project's committed validation view. The actual three-engine replay and detector qualification
remain outstanding under #457; synthetic parser and UI checks do not supply that evidence.

It reads the kind, target and paths the gate is given, and for a command the simple commands inside it. It cannot see
what the gate is never asked: an engine's own file reads and its own network, which Orbyn does not hear about. It follows a `cd` (so `cd .. && rm x` is outside), `git -C`, `git worktree add`, `find -delete`, `tar -C` and `unzip -d`, and counts a
patch (`patch`, `git apply`), an interpreter reading a file, its input or a pipe, and `python -m` of a module that is not a well-known one
(pytest, pip, venv, ruff, black, mypy, ...), as generated code (two keys). It reads
no script's contents, so a script that does a forbidden thing inside itself passes unless it is a generated script (two
keys). It misses obfuscation and encoding (`base64 -d | sh` is caught as a pipe to a shell and two keys, not by name), aliases
and shell functions. A host in `allowed_hosts` is trusted whatever runs there. A package manager's own registry is not
checked against `allowed_hosts` unless the command names a host (installing is the two-key class). `git push origin
branch` is the repository's own remote and reaches the lead as network, two keys. N10 and N9 match by host and
program, and for tools and pages by word, so they can both miss and over-match. Tests pin the cases that matter and the
near misses that must pass (`grep -r stripe` is not a payment).

Not caught, listed with [#457](https://github.com/orbyn-dev/orbyn-engineering/issues/457): a symlink inside the worktree that
points out (a path is read lexically, so a write through it lands outside), a path that `xargs` or a pipe supplies, the files
a patch names, a script's own contents, and anything an engine does without asking. The scratch checkouts of the merge queue
sit inside the data folder, which a worker running as the same user can read: the sandbox follow-up is #441.

## Ledger

`crew_autopilot_events` (migration 38) is append-only: `allowed`, `denied`, `person`, `disagreement`, `tripwire`, and the rows of a held command (`held`, `approved_once`, `denied_by_person`, `dismissed_by_person`, `hold_used`, `hold_ended`) and, one per routing, `route_class` (the data classes a brief was in and the engines they blocked),
`approval_started`, `approval_usage`, `approval_ended`, `second_engine_dismissed` and `second_engine_resolved` also retain approval turn reservations, redacted usage and notice resolution. Each decision has the action, who decided and the rule or reasons. Counters are read from the rows. This is not the hash-chained
ledger of slice 4c; when that lands these rows move into it.

## Seams for later slices

- **4i, blast-radius guard and data-class routing:** `tier::tier` adds a two-key reason for a destructive command against
  a production-linked target, so it needs two keys under Full too. Path classes and the router's blocking filter go in
  `crew::filters`, reading `.orbyn/crew.json` at the base commit as `hosts` does.
- **4j, autonomy by area:** `Core::crew_autopilot_mode` still resolves the person's mode (the ceiling). The merge gate lowers it
  per area in [`areas/effective`](areas/effective.rs): the lower of the ceiling and every touched area, where a lowered area is On only
  under a Full ceiling. The chain and the tier do not read an area.

## Engine approval runs

The service enables request-scoped engine approvers only with Preview. An approval is a separate delegated turn on the lead's
engine and selected model, with its effort/options captured for the request. A busy lead is never steered. The only prompt data
is the redacted `Summary`. A second key must pass the registry, quota, data-class and lead-tier filters on another vendor.

The chain shares a 30-second deadline, shortened by the task's remaining minute budget. Each admitted approval atomically reserves
one of the brief's existing turns, counting prior approval reservations and known worker admissions. Missing or ended budgets refuse.
This does not complete global worker-turn enforcement (#438). Redacted `approval_usage` rows keep reported tokens and estimated cost,
even for rejected answers; an interrupted or malformed response with no usage receipt is explicitly unknown, never zero.

Claude Code 2.1.295+ has a dedicated CLI adapter: no builtin tools, strict empty MCP, customizations/hooks disabled, no saved/resumed
session and one turn. It sets a 1,024-output-token limit and a $0.05 estimated-spend stop. Total reported usage above 16,384 tokens,
or reported estimated cost above $0.05, rejects the answer **after** that usage was spent. These are not hard pre-spend limits on
input/reasoning tokens or billed dollars; the provider's spend stop can overshoot. Unsupported options, older/unknown versions,
missing usage and invalid responses fail closed. See the [CLI reference](https://code.claude.com/docs/en/cli-reference) and
[output-token environment contract](https://code.claude.com/docs/en/env-vars.md).

Claude model aliases require the bounded effective model reported by the exact active lead run. The approval passes that full
model ID and accepts only an exact matching receipt. Missing or ambiguous metadata refuses before an approval launch. An idle
lead or second Claude instance needs an explicitly pinned full Anthropic model ID; a catalog containing only aliases is unavailable
for that role. Provider-specific model IDs are not yet qualified. See [model aliases](https://code.claude.com/docs/en/model-config#model-aliases).

Core denies permission/question/tool/session events, bounds reply text to 4 KiB, and accepts only strict `{approve, reason}` with a
nonempty reason of at most 300 characters. Cancellation intent, task end, changed attempt or mode invalidate late answers. Dropping
the requester stops its owned driver task and closes admission. No MCP binding, inherited transcript, raw page/tool text or answer
key enters the turn. This is a tool-admission contract, not an OS sandbox claim.

Ordinary Full-mode missing-second-key denials project to a server-persistent Needs you notice. It blocks nothing and offers only
Sign in and Dismiss. The ledger pins the lead's vendor. An eligible signed-in other vendor or a local caller's explicit dismissal
writes one durable resolution row; another instance of the same vendor cannot clear it. Cursor pages enumerate all pending notices,
including old ones; neither action approves or retries work. Guarded commands retain their separate person-approval flow.

## Limits of Preview

- Codex and other native/ACP drivers have no bounded tool-free adapter yet and remain `Unsupported`; they are never substituted
  with ordinary one-shot or read-only turns. Claude backing and the notice are covered by synthetic tests in
  [#456](https://github.com/orbyn-dev/orbyn-engineering/issues/456). Real-provider and installed-platform qualification remains pending.
- Forge merging needs the person's nonempty `ORBYN_CREW_REQUIRED_CHECKS` JSON list. Only GitHub and GitLab have guarded Crew
  receipts. Idle delivery is off unless Preview and `ORBYN_CREW_WATCH_INTERVAL_SECS` (60..86400) are explicitly enabled; the
  service still makes one read-only startup reconciliation/watch pass with Preview on, and cancels/awaits its single owner on
  shutdown. A person's Revert or a certain watch culprit prepares a distinct Core-owned rollback task. Its original scope,
  protected files, trusted repository checks and exact rollback tree are verified without a provider turn. The ordinary forge
  queue requires fresh reviewer lane Gates, configured current-head CI and all mode/area holds before publication and merging.
  Same-vendor Security requires its persisted once-consent or Full exception receipt, a separate lead-tier run and High risk,
  or a person's merge release; locked lanes remain owed. Coverage and live-forge qualification remain unmeasured
  ([#460](https://github.com/orbyn-dev/orbyn-engineering/issues/460)).
- The detector and the list are heuristics (above). Host-admission regressions use synthetic inputs,
  including malformed configuration and legitimate local/private destinations; they are not captured
  engine runs. The three-engine real-run replay campaign and its observed misses remain outstanding
  ([#457](https://github.com/orbyn-dev/orbyn-engineering/issues/457)); no real-run findings are claimed here.


### Corrective forge tasks

A rollback has its own immutable schema-56 binding to the original merge, remote, branch, current target tip, candidate and
registered sibling checkout. The original merge stays in the Review queue until the corrective merge receipt is confirmed. Parent projection and the idempotent area effect are resumed after a crash; completion and the revert ledger row commit together only after both succeeded.
Revert replies report that persisted state and carry no revert commit while the PR waits. Repeating an answer or restarting
reuses the same task and publication intent. Unknown creates and merges use the ordinary receipt reconciler without another send.

The retained candidate checkout is the source of normal review, finding checks and lane Gates. It is validated against its
binding, registered Git common directory, exact HEAD and clean index before admission. A missing or altered checkout, target
drift, failed check, confirmed finding or ended corrective task asks a person. A deterministic rollback with a finding does
not launch a model to rewrite the candidate. The lead's checkout is untouched. The watcher proves a reproducible Crew culprit
and excludes unknown causes; Off asks a person. Startup resumes code-owned verification and processes the ordinary lane.

Each original merge has at most one correction, and startup refuses more than 128 pending bindings. Candidate checkouts remain
held while their task, PR or review threads need them, including a failed or cancelled task requiring inspection. They are normal
registered worktrees visible to the app's retention/cleanup policy, never silently deleted by this workflow. Retire them only
after the task, review threads and forge intent are terminal and their proof no longer needs the checkout. Completed bindings
and commit receipts remain durable. A confirmed revert projects the original state and area breaker once.
