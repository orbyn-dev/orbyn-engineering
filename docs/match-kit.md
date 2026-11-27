# Match kit

A pull request that changes `web/` names the screens it built, and a bot posts their screenshots. For development only: it draws **our** app on its
dev-only mock data (fake data, no account, no network), never the design frames. The designs are private and this repository is public, so
nothing here reads or publishes them. Comparing with a design stays a local task for whoever has the designs.

## Name your frames

Put one line in the pull request description (the template has it):

```
Frames built: pr-list-failed, pr-detail-running @ v56
```

The ids are screens from the list below; `@ v56` is the version of the designs they were built to. A pull request that draws nothing says
`Frames built: none` and gets no comment. A missing or empty line gets a comment asking for it. Nothing is blocked either way.

An exact artifact source revision is also accepted: `Frames built: flow-offline @ git:a582a9ba4fb950d4024ac50541a63e9c01f7669a`.
Use the actual full Git object id; no numbered published version is implied and no private source URL is needed.

## What it does

`.github/workflows/ui-preview.yml` runs for a same-repository pull request that changes `web/` (a fork's is skipped: no `pull_request_target`, no
secrets, every action pinned by commit). It starts the dev server (`vite`, which serves `web/mock`) on a free port, opens each named screen with
Playwright at 1400x760 in Light, Dark and Dim, publishes the images to the `pr-previews` branch (one folder per pull request, linked with
`?raw=true`, removed when it closes by `ui-preview-cleanup.yml`) and posts one sticky comment: Light first, Dark and Dim folded, at most 12
frames, with a link to the run, which keeps every image for 14 days. A render that fails says so in the comment. It is informational, not a
required check.

Locally, from `web/` after `npm ci` (Playwright needs a browser: `npx playwright install chromium`, or point `PLAYWRIGHT_BROWSERS_PATH` at one):

```bash
printf 'Frames built: pr-list-failed, shell-home @ v56\n' > /tmp/body.md
node tools/pr-preview/render.mjs --body /tmp/body.md --out /tmp/preview
```

Images are written to `/tmp/preview/images/<frame>/<theme>.png` and the comment to `/tmp/preview/comment.md`. Never commit images.

## The screens

`web/tools/pr-preview/catalog.mjs` is the list: each id says which route to open, which mock scenario to use (`web/mock/api.ts`), what to hold
or fail on the network, and what to click first. Use the design's frame id for a screen that has one.

**The shell.** `shell-home`, `shell-new-thread`, `shell-ledger`, `shell-thread-row-actions`, `shell-project-controls`, `update-sources`

**The engine picker.** `engine-picker-closed`, `engine-picker-new-thread`, `engine-picker-crew-tiers`, `engine-picker-crew-tiers-narrow`

**Unmanaged threads.** `unmanaged-thread-open`, `unmanaged-thread-open-narrow`, `unmanaged-thread-first-run`, `unmanaged-thread-first-run-narrow` (the mock's Kiro, a `managed: false` non-Cursor engine; the notice and labels only, because the non-Cursor mode refusal is planned until the service sends it)

**Pull requests, the list and its states.** `pr-row-variants`, `pr-row-pending`, `pr-row-pending-failed`, `pr-list-none`, `pr-list-loading`, `pr-list-error`, `pr-list-signedout`, `pr-list-offline`, `pr-list-draft`, `pr-list-filtered`, `pr-list-narrow-900`, `pr-list-narrow-800`, `pr-list-narrow-700`, `pr-list-narrow-640`, `pr-list-narrow-580`, `pr-tb-filter-waiting`, `pr-tb-filter-failed`, `pr-tb-filter-rate-limited`, `pr-tb-filter-still-limited`, `pr-row-tooltips`, `pr-row-actions-hover`

**The command palette.** `palette-prs-unreadable` (Engineer designs v195): the Pull requests read in the palette fails, so the note under the group's heading draws the alert triangle, the words and Retry.

**Pull requests, the toolbar.** `pr-tb-default`, `pr-tb-search`, `pr-tb-filters`, `pr-tb-chips`, `pr-tb-sort`, `pr-tb-project`, `pr-tb-refreshing`, `pr-tb-loading`, `pr-tb-empty`, `pr-tb-author`, `pr-tb-author-none`, `pr-tb-narrow`, `pr-tb-phone`

**Pull requests, one page and its states.** `pr-detail-running`, `pr-detail-failed`, `pr-detail-review`, `pr-detail-changes`, `pr-detail-approved`, `pr-detail-conflict`, `pr-detail-merged`, `pr-detail-closed`, `pr-detail-draft`, `pr-detail-conversation`, `pr-detail-files`, `pr-detail-checks`, `pr-detail-merge-menu`, `pr-detail-loading`, `pr-detail-error`, `pr-detail-error-uncached`, `pr-detail-menu`, `pr-detail-merge-dialog`, `pr-detail-merge-dialog-credits-readonly`, `pr-detail-merge-dialog-credits-readonly-on`, `pr-detail-merge-dialog-auto`, `pr-detail-merge-dialog-rebase`, `pr-detail-merge-dialog-busy`, `pr-detail-merge-dialog-error`, `pr-list-merge-dialog`, `pr-detail-signedout`, `pr-detail-offline`, `pr-detail-filtered`, `pr-review-open`, `pr-review-filled`, `pr-review-busy`, `pr-review-done`, `pr-review-error`, `pr-review-own`

**Threads.** `thread-approval`, `thread-done`, `thread-failed`, `thread-interrupted`, `thread-empty`, `thread-tools`, `thread-tools-live`, `summary-work-group`, `summary-outcomes`, `thread-details`, `details-popover`, `thread-run-no-reply`, `thread-run-session-lost`, `thread-background-stopped`, `thread-background-stopped-many`, `thread-background-stopped-narrow`

**Transcripts.** `transcript-export-menu`, `transcript-export-menu-sidebar`, `transcript-export-exporting`, `transcript-export-saved`, `transcript-export-failed`, `transcript-import-project-menu`, `transcript-import-drop`, `transcript-import-reading`, `transcript-import-preview`, `transcript-import-preview-dropped`, `transcript-import-importing`, `transcript-import-engine-menu`, `transcript-import-preview-no-engine`, `transcript-import-error-too-large`, `transcript-import-error-too-new`, `transcript-import-error-unreadable`, `transcript-import-error-key-missing`, `transcript-imported-thread`

**The side panel.** `panel-empty`, `panel-launcher-menu`, `panel-tab-menu`, `panel-tabs-single`, `panel-browser`, `panel-activity`, `panel-side`

**Changes, in the side panel.** `changes-default`, `changes-empty`, `changes-error`, `changes-loading`, `changes-view-menu`, `changes-compare-menu`, `changes-find`, `changes-file-menu`, `changes-more-files`, `changes-turn-picker`, `changes-commit-picker`, `comment-select`, `comment-pending`

**The file editor.** `editor-default`, `editor-unsaved`, `editor-find`, `editor-goto`, `editor-tab-menu`, `editor-close-confirm`, `editor-loading`

**Settings, one page for each section.** `s-general`, `s-appearance`, `s-chat`, `s-diffs`, `s-keybindings`, `s-providers`, `s-threads`, `s-tasks`, `s-projects`, `s-source-control`, `s-worktrees`, `s-maintenance`, `s-labs`, `s-about`, `s-archive`

**Keybindings, Reset to default** (Engineer designs v282), on `#/settings/keybindings` over the mock's `keybindings-reset-moved` and `keybindings-reset-clash` scenarios: the More menu on a changed rule with its built-in key as the item's shortcut, the same menu on a default rule with Reset to default off and the “Already the default” tip, the row back on its built-in key with the reset toast and Undo, the rule custom again after Undo, a reset onto a key another rule uses with the amber banner and both rows' amber caps, and the menu in a narrow window. `s-x-keys-reset-menu`, `s-x-keys-reset-menu-default`, `s-x-keys-reset-done`, `s-x-keys-reset-undone`, `s-x-keys-reset-clash`, `s-x-keys-reset-narrow`

**Snap Shot, settings and the draft flow** (K49, docs/snapshots.md), drawn on the dev-only mock at `#/preview/snapshot?frame=<id>`. Settings: `snapshot-settings-off`, `snapshot-settings`, `snapshot-settings-shortcut`, `snapshot-settings-conflict`, `snapshot-settings-excluded`, `snapshot-settings-accessibility`, `snapshot-settings-windows`, `snapshot-settings-linux`, `snapshot-settings-off-narrow`, `snapshot-settings-narrow`, `snapshot-settings-excluded-narrow`. The composer and the draft: `snapshot-composer-button`, `snapshot-composer-button-narrow`, `snapshot-draft`, `snapshot-details`, `snapshot-capturing`, `snapshot-captured`, `snapshot-no-thread`, `snapshot-excluded`, `snapshot-excluded-narrow`, `snapshot-failed`, `snapshot-failed-no-window`, `snapshot-failed-too-large`, `snapshot-failed-timeout`, `snapshot-failed-error`, `snapshot-failed-narrow`, `snapshot-narrow`. Setup: `snapshot-setup-mac`, `snapshot-setup-asked`, `snapshot-setup-denied`, `snapshot-setup-accessibility`, `snapshot-setup-granted`, `snapshot-setup-windows`, `snapshot-setup-narrow`.

**Usage, the Limits tab.** `usage-claude-windows`, `usage-claude-windows-unavailable`, `usage-claude-windows-loading`, `usage-claude-windows-narrow`, `usage-claude-windows-near`, `usage-claude-windows-reached`, `usage-claude-windows-signedout`, `usage-providers`, `usage-providers-error`

**Crew, Autopilot by area** (Crew Designs v39). `autonomy-table`, `autonomy-tripped`, `autonomy-demoted-ceiling-lowered`, `autonomy-recovering`, `autonomy-criteria-met`, `autonomy-restored`, `autonomy-lowdata`, `autonomy-empty`, `autonomy-repos`, `autonomy-entry`, `autonomy-override`, `autonomy-override-warning`, `autonomy-override-off-warning`, `autonomy-thresholds`, `autonomy-narrow`

**Crew, the guard, path classes and routing** (Crew Designs v39). `guard-held`, `guard-denied-no-second`, `guard-denied`, `guard-approved`, `guard-approved-person`, `guard-approved-person-unknown`, `guard-settings`, `classes-list`, `classes-error`, `classes-test`, `classes-edit`, `classes-narrow`, `classes-pinned-ref-blocked`, `route-log-none`, `route-log`

**Crew, the background create form and its daily budget**, on the mock's `crew-background` scenario. `background-new`, `background-new-label`, `background-new-schedule`, `background-create-daily-max`, `background-create-daily-min`, `background-create-daily-invalid`, `background-create-daily-range-per-run`, `background-create-daily-range-empty`, `background-create-daily-range-hours`, `background-create-daily-range-name-label`, `background-create-daily-range-narrow`

**Crew, KC2 a task whose worker could not be stopped** (Crew Designs v125, `kc2-close-lead-could-not-stop` and `kc2-close-lead-could-not-stop-board`), on the live inbox and Board over the mock's `crew-could-not-stop` scenario. Both the Inbox row and the Board card draw. `kc2-close-lead-could-not-stop`, `kc2-close-lead-could-not-stop-board`

**Crew, KC3 the Taskboard's words** (Crew Designs v147, `needs-one-source-working-board` and `needs-one-source-board`), on the live Board over the mock's `crew-board-working` and `crew-board-needs-kinds` scenarios: the Working line on each card whose worker owns live work, the empty Needs you column's sentence, and the amber sentence each needs-you kind draws. The dashboard, Map and Inbox frames are separate PRs; "A command needs your approval" and "The review waits for you" have no kind in the field yet, so the frame draws more than the build. `needs-one-source-working-board`, `needs-one-source-board`

**Crew, KC3 the dashboard's Right now row** (Crew Designs v149, `needs-one-source-dashboard`), on the dashboard over the mock's live sample: the Needs you and Working tiles with each kind's own row and the line under each. The frame draws exactly the built row: no Idle tile and no "A command to approve" or "A review that waits" row, because the service has no such kind (Crew Designs pass 185). `needs-one-source-dashboard`

**Crew, KC3 the Map's Working word** (Crew Designs v147, `needs-one-source-working-not-idle`), on the live Map over the mock's `crew-map-working` scenario: "Working" replaces Running and Verifying on a worker that owns live work, the muted why line sits under its task, and the summary above the engines counts workers, those working and those that need you; no idle count, because the service sends none. `needs-one-source-working-not-idle`

**Crew, KC8 the resumed task's inbox row** (Crew Designs v145, `kc8-resumed-inbox`), on the live inbox over the mock's `crew-worker-stopped-twice` scenario: the Stops row for a task whose worker stopped twice after its one resume, with Dismiss alone and the note under the list; the card, the zoom and the ledger frames are separate. `kc8-resumed-inbox`

**Crew, KC4 a background start the disk guard refused** (Crew Designs, `kc4-low-disk-background-run`), on the live Background tab over the mock's `crew-background-disk-low` scenario: the newest row of Recent runs reads "Not started · Disk", and the crew's last-run cell says the same. `kc4-low-disk-background-run`

**Crew, the KC6 watches** (Crew Designs v129, the `watch-*` frames), on the real Background routes over the mock's `crew-background-watch*` scenarios. The list that mixes watches with the other crews, the create dialog's Watch fields, its two actions and its range and service refusals, and each watch's own page in its state; the change table alone draws in the Preview chunk, and the two needs-you rows draw in the live inbox. `watch-list`, `watch-setup`, `watch-setup-template`, `watch-setup-error`, `watch-setup-refused`, `watch-setup-narrow`, `watch-setup-saved`, `watch-passing`, `watch-failing`, `watch-still-failing`, `watch-recovered`, `watch-paused`, `watch-command-error`, `watch-failing-template`, `watch-history`, `watch-needsyou`

**Crew, KC6 a run's own page and a watch's refused starts** (Crew Designs v142+, frames `background-run-detail` and `watch-could-not-start-reasons`, read at git:21f05ae54fa2682b0d9795fb3588c63dd59420ed): on the real Background routes over the mock's `crew-background-run-detail` and `crew-background-watch-refused` scenarios. "Open the run" on the watch's banner opens the run's own page from the one run read: Crew, Template, Ran at, Ended, State, Spent, Review waiting and Crew id, and no log, tail, cost or action; "Recent runs" goes back to the list, and a run the read does not have draws nothing so the list stays. A refused template start's change row reads `Could not start <template>: <sentence>` in core's own words, and a row that kept no refusal keeps "Raised in Needs you.". `background-run-detail`, `watch-could-not-start-reasons`, `watch-failing-template`

**Account and sync.** `settings-account-gate-off`, `acct-settings-signed-out`, `acct-signin`, `acct-signin-wrong-password`, `acct-signin-network`, `acct-signup-invite`, `acct-signup-details`, `acct-signup-password`, `acct-signup-password-weak`, `acct-signup-password-mismatch`, `acct-signup-password-network`, `acct-signup-check-failed`, `acct-signup-email-code`, `acct-signup-code-wrong`, `acct-signup-code-network`, `acct-signup-done`, `acct-2fa-code`, `acct-settings-signed-in`, `acct-settings-paused`, `acct-settings-loading`, `acct-settings-error`, `acct-settings-offline`, `acct-signup-check`, `acct-2fa-closed`, `acct-reauth`, `acct-reauth-network`, `sync-line`, `acct-device-waiting`, `acct-device-approved`, `acct-device-denied`, `acct-device-timeout`, `acct-device-enter-digits`, `acct-device-codes-mismatch`, `acct-device-digits-locked`, `acct-device-approve-digits`, `acct-device-check-failed`, `acct-settings-not-connected`, `sync-popover-not-connected`, `acct-connect-workspace`, `acct-connect-progress`, `acct-connect-done`, `signout-other-device-removed`, `devices-waiting-approval`, `acct-device-approve-prompt`, `devices-list`, `devices-remove-confirm`, `signout-confirm`, `signout-everywhere`, `sync-popover`, `sync-popover-offline`, `sync-popover-failed`

The existing ordinary Full-mode missing-second-engine drawer is `approval-one-vendor` (live mock scenario `crew-one-vendor`).
It offers Sign in and Dismiss and grants no action.

## Add a screen

Live Flow review consent and badges: `flow-offline`, `lane-same-engine`, `lane-same-engine-full`. The Flow task zoom's artifacts view (Crew Designs, frame `flow-artifacts`), from `GET /api/crew/flow/tasks/{id}/artifacts` on the mock's `crew` scenario, opened from the zoom's Artifacts button: the checks each row names, the artifacts they named and the diff pinned to its base commit. `flow-artifacts`.
The wide Map tree and the swim lanes, built from the map and message reads: `crew-map`, `lanes-main`.
Approved review controls: `lane-remove-reason`, `settings-acp-tier-editor`. Crew, Settings › Worker provider (Crew Designs v75): `settings-crew-worker-provider`, `settings-crew-worker-provider-any`, `settings-crew-worker-provider-narrow`. Crew, Settings › Crew's Daily digest row (frame `settings-crew`, designs at git:f6c061bb5da71b7616e9cfc1e197b390a4fab448): `settings-crew`.
These retain the beta stage list; the planned graph and PR cards are not drawn.

Crew, the escape log and its check frames (Crew Designs v79), on the real escape routes: `escape-log`, `escape-empty`, `escape-low-sample`, `escape-error`, `escape-possible`, `escape-detail`, `escape-detail-tooling`, `escape-confirmed`, `escape-dismissed`, `escape-gap`, `escape-check-proposed`, `escape-check-writing`, `escape-check-approve`, `escape-check-proven`, `escape-check-invalid`, `escape-check-retry`, `escape-check-withdraw`, `escape-check-withdrawn`, `escape-limit` (its banner follows retention: old dismissed, then possible, escapes are removed to make room, confirmed escapes and escapes with a check are kept, and a log of only kept escapes refuses; the kept count reads Not measured), `escape-settings` (on the thresholds read) and the log's narrow variants `escape-narrow-window` and `escape-narrow-panel` (a 640 px window and the 320 px side panel; the panel draws on its own at 320 px). Still drawn samples, matched to v79 because no read carries their data: `escape-report` (the mark dialog, which has no Review queue to open it from), `escape-dashboard` and the Route cards `escape-route-counts` and `escape-route-shifted` (the report and per-area counts routes are not built). The create-check proposal form stays the v20 drawn sample.

Crew, Settings › Review capacity (Crew Designs v77), on the real capacity and settings routes: the editor that Change the limit and Set a limit open, that editor in a 640 px window, and a save the service refused for want of the person key — `capacity-settings-editor`, `capacity-settings-editor-narrow`, `capacity-settings-editor-error`.

Crew, skill proposals and the skills added from lessons (Crew Designs v129, KC9), on the real routes and the mock's `crew-proposals` scenarios: the proposal waiting on the Lessons page and its narrow card, nothing proposed, the pattern that lost a lesson and its stale review, the approve and dismiss confirms, the review page and its narrow layout, the refused edit, the write in flight, the refused write, the Lessons page after an approve, the Proposed and Added from lessons groups on the Skills page (and the line while nothing is added), and the skill added from lessons on its own page — `kc9-skill-proposal-card`, `kc9-skill-proposal-card-narrow`, `kc9-skill-proposal-none`, `kc9-skill-proposal-stale`, `kc9-skill-proposal-review-stale`, `kc9-skill-proposal-confirm`, `kc9-skill-proposal-dismiss-confirm`, `kc9-skill-proposal-approved`, `kc9-skill-proposal-review`, `kc9-skill-proposal-review-narrow`, `kc9-skill-proposal-review-refused`, `kc9-skill-proposal-approving`, `kc9-skill-proposal-approve-failed`, `kc9-skill-proposal-skills-item`, `kc9-skill-proposal-skill-added`, `kc9-skills-added-group`, `kc9-skills-added-group-empty`. The dismissed state (`kc9-skill-proposal-dismissed`) is drawn by the tests rather than the shooter: confirming it needs a second click on a button the card behind the dialog names the same way.

Crew, a lesson's card and its states (Crew Designs v125, KC5), on the real lessons routes and the mock's `crew-lessons`, `crew-lessons-off`, `crew-lessons-states` and `crew-proposals-approved` scenarios, whose sample lessons follow the design's states: the card, its edit form, a lesson turned off, a correction for all repositories, the promote confirm, the refusal the contract gives a lesson that names a path, the move-back confirm, the add form, the list right after a delete with its Undo toast, a lesson over the five-lesson limit, one that became a skill, one kept out by the repository's switch, one left out of a brief, the escape and revert sources on a repository lesson, and the add's saved card — `lessons-card`, `lessons-edit`, `lessons-disabled`, `lessons-from-correction`, `lessons-promote-confirm`, `lessons-promote-refused`, `lessons-move-back-confirm`, `lessons-add`, `lessons-deleted-undo`, `lessons-card-not-in-brief-cap`, `lessons-card-not-in-brief-skill`, `lessons-card-not-in-brief-switch-off`, `lessons-left-out`, `lessons-from-escape`, `lessons-from-revert`, `lessons-add-saved`. The states that need a write to fail are drawn from the mock's own failing writes (the `crew-lessons-add-secret`, `crew-lessons-add-fails`, `crew-lessons-save-fails` and `crew-lessons-delete-fails` scenarios): the scan's refusal of an add, and the add, save and delete failures — `lessons-add-refused`, `lessons-add-failed`, `lessons-save-failed`, `lessons-delete-failed`.

Crew, the reviewer backtest (Crew Designs v23), on the real 4n routes and the mock's `crew-backtest` scenarios: `backtest-estimate`, `backtest-report` (the run named in the hash: `?id=run-4n`) and `backtest-stopped` (`?id=run-stopped`). The backtest page reads start-view: `entry-crew-backtest-page` (the project never ran one), `entry-crew-backtest-page-last-finished`, `entry-crew-backtest-page-last-stopped`, `entry-crew-backtest-page-running`, `entry-crew-backtest-page-narrow-window`, `backtest-lanes-reading`, `backtest-lanes-error` (Try again), `backtest-lanes-kind-not-recorded`, `backtest-lanes-budget-not-set`, `backtest-lanes-last-failed` and `backtest-lanes-narrow-window`; `backtest-lanes-data-map` is the contract drawing rather than a screen, so it has no catalog entry. The settings, over-budget, running, vendors, escapes, unmatched, lane-failed, empty, low-history, lane-picker and narrow frames stay drawn sample data until their own pull requests.

Crew, KC10 device notifications: the Settings row is built (Crew Designs v140 frames `kc10-notify-settings*`, system-off drawn by the shooter as `kc10-notify-settings-system-off`; on, off, denied, denied-narrow and not-asked are covered by DeviceNotifications.test.tsx because the shooter cannot set the notification permission); the cards themselves stay system notifications, not screens.

1. If the screen needs data the mock does not have, add a scenario to `web/mock/api.ts` (and its fixtures to `web/mock/fixtures.ts`).
2. Add an entry to `catalog.mjs`: `route`, `scenario`, `pre` (requests to hold or fail before the page opens), `steps` (click, fill, press, route, offline) and `wait` for states that show only after a delay.
3. List the id here (a test fails until you do), render it locally and look at all three themes.

## Development only

The tools live in `web/tools/pr-preview/`, outside `src/`: nothing in the app imports them (a test checks), and the mock API they draw on is a dev-server
feature that `scripts/assert-no-mock.mjs` keeps out of every production build.
