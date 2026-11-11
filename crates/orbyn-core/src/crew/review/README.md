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

- **Lanes.** The lead names lanes (any lowercase word); repo rules add more. Each lane is a task with a `review` brief, routed by role
  `review` and its lane at the lead tier, to another vendor than the builder where one is configured. It has no write lease and posts
  only to the lead. It is told the work's purpose, the commit and how many checks passed, never a check, its command or the builder's
  transcript. Its key has one check: the review changed no file that was unchanged when it began.
- **Findings** are the protocol's object. A finding has no state: it blocks nothing until a Gate at `finding_check` confirms it.
  `crew_finding` is a member tool, so it refuses once the reviewer's run has ended (H7); the Gate is a core write and does not.
- **The check.** With a `repro` (a check the lead states as the *correct* behaviour), the core runs it: failing confirms the finding,
  passing leaves it unconfirmed, a repro that cannot run decides nothing. Without one, an independent second review of the lane runs, on
  the other vendor from the first reviewer where one is configured, never shown the finding. It confirms only if it reports the same file within
  3 lines. Both write core Evidence and a Gate decided by `code`, stored with the reviewed task. A model's review is never a pass or fail gate.
- **The fix loop.** A confirmed finding sends a fix brief to a builder, written by the core and started through the same function as
  `crew_start` (shape, secrets, protected paths, the key's text). It may write the finding's file only. Its key is the reviewed task's checks
  plus the repro, and `.orbyn/crew.json` is protected. When the fix passes, every lane that has reviewed the work runs its next round on the fix's commit.
  A finding is then fixed (gone from the lane's latest finished round), still open, or regressed (back after being gone).
- **The loop guard.** The fix task has slice 3's guard. A finding gets one fix brief: the same finding (lane, file and claim) after a fix is still open, is
  not sent again, and the lead is told that a person decides.

## Limits of Preview

- A fingerprint is the lane, the file and the claim with case and spacing ignored. A reworded claim is a new finding.
- The key's one check cannot see an edit to a file that was already changed when the review began. Reviewers share the lead's checkout (the lease rule and
  that check are what keep them read-only; no sandbox does).
- A rule's `when.paths` are file or `dir/**` patterns, like a lease; `required` and `checks` in the rule file are not read yet. Removing a locked lane
  is a person's act and nothing builds it yet.
- A fix whose finding is outside the reviewed task's own `file_scope` check fails that check; the guard then stops it.
- Not built: the same-engine badge and its question, PR review on the forge, a `review` Gate per lane, Autopilot and the merge queue.

## Tested

`tests/review.rs` runs fake engines: an unconfirmed finding blocks nothing, a confirmed one sends a fix brief and the re-review shows it fixed, the
same finding twice stops the loop, a lane added after a pass runs, repo rules add lanes, a second review confirms by agreement or leaves a finding unconfirmed, and a
reviewer reads and reports. `tests/review_rules.rs` covers the pure rules.
