# Review stages

Slice 4b of [docs/crew.md](../../../../../docs/crew.md#review-stages), behind `CoreConfig::crew_preview`. A review lane is a run of
its own. Code, not a model, decides whether a finding is true.

| Module | Owns |
| --- | --- |
| `lanes` | `crew_review`: one read-only run per lane, no cap, also after the work has passed. A lane named again is its next round. The reviewer's prompt and its key |
| `findings` | `crew_finding`: a reviewer files a typed finding; the core stamps author, lane, commit, run and id |
| `decide` | `crew_check_finding`: the finding check, the Evidence it cites and the `finding_check` Gate |
| `fix` | The fix brief for a confirmed finding, and the loop stop |
| `ended` | What follows a task that passed: a second run decides, a fix re-runs the lanes, a lane tells the lead |
| `status` | `crew_review_status`: lanes, rounds, each finding's check and standing. Derived, never stored |
| `rules` | The `lanes` of `.orbyn/crew.json`: a repo rule can only add a lane |

## How it works

- **Lanes.** The lead names lanes (any lowercase word); repo rules add more, read from `.orbyn/crew.json` **at the commit the work
  started from**, so a worker cannot edit a lane away; a broken file is an error. Each lane is a task with a `review` brief, routed by role
  `review` and its lane at the lead tier, to another vendor than the builder where one is configured. It has no write lease and posts
  only to the lead. It is told the work's purpose, the commit and how many checks passed (the task's own, not a finding check's), never a check,
  its command or the builder's transcript.
- **Read-only is checked by bytes.** Every file already changed when the review begins (the work under review, in the usual case)
  is a protected path of the reviewer's key, so the core's snapshot compares it, and a scope check fails any other changed file
  (a `diff_size` of 0 when nothing was changed). More than 256 changed files is refused: commit the work first. A run that fails the check
  stops, and its findings cannot be confirmed. A lane runs at most 5 rounds on one piece of work.
- **Git cannot be told to look away.** `git update-index --assume-unchanged` or `--skip-worktree` hides an edit from every git-based check. The core
  records the flagged entries (`git ls-files -v`: lowercase tag or `S`) when any task starts and fails the verification if one was added. The cost is one
  `git ls-files` at the start and one per verification, with no file reads; the alternative, a byte snapshot of every tracked file, would cost a read of the whole
  repository per task. Flags that were there at the start (a sparse checkout's) are not a failure.
- **Findings** are the protocol's object. A finding has no state: it blocks nothing until a Gate at `finding_check` confirms it.
  `crew_finding` is a member tool, so it refuses once the reviewer's run has ended (H7); the Gate is a core write and does not.
- **The check.** With a `repro` (a check the lead states as the *correct* behaviour, run for at most 2 minutes), the core runs it: failing confirms
  the finding, passing leaves it unconfirmed, a repro that cannot run decides nothing. Without one, an independent second review of the lane
  runs on **another vendor than the first reviewer**: if none is configured it is refused, with the reason stored, never run on the same vendor.
  It is never shown the finding and confirms only if it reports the same file within 3 lines, and not if it filed more than 3 findings in that file
  (it files at most 10). Both write core Evidence and a Gate decided by `code` (naming the engines on each side), stored with the reviewed task and kept
  out of the task's own verification Evidence. One Gate per finding, written in one transaction. A model's review is never a pass or fail gate.
- **The fix loop.** A confirmed finding sends a fix brief to a builder, written by the core and started through the same function as
  `crew_start`. It is refused for a file the reviewed task could not write, a test or CI file, a protected path or a file that is not there.
  Its key is the reviewed task's checks, the repro, a check that the file now differs from the copy the core kept (a cheap guard that it was touched), and a scope
  check (the files changed when the fix began, and the finding's file). The files already changed (the work under review), bar the finding's file, are held by
  their bytes, as a reviewer's are. A fix that changes nothing fails and stops. The lanes run again when a fix passes, after the last fix still running.
- **When a finding stops blocking.** A place (lane and file) with a confirmed finding blocks until a fix that names it has passed **and** a repro has proved it:
  the lead's repro, which was in the fix's key, or one the lead supplies after the fix (`crew_check_finding` with a `repro` on a finding the core confirmed by
  agreement), which must pass now and is then recorded as Evidence and a `checks` Gate on the finding. A finding confirmed by agreement has no repro, so a fix
  alone never clears it: a cosmetic edit, a changed copy or a silent re-review change nothing. It blocks again if a later finished round reports the place.
  Nothing else clears it yet: that is a person's act (see #450). A reviewer that goes quiet, or rewords its claim, clears nothing (`unreported`).
- **The loop guard.** The fix task has slice 3's guard. A lane and file gets one fix brief, so a reworded claim is the same place, and one reviewed
  task gets at most 3 fix briefs. Past either the loop stops, the lead is told, and a person decides.

## Limits of Preview

- A fingerprint (lane, file, claim) only labels a finding across rounds; the guard and the blocking keep to the lane and file.
- A path hidden by `.gitignore` or `.git/info/exclude` hides a new file from the git-based checks, and a write outside the checkout is seen by none of them (#451).
- A fix key cannot hold a manifest that defines check commands (`package.json`, `Makefile`, `justfile`, ...): such a file is refused for a fix.
- A repro runs in the checkout as it is now, not at the finding's commit (its Evidence records the commit it ran on).
- Reviewers share the lead's checkout. The lease rule, the protected-file comparison and the scope check keep them read-only; no sandbox does (#436).
- A rule's `when.paths` are file or `dir/**` patterns, like a lease; `required` and `checks` in the rule file are not read yet. Removing a locked lane
  is a person's act and nothing builds it yet.
- Not built: the same-engine badge and its question, PR review on the forge, a `review` Gate per lane, Autopilot and the merge queue.

## Tested

`tests/review.rs` runs fake engines: an unconfirmed finding blocks nothing, a confirmed one sends a fix brief and the re-review shows it fixed, the
same finding twice stops the loop, a lane added after a pass runs, repo rules add lanes, a second review confirms by agreement or leaves a finding unconfirmed, and a
reviewer reads and reports. `tests/review_rules.rs` covers the pure rules.
