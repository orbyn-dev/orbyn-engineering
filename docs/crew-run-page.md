# Crew Run page (W11): what is built and what the reads still miss

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

## The read (built)

`GET /api/crew/flow/tasks/{id}/run` (`Core::crew_flow_run`) answers one work task's page. A Local read behind Crew Preview
like every Crew route, listed in `orbyn_core::crew::changes::READS`: no store write, no process, no file. Every other text
passes the core's own redaction first: the crew's answer keys are withheld, then the secret scan of every built-in format,
then the person's patterns; a text that still holds the key is replaced, never shown. The web cleans it again.

A check's command and its `proves` line are the answer key's own words, so they are the one place Crew does not withhold the
key's text: the page exists to name the command that ran, and this route is the person's, never a worker's. The key's id and
its expectations are still never in it. A not-verified line that is a skipped check's command is withheld for that same
reason: it is the key's own text, so it becomes "Withheld.".

A check's command and what it proves are the one place this read parts from the Flow task read, which withholds all check
text: the Run page is the person's own record of their own run and the checks are what it is for. The answer key's id and
its expectations are never in it.

| Field | Shape | Cap | State |
| --- | --- | --- | --- |
| Prompt | `{ text, hidden, truncated }`: the brief's outcome as redacted, `hidden` when a value was hidden | 2000 characters | Built |
| Checks | list of `{ label, outcome, exit_code, duration_ms, proves, commit }`, `label` the command as written in the check, not a full environment | 50 checks, 200 characters per text | Built |
| Not verified | `CrewResult.not_verified` | 20 lines, 200 characters | Built || Commit | the commit the checks ran on | one hash | Built |
| Pull request | `{ number, branch, commit }`: the forge's number from the task's forge intent, and the branch and commit the run published | one per task | Built |
| Plan | list of `{ step, engine }` | 30 steps, 200 characters each | Missing: no table, object or read stores a task's plan steps |
| Pull request URL and head | the forge's own `url`, and the pull request's current `head` for the Stale frame | one URL, two hashes | Missing: a Crew pull request's publish records neither, so the page cannot say the proof is stale |

Rules: reads only, behind Crew Preview like every Crew route; the core redacts, the web never trusts it (it cleans again).

## Not built yet

- Plan (`runpage-plan-long`) and the per-check detail frame (`runpage-findings`) need the plan rows above; nothing stores
  them, so the page does not invent them.
- Stale proof (`runpage-stale`) needs the pull request's head, and the Run tab needs the forge's own URL; the publish would
  have to record the link first.
- The section chooser before saving (`runpage-publish-menu`), the "Run page saved on this device" line, the 400 ms loading
  shape (`runpage-loading`) and failing-check-first ordering are web work on the read above.
- The Run tab on a pull request is still hidden: `RunLinkClient` has no client that can say which task made a pull request,
  which needs a read by project and number.

