# Crew Run page (W11): what is built and the reads it still needs

Frames: `runpage-*` in `orbyn-designs/artifacts/engineer-designs.html`. Beta ruling: the page is generated on the person's
device; nothing is uploaded; the signed proof and its verify step stay Planned.

## Built (web only)

| Piece | Where | Behaviour |
| --- | --- | --- |
| Static page and PR comment | `web/src/features/crew/runpage/runExport.ts` | Pure functions from one `FlowTaskDetail`. HTML: one inline style, a `default-src 'none'` policy, no script, no link, no remote resource, every value escaped. Markdown: every punctuation mark, mention, issue reference, link and HTML escaped, at most 4000 characters, findings cut with an "and N more" line. |
| Redaction | `cleanText` in the same file | Control and bidi characters, key-shaped words, `key=value` pairs and any path on the person's machine are hidden before escaping; each text is capped. |
| Run view | `runpage/RunPage.tsx` | Opened from the Flow task zoom ("Run page"). Save as file is a Blob and an anchor (object URL revoked after 1 s) with the "Download started" toast. Copy PR comment writes the Markdown once and shows a clipboard failure. One click, one action; no request is made. |
| Run tab on a pull request | `pulls/detail/PullDetail.tsx` | Drawn only when `runLinkClient().forPull()` returns a task. The production client returns none, so the tab is absent. No match by branch or title. |

Planned and shown as such: signed proof, Verify (frame `runpage-verify`), opening the page from another device (frame
`runpage-other-device`).

## The read still missing

The Flow task read has no pull request, no check list, no plan and no prompt. Add one Local read (no writes), or extend
`FlowTaskDetail`; the web already waits for it behind `RunLinkClient`.

Owner: Eng 2 (crew core), with Eng 4 for the pull request link.

| Field | Shape | Cap | Redaction |
| --- | --- | --- | --- |
| Pull request link | `{ number, url }` or `{ branch, commit }` of the task's result, only for a task Orbyn opened a pull request for | one per task | `url` must be the forge's own pull request URL; no credential, no query string |
| Checks | list of `{ label, exit_code, duration_ms, proves }`, `label` is the command as written in the check, not a full environment | 50 checks, 200 characters per text | the core's secret scan, all built-in formats; no environment values, no output excerpt, no machine path (repo-relative only) |
| Plan | list of `{ step, engine }` | 30 steps, 200 characters each | same scan |
| Prompt | the person's brief as redacted by the core, with a flag when a value was hidden | 2000 characters | same scan; the answer key is never included |
| Not verified | `CrewResult.not_verified` | 20 lines, 200 characters | same scan |
| Pull request head commit | for the "Stale" frame (`runpage-stale`): the run's commit and the pull request's current head | two hashes | none |

Rules: reads only, behind Crew Preview like every Crew route; the core redacts, the web never trusts it (it cleans again).

## Not built yet

- Section chooser before saving (`runpage-publish-menu`), "Run page saved on this device" line, Stale proof, per-check detail,
  plan, prompt, failing-check-first ordering: all wait for the read above.
- Reading time under 400 ms shows no loading shape; the page uses the app's standard loading state.
