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
| `tripwire` | 3 denials in a row, 20 in a run, and the injection detector |
| `ledger` | Writes the approval ledger rows: redacted, the key's text withheld, scanned |
| `review` | `Core`'s side: the mode, the approvers, and `ActionReview` for the gate |
| `merge/risk` | The risk tier of a diff, with its reasons. Pure |
| `merge/holds`, `merge/guard` | What holds a merge: the person-only diffs, the owed lanes, the mode now |
| `merge/queue` | The merge queue: score, mode, one merge at a time, checks again on the combined state |
| `merge/review_queue` | The after-merge Review queue and the person's answers |
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
4. **Tier:** reads and writes inside the leased paths and the brief's own check commands need nobody. Network, installing
   dependencies, running a script or code the worker wrote, and CI files need two keys. The rest is the lead's.
5. **Chain:** the lead answers a [`Summary`](chain.rs) (kind, target, paths, the brief's outcome, the tier and why), never
   raw page or tool text, and must give a reason (an approval without one is a denial). Two keys: a second approver on
   another vendor (`family`) must also say yes. A disagreement is denied and logged as its own row. A person's deny rule
   still wins over an approval, and an allow rule or no rule lets an approval through.
6. **Trip wires** after each denial: 3 in a row or 20 in a run stop the task, `too_many_denials`. "In a run" is counted per task
   (a task has one run). Only an approval by the lead or by two keys ends a row of denials: a read that needs nobody does not,
   so a worker cannot put `ls` between its denials.

Outside a Crew task only N1 (the account) is enforced, so an ordinary thread cannot call a state-changing account route. N1 looks
at requests, not at files: a network, browser or tool action that names a route, or a command run by a connecting program
(curl, wget, an interpreter) that does. A file change, a search or a read of a document that merely names `/api/account/...`
is not refused, so ordinary work does not trip the wires.

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

**Local only.** A merge moves a *local* branch in a repository where no worktree has that branch checked out, by a
compare-and-swap `update-ref`. A branch that is checked out is never moved: the entry waits for a person. Nothing is pushed,
no pull request is opened or merged, no forge CI is read, and nothing reaches a remote. Every merge, check and revert runs in a
scratch checkout under the data folder (`crew-merge/`), never in a person's checkout.

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
  Windows: high 4 hours, medium 24, low none. Answers: looks good, comment (kept; the fix brief is not built), revert (made in a
  scratch checkout, the checks run on the result, then the branch moves, and it is refused if a later merge needs the one reverted).
- **Watch.** `crew_watch_main` runs the repository's checks, read at the last commit known good (or where the first watched
  merge began), so a merge cannot remove its own check. If they fail it bisects the first-parent commits to the first that fails.
  It reverts only when the culprit is a merge the crew made, the checks fail with it twice and pass without it, and they pass
  after the revert (so no later merge needs it). Otherwise it records the break, flags the item to the top of the Review queue
  and tells the person (a `person_told` row). Under `off` it only asks. A revert in a repository that defines no checks runs none and is recorded `verified: false`. A repository with no checks is reported `NoChecks`,
  never healthy. Nothing schedules the watch yet: a caller runs it after merges.

## The always-denied list: what it reads, and what it cannot

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
- The queue and the watch are called by whoever drives them: no tool exposes them to the lead, and nothing runs them on a
  schedule or after a merge yet. The merge Gate (stage `merge`) is not written, since it needs Evidence objects.
  See [#460](https://github.com/orbyn-dev/orbyn-engineering/issues/460).
- The detector and the list are heuristics (above); real runs will show what they miss ([#457](https://github.com/orbyn-dev/orbyn-engineering/issues/457)).
