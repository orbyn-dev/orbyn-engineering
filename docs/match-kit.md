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

**The shell.** `shell-home`, `shell-new-thread`, `shell-ledger`

**Pull requests, the list and its states.** `pr-row-variants`, `pr-row-pending`, `pr-row-pending-failed`, `pr-list-none`, `pr-list-loading`, `pr-list-error`, `pr-list-signedout`, `pr-list-offline`, `pr-list-filtered`, `pr-tb-filter-waiting`, `pr-tb-filter-failed`

**Pull requests, one page and its states.** `pr-detail-running`, `pr-detail-failed`, `pr-detail-review`, `pr-detail-changes`, `pr-detail-approved`, `pr-detail-conflict`, `pr-detail-merged`, `pr-detail-closed`, `pr-detail-draft`, `pr-detail-conversation`, `pr-detail-files`, `pr-detail-checks`, `pr-detail-merge-menu`, `pr-detail-loading`, `pr-detail-error`, `pr-detail-signedout`, `pr-detail-offline`, `pr-detail-filtered`, `pr-review-open`

**Threads.** `thread-approval`, `thread-done`, `thread-failed`, `thread-interrupted`, `thread-empty`, `thread-tools`, `thread-tools-live`, `thread-details`

**Settings, one page for each section.** `s-general`, `s-appearance`, `s-chat`, `s-diffs`, `s-keybindings`, `s-providers`, `s-threads`, `s-tasks`, `s-projects`, `s-source-control`, `s-worktrees`, `s-maintenance`, `s-labs`, `s-about`, `s-archive`

**Account and sync.** `settings-account-gate-off`, `acct-settings-signed-out`, `acct-signin`, `acct-signin-wrong-password`, `acct-signin-network`, `acct-signup-invite`, `acct-signup-email-code`, `acct-2fa-code`, `acct-settings-signed-in`, `acct-settings-paused`, `acct-settings-loading`, `acct-settings-error`, `acct-settings-offline`, `acct-signup-check`, `acct-signup-code-wrong`, `acct-2fa-closed`, `acct-reauth`, `sync-line`, `sync-popover`, `sync-popover-offline`, `sync-popover-failed`

## Add a screen

1. If the screen needs data the mock does not have, add a scenario to `web/mock/api.ts` (and its fixtures to `web/mock/fixtures.ts`).
2. Add an entry to `catalog.mjs`: `route`, `scenario`, `pre` (requests to hold or fail before the page opens), `steps` (click, fill, press, route, offline) and `wait` for states that show only after a delay.
3. List the id here (a test fails until you do), render it locally and look at all three themes.

## Development only

The tools live in `web/tools/pr-preview/`, outside `src/`: nothing in the app imports them (a test checks), and the mock API they draw on is a dev-server
feature that `scripts/assert-no-mock.mjs` keeps out of every production build.
