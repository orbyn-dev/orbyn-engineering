// DEVELOPMENT ONLY. The screens a pull request can name in its "Frames built" line, and how the mock app is driven to each one.
//
// An id is the name of a screen (the design's frame id where the screen has one). Each entry says where to go and what to do there:
//   route     the hash route to open ("#/pulls")
//   scenario  the mock API's scenario (web/mock/api.ts); "default" when omitted
//   pre       network behaviour set up before the page opens: { route, delay } holds a request for `delay` ms, { route, status } answers it
//   steps     what to do once it has loaded, in order:
//               { do: "click", role, name, prefix?, exact?, within? }   a button, tab, checkbox ... by its accessible name (`within`: only inside the region of that name)
//               { do: "hover", role, name, prefix? }  or  { do: "hover", selector, text? }   rest the pointer there (a CSS selector, and the text it must hold)
//               { do: "fill", label, value, exact? }          a field by its label (exact when another label contains it)
//               { do: "press", key }
//               { do: "route", route, delay | status }        network behaviour from here on
//               { do: "offline" }                             the device loses its network
//               { do: "wait", ms }                            let time pass here, for a state that arrives after a step
//   width     the window width in px when the screen is drawn narrow (default 1400)
//   wait      ms to wait after load, for states that show only after a delay (default 1500)
// Only data and a few verbs: nothing here imports the app. Add a screen by adding an entry, and its id to docs/match-kit.md.

/** Settings › Crew › Autopilot by area, for the mock's first repository. */
const AREAS = "#/settings/crew/areas?repo=p-app";
const pull = (n) => `#/pulls/p-app/${n}`;
const click = (role, name, more = {}) => ({ do: "click", role, name, ...more });
const note = [{ do: "fill", label: "What needs to change?", value: "Should this reject //evil.com? The check also accepts //evil.com/x." }];
const send = click("button", "Request changes", { within: "Request changes" });
const filterNeedsMyReview = [
  click("button", "Filters", { prefix: true }),
  click("checkbox", "Needs my review"),
  { do: "press", key: "Escape" },
];

const openFilters = click("button", "Filters", { prefix: true });
const needsAndFailing = [openFilters, click("checkbox", "Needs my review"), click("checkbox", "Failing")];
const filterChips = [...needsAndFailing, { do: "press", key: "Escape" }];
/** The reviewer filter on, then a refresh the service answers with the rate limit (mock "pulls-limited*": the first details read is whole). */
const limitedAfterRefresh = [...filterNeedsMyReview, click("button", "Refresh"), { do: "wait", ms: 900 }];

/** Opens Cargo.toml from the left bar's Files view, which puts it in the editor. */
const openFile = [click("tab", "Files"), click("treeitem", "Cargo.toml", { prefix: true })];

/** Opens the side panel, then each named tab from the + launcher, the way a person does. */
const openTabs = (names) => [
  click("button", "Show side panel"),
  ...names.flatMap((name) => [click("button", "Open a tab"), click("menuitem", name, { prefix: true })]),
];

/** Create account, driven as far as `step` ("details" is the email step, "password", "code"). */
const signUpTo = (step, email = "sam@example.com") => {
  const steps = [click("button", "Create account"), { do: "fill", label: "Invite code", value: "K7QM2XWH9QXA3C2D" }, click("button", "Continue")];
  if (step === "details") return steps;
  steps.push({ do: "fill", label: "Email", value: email }, click("button", "Continue"));
  if (step === "password") return steps;
  return [...steps, ...passwords("a throwaway phrase", "a throwaway phrase"), click("button", "Continue")];
};
const passwords = (password, confirm) => [
  { do: "fill", label: "Password", value: password, exact: true },
  { do: "fill", label: "Confirm password", value: confirm },
];

/** Six digits into the six boxes, one each, then Continue. */
const typeDigits = (digits) => [...[...digits].map((digit, index) => ({ do: "fill", label: `Digit ${index + 1} of 6`, value: digit })), click("button", "Continue")];
/** A new device whose trusted device allows it, with `digits` typed once. */
const approveAndType = (digits) => [
  click("button", "Sign in"),
  { do: "fill", label: "Email", value: "approve@example.com" },
  { do: "fill", label: "Password", value: "correct horse battery" },
  click("button", "Sign in"),
  { do: "wait", ms: 2600 },
  ...typeDigits(digits),
];


/** Transcript import sheets: the same file and the same two menu steps, then whichever state the frame draws. */
const TRANSCRIPT_TITLE = "Redirect loop on logout";
const TRANSCRIPT_MESSAGES = Array.from({ length: 214 }, (_, i) => ({
  role: i % 2 === 0 ? "user" : "assistant",
  ts: "2026-11-20T09:00:05Z",
  text: `Message ${i + 1}`,
}));
const transcriptBody = (thread, over = {}) =>
  JSON.stringify({ format: "orbyn-transcript", version: 1, exported_at: "2026-11-22T10:00:00Z", thread, messages: TRANSCRIPT_MESSAGES, omitted_earlier: 0, ...over });
const importSteps = (name, thread, over = {}, after = [], content = transcriptBody(thread, over)) => [
  { do: "contextmenu", selector: ".folder-toggle", text: "orbyn-engineering-app" },
  click("menuitem", "Import transcript…"),
  { do: "file", name, mime: "application/vnd.orbyn.transcript+json", content },
  ...after,
];
const importRoute = (name, thread, over = {}, after = [], content = transcriptBody(thread, over)) => ({
  route: "#/threads/t-done",
  scenario: "transcript-import",
  steps: importSteps(name, thread, over, after, content),
  wait: 1200,
});
const engineThread = { title: TRANSCRIPT_TITLE, engine: "codex", model: "opus", created_at: "2026-11-20T09:00:00Z" };
/** A transcript over the cap, so the page refuses it before sending; 3.4 MiB as the frame shows. */
const TOO_LARGE = "x".repeat(Math.ceil(3.4 * 1024 * 1024));

export const FRAMES = {
  // The shell.
  "shell-home": { route: "#/" },
  "update-sources": { route: "#/", scenario: "updates-unconfigured" },
  "shell-new-thread": { route: "#/new" },
  // The engine picker's visible Unmanaged label, and Crew's Reviewer only chip for an engine the service limits.
  "engine-picker-closed": { route: "#/threads/t-unmanaged" },
  "engine-picker-new-thread": { route: "#/new", scenario: "crew-engine-picker" },
  "engine-picker-crew-tiers": { route: "#/settings/crew", scenario: "crew-engine-picker" },
  "engine-picker-crew-tiers-narrow": { route: "#/settings/crew", scenario: "crew-engine-picker", width: 640 },
  // The unmanaged thread's one mode, notice and labels, on the mock's managed:false Kiro.
  "unmanaged-thread-open": { route: "#/threads/t-empty", scenario: "unmanaged-thread" },
  "unmanaged-thread-open-narrow": { route: "#/threads/t-empty", scenario: "unmanaged-thread", width: 640 },
  "unmanaged-thread-first-run": {
    route: "#/threads/t-empty",
    scenario: "unmanaged-thread",
    steps: [{ do: "fill", label: "Message", value: "Run the tests." }, click("button", "Send")],
    wait: 900,
  },
  "unmanaged-thread-first-run-narrow": {
    route: "#/threads/t-empty",
    scenario: "unmanaged-thread",
    width: 640,
    steps: [{ do: "fill", label: "Message", value: "Run the tests." }, click("button", "Send")],
    wait: 900,
  },
  // The sidebar: the strip over a thread row under the pointer, and a project row with its hover controls.
  "shell-thread-row-actions": { route: "#/threads/t-done", steps: [{ do: "hover", selector: ".thread-row", text: "Keep the checks running" }] },
  "shell-project-controls": { route: "#/threads/t-done", steps: [{ do: "hover", selector: ".folder-head" }] },
  // Snap Shot (K49 and the draft flow, docs/snapshots.md), on the dev-only mock at #/preview/snapshot.
  "snapshot-settings-off": { route: "#/preview/snapshot?frame=snapshot-settings-off" },
  "snapshot-settings": { route: "#/preview/snapshot?frame=snapshot-settings" },
  "snapshot-settings-shortcut": { route: "#/preview/snapshot?frame=snapshot-settings-shortcut", steps: [click("button", "Change")] },
  "snapshot-settings-conflict": { route: "#/preview/snapshot?frame=snapshot-settings-conflict" },
  "snapshot-settings-excluded": { route: "#/preview/snapshot?frame=snapshot-settings-excluded", steps: [click("button", "Edit list…")] },
  "snapshot-settings-accessibility": { route: "#/preview/snapshot?frame=snapshot-settings-accessibility" },
  "snapshot-settings-windows": { route: "#/preview/snapshot?frame=snapshot-settings-windows" },
  "snapshot-settings-linux": { route: "#/preview/snapshot?frame=snapshot-settings-linux" },
  "snapshot-settings-off-narrow": { route: "#/preview/snapshot?frame=snapshot-settings-off-narrow", width: 640 },
  "snapshot-settings-narrow": { route: "#/preview/snapshot?frame=snapshot-settings-narrow", width: 640 },
  "snapshot-settings-excluded-narrow": { route: "#/preview/snapshot?frame=snapshot-settings-excluded-narrow", width: 640, steps: [click("button", "Edit list…")] },
  "snapshot-composer-button": { route: "#/preview/snapshot?frame=snapshot-composer-button", steps: [{ do: "hover", selector: ".snapshot-button" }] },
  "snapshot-composer-button-narrow": { route: "#/preview/snapshot?frame=snapshot-composer-button-narrow", width: 640, steps: [{ do: "hover", selector: ".snapshot-button" }] },
  "snapshot-draft": { route: "#/preview/snapshot?frame=snapshot-draft" },
  "snapshot-details": { route: "#/preview/snapshot?frame=snapshot-details", steps: [click("button", "Show Snap Shot details")] },
  "snapshot-capturing": { route: "#/preview/snapshot?frame=snapshot-capturing" },
  "snapshot-captured": { route: "#/preview/snapshot?frame=snapshot-captured" },
  "snapshot-no-thread": { route: "#/preview/snapshot?frame=snapshot-no-thread" },
  "snapshot-excluded": { route: "#/preview/snapshot?frame=snapshot-excluded" },
  "snapshot-excluded-narrow": { route: "#/preview/snapshot?frame=snapshot-excluded-narrow", width: 640 },
  "snapshot-failed": { route: "#/preview/snapshot?frame=snapshot-failed" },
  "snapshot-failed-no-window": { route: "#/preview/snapshot?frame=snapshot-failed-no-window" },
  "snapshot-failed-too-large": { route: "#/preview/snapshot?frame=snapshot-failed-too-large" },
  "snapshot-failed-timeout": { route: "#/preview/snapshot?frame=snapshot-failed-timeout" },
  "snapshot-failed-error": { route: "#/preview/snapshot?frame=snapshot-failed-error" },
  "snapshot-failed-narrow": { route: "#/preview/snapshot?frame=snapshot-failed-narrow", width: 640 },
  "snapshot-setup-mac": { route: "#/preview/snapshot?frame=snapshot-setup-mac" },
  "snapshot-setup-asked": { route: "#/preview/snapshot?frame=snapshot-setup-asked", steps: [click("button", "Allow Screen Recording")] },
  "snapshot-setup-denied": { route: "#/preview/snapshot?frame=snapshot-setup-denied" },
  "snapshot-setup-accessibility": { route: "#/preview/snapshot?frame=snapshot-setup-accessibility" },
  "snapshot-setup-granted": { route: "#/preview/snapshot?frame=snapshot-setup-granted", steps: [click("button", "Test capture")] },
  "snapshot-setup-windows": { route: "#/preview/snapshot?frame=snapshot-setup-windows" },
  "snapshot-setup-narrow": { route: "#/preview/snapshot?frame=snapshot-setup-narrow", width: 640 },
  "snapshot-narrow": { route: "#/preview/snapshot?frame=snapshot-narrow", width: 640 },
  "shell-ledger": {
    route: "#/threads/t-done",
    steps: [
      click("link", "Settings", { exact: true }),
      click("link", "Appearance", { exact: true }),
      click("radio", "Ledger", { prefix: true }),
      click("link", "Chat", { exact: true }),
      click("radio", "Full", { exact: true }),
      { do: "press", key: "Escape" },
    ],
  },

  // Pull requests: the list and its states (mock scenarios "pulls*").
  "pr-row-variants": { route: "#/pulls", scenario: "pulls" },
  "pr-row-pending": { route: "#/pulls", scenario: "pulls", pre: [{ route: "**/api/pulls/details", delay: 20000 }] },
  "pr-row-pending-failed": { route: "#/pulls", scenario: "pulls-details-failed" },
  // From a list row the method is a select in the dialog.
  "pr-list-merge-dialog": { route: "#/pulls", scenario: "pulls", steps: [click("button", "Merge #", { prefix: true })] },
  "pr-list-none": { route: "#/pulls", scenario: "pulls-none" },
  "pr-list-loading": { route: "#/pulls", scenario: "pulls", pre: [{ route: "**/api/pulls?*", delay: 20000 }] },
  "pr-list-error": { route: "#/pulls", scenario: "pulls-unreachable" },
  "pr-list-signedout": { route: "#/pulls", scenario: "pulls-signed-out" },
  "pr-list-draft": { route: "#/pulls", scenario: "pulls", steps: [click("button", "Drafts", { prefix: true })] },
  "pr-list-offline": { route: "#/pulls", scenario: "pulls", steps: [{ do: "offline" }] },
  "pr-list-filtered": {
    route: "#/pulls",
    scenario: "pulls",
    steps: [click("button", "Filters", { prefix: true }), click("checkbox", "Running"), click("radio", "Only"), { do: "press", key: "Escape" }],
  },
  "pr-tb-filter-waiting": {
    route: "#/pulls",
    scenario: "pulls",
    steps: [...filterNeedsMyReview, { do: "route", route: "**/api/pulls/details", delay: 20000 }, click("button", "Closed", { prefix: true })],
    wait: 900,
  },
  "pr-tb-filter-failed": {
    route: "#/pulls",
    scenario: "pulls",
    steps: [...filterNeedsMyReview, { do: "route", route: "**/api/pulls/details", status: 502 }, click("button", "Closed", { prefix: true })],
    wait: 700,
  },

  // Pull requests, the toolbar (Engineer Designs pr-tb-*): each menu and state of the row above the list.
  "pr-tb-default": { route: "#/pulls", scenario: "pulls" },
  "pr-tb-search": { route: "#/pulls", scenario: "pulls", steps: [{ do: "fill", label: "Search pull requests", value: "label:bug login" }] },
  "pr-tb-filters": { route: "#/pulls", scenario: "pulls", steps: needsAndFailing },
  "pr-tb-chips": { route: "#/pulls", scenario: "pulls", steps: filterChips },
  "pr-tb-sort": { route: "#/pulls", scenario: "pulls", steps: [click("button", "Sort", { prefix: true })] },
  "pr-tb-project": { route: "#/pulls", scenario: "pulls", steps: [click("button", "All projects", { prefix: true })] },
  // The read is held back, so the header says Refreshing and the rows stay.
  "pr-tb-refreshing": {
    route: "#/pulls",
    scenario: "pulls",
    steps: [{ do: "route", route: "**/api/pulls?*", delay: 20000 }, click("button", "Refresh")],
    wait: 900,
  },
  "pr-tb-loading": { route: "#/pulls", scenario: "pulls", pre: [{ route: "**/api/pulls?*", delay: 20000 }] },
  // A project, a search and two filters that match nothing.
  "pr-tb-empty": {
    route: "#/pulls",
    scenario: "pulls",
    steps: [click("button", "All projects", { prefix: true }), click("menuitemradio", "orbyn-engineering-app", { prefix: true }), ...filterChips],
  },
  "pr-tb-author": { route: "#/pulls", scenario: "pulls", steps: [openFilters, { do: "fill", label: "Search people", value: "ri" }] },
  "pr-tb-author-none": { route: "#/pulls", scenario: "pulls", steps: [openFilters, { do: "fill", label: "Search people", value: "zq" }] },
  "pr-tb-filter-rate-limited": { route: "#/pulls", scenario: "pulls-limited", steps: limitedAfterRefresh },
  // The pause ends two seconds after every read; the check at that time finds the limit still on, and the banner says when the next one is.
  "pr-tb-filter-still-limited": { route: "#/pulls", scenario: "pulls-limited-still", steps: [...limitedAfterRefresh, { do: "wait", ms: 3200 }] },
  "pr-tb-narrow": { route: "#/pulls", scenario: "pulls", width: 900, steps: filterChips },
  "pr-tb-phone": { route: "#/pulls", scenario: "pulls", width: 390, steps: filterChips },
  "pr-list-narrow-900": { route: "#/pulls", scenario: "pulls", width: 900 },
  // The drawn steps below it: the list reads its own width (the window minus 56 px, and minus the docked 224 px sidebar from 900 up).
  "pr-list-narrow-800": { route: "#/pulls", scenario: "pulls", width: 800 },
  "pr-list-narrow-700": { route: "#/pulls", scenario: "pulls", width: 700 },
  "pr-list-narrow-640": { route: "#/pulls", scenario: "pulls", width: 640 },
  "pr-list-narrow-580": { route: "#/pulls", scenario: "pulls", width: 580 },
  // The cell's tooltip says why a value is missing.
  "pr-row-tooltips": { route: "#/pulls", scenario: "pulls", steps: [{ do: "hover", selector: ".pull-rows > li:last-child .pull-reviewers-cell" }] },
  // Two rows selected, the pointer on the Ready to merge row: its Merge and Close appear at its end.
  "pr-row-actions-hover": {
    route: "#/pulls",
    scenario: "pulls-ready",
    steps: [click("checkbox", "Select #139"), click("checkbox", "Select #138"), { do: "hover", selector: ".pull-rows > li", text: "Settings scoped rows" }],
  },

  // Pull requests: one pull request as a page, in each state (201 running ... 209 draft, see mock/fixtures.ts pageStates).
  "pr-detail-running": { route: pull(201), scenario: "pulls" },
  "pr-detail-failed": { route: pull(202), scenario: "pulls" },
  "pr-detail-review": { route: pull(203), scenario: "pulls" },
  "pr-detail-changes": { route: pull(204), scenario: "pulls" },
  "pr-detail-approved": { route: pull(205), scenario: "pulls" },
  "pr-detail-conflict": { route: pull(206), scenario: "pulls" },
  "pr-detail-merged": { route: pull(207), scenario: "pulls" },
  "pr-detail-closed": { route: pull(208), scenario: "pulls" },
  "pr-detail-draft": { route: pull(209), scenario: "pulls" },
  "pr-detail-conversation": { route: pull(202), scenario: "pulls", steps: [click("tab", "Conversation", { prefix: true })] },
  "pr-detail-files": { route: pull(202), scenario: "pulls", steps: [click("tab", "Files changed", { prefix: true })] },
  "pr-detail-checks": { route: pull(202), scenario: "pulls", steps: [click("tab", "Checks", { prefix: true })] },
  "pr-detail-merge-menu": { route: pull(205), scenario: "pulls", steps: [click("button", "Squash and merge", { prefix: true })] },
  "pr-detail-loading": { route: pull(201), scenario: "pulls", pre: [{ route: "**/api/projects/*/pulls/201", delay: 20000 }] },
  "pr-detail-error": { route: pull(201), scenario: "pulls", pre: [{ route: "**/api/projects/*/pulls/201", status: 502 }] },
  "pr-detail-signedout": { route: pull(201), scenario: "pulls-signed-out", pre: [{ route: "**/api/projects/*/pulls/201", status: 502 }] },
  // Nothing cached: the header says Pull request #N and the page only that the details did not load (the same frame as pr-detail-error, opened cold).
  "pr-detail-error-uncached": { route: pull(142), scenario: "pulls", pre: [{ route: "**/api/projects/*/pulls/142", status: 502 }] },
  "pr-detail-menu": { route: pull(205), scenario: "pulls", steps: [click("button", "More actions")] },
  "pr-detail-merge-dialog": { route: pull(205), scenario: "pulls", steps: [click("button", "Merge", { exact: true })] },
  // The dialog's states. The account setting is off in the mock, so the switch reads off; the on frame drives a mock with
  // it on. The design's two read-only frames described the switch before the service obeys the per-merge value; it does now
  // (crates/orbyn-core/src/source_control/scm/review.rs), so the switch is editable and only the setting decides its start.
  "pr-detail-merge-dialog-credits-readonly": { route: pull(205), scenario: "pulls", steps: [click("button", "Merge", { exact: true })] },
  "pr-detail-merge-dialog-credits-readonly-on": { route: pull(205), scenario: "pulls-credits-on", steps: [click("button", "Merge", { exact: true })] },
  "pr-detail-merge-dialog-auto": {
    route: pull(205),
    scenario: "pulls",
    steps: [click("button", "Squash and merge", { prefix: true }), click("menuitemcheckbox", "Merge when checks pass"), click("button", "Merge", { exact: true })],
  },
  "pr-detail-merge-dialog-rebase": {
    route: pull(205),
    scenario: "pulls",
    steps: [click("button", "Squash and merge", { prefix: true }), click("menuitem", "Rebase and merge"), click("button", "Merge", { exact: true })],
  },
  "pr-detail-merge-dialog-busy": {
    route: pull(205),
    scenario: "pulls",
    pre: [{ route: "**/api/projects/*/pulls/205/merge", delay: 20000 }],
    steps: [click("button", "Merge", { exact: true }), click("button", "Merge", { exact: true, within: "Merge pull request" }), { do: "wait", ms: 300 }],
  },
  "pr-detail-merge-dialog-error": {
    route: pull(205),
    scenario: "pulls",
    pre: [{ route: "**/api/projects/*/pulls/205/merge", status: 502 }],
    steps: [click("button", "Merge", { exact: true }), click("button", "Merge", { exact: true, within: "Merge pull request" })],
  },
  "pr-detail-offline": { route: pull(201), scenario: "pulls", steps: [{ do: "offline" }] },
  "pr-detail-filtered": {
    route: pull(202),
    scenario: "pulls",
    steps: [click("tab", "Files changed", { prefix: true }), { do: "fill", label: "Filter files", value: "migrations" }],
  },
  "pr-review-open": { route: pull(203), scenario: "pulls", steps: [click("button", "Request changes")] },
  "pr-review-filled": { route: pull(203), scenario: "pulls", steps: [click("button", "Request changes"), ...note] },
  // The answer is held back, so the card shows Requesting…
  "pr-review-busy": {
    route: pull(203),
    scenario: "pulls",
    pre: [{ route: "**/api/projects/*/pulls/203/review", delay: 20000 }],
    steps: [click("button", "Request changes"), ...note, send],
    wait: 900,
  },
  "pr-review-done": { route: pull(203), scenario: "pulls", steps: [click("button", "Request changes"), ...note, send, { do: "wait", ms: 600 }] },
  // The forge refuses it (a 409, so nothing was sent).
  "pr-review-error": {
    route: pull(203),
    scenario: "pulls",
    pre: [{ route: "**/api/projects/*/pulls/203/review", status: 409 }],
    steps: [click("button", "Request changes"), ...note, send],
  },
  "pr-review-own": { route: pull(210), scenario: "pulls", steps: [click("button", "Request changes")] },

  // Threads: run states, the tool call states, the details card.
  "thread-approval": { route: "#/threads/t-approval" },
  "thread-done": { route: "#/threads/t-done" },
  "thread-failed": { route: "#/threads/t-failed" },
  "thread-run-no-reply": { route: "#/threads/t-run-no-reply" },
  "thread-run-session-lost": { route: "#/threads/t-run-session-lost" },
  "thread-background-stopped": { route: "#/threads/t-empty" },
  "thread-background-stopped-many": { route: "#/threads/t-empty-many" },
  "thread-background-stopped-narrow": { route: "#/threads/t-empty", width: 640 },
  "thread-interrupted": { route: "#/threads/t-interrupted" },
  "thread-empty": { route: "#/threads/t-empty" },
  "thread-tools": { route: "#/threads/t-tools" },
  // Design frames `summary-work-group` and `summary-outcomes`: a turn's groups, with the failed and unanswered calls counted apart.
  "summary-work-group": { route: "#/threads/t-summary-work", steps: [click("button", "Changed 2 files", { prefix: true })] },
  "summary-outcomes": { route: "#/threads/t-summary-outcomes", steps: [click("button", "Ran 1 command", { prefix: true }), click("button", "Ran 4 commands", { prefix: true })] },
  "thread-tools-live": { route: "#/threads/t-tools-live" },
  "thread-details": { route: "#/threads/t-done", steps: [click("button", "Details")] },
  "details-popover": { route: "#/threads/t-done", steps: [click("button", "Details")] },

  // The side panel: starts with no tab, the + launcher, the tab menu (Engineer Designs v98). panel-tabs-drag is a pointer drag and has no verb here.
  "panel-empty": { route: "#/threads/t-done", steps: [click("button", "Show side panel")] },
  "panel-launcher-menu": { route: "#/threads/t-done", steps: [...openTabs(["Changes"]), click("button", "Open a tab")] },
  "panel-tab-menu": { route: "#/threads/t-done", steps: [...openTabs(["Changes", "Browser", "Tools"]), click("tab", "Changes"), click("button", "Tab menu")] },
  "panel-tabs-single": { route: "#/threads/t-done", steps: openTabs(["Changes"]) },
  "panel-browser": { route: "#/threads/t-done", steps: openTabs(["Changes", "Browser"]) },
  "panel-activity": { route: "#/threads/t-done", steps: openTabs(["Changes", "Browser", "Activity"]) },
  "panel-side": { route: "#/threads/t-done", steps: openTabs(["Changes", "Browser", "Activity", "Side chat"]) },

  // Changes (mock threads: t-approval has three changed files, t-done none). The conflict, binary, rename, huge, staged, Markdown, more-files,
  // turn-picker, commit-picker and comment-pending frames need data the mock does not carry; they are drawn from fixtures in the unit tests.
  "changes-default": { route: "#/threads/t-approval", steps: openTabs(["Changes"]) },
  "changes-empty": { route: "#/threads/t-done", steps: openTabs(["Changes"]) },
  "changes-error": { route: "#/threads/t-approval", pre: [{ route: "**/api/threads/*/diff*", status: 500 }], steps: openTabs(["Changes"]) },
  "changes-loading": { route: "#/threads/t-approval", pre: [{ route: "**/api/threads/*/diff*", delay: 20000 }], steps: openTabs(["Changes"]), wait: 900 },
  "changes-view-menu": { route: "#/threads/t-approval", steps: [...openTabs(["Changes"]), click("button", "View options")] },
  "changes-compare-menu": {
    route: "#/threads/t-approval",
    steps: [...openTabs(["Changes"]), click("button", "Scope:", { prefix: true }), click("menuitem", "Branch"), click("button", "Compare", { prefix: true })],
  },
  "changes-find": {
    route: "#/threads/t-approval",
    steps: [...openTabs(["Changes"]), click("group", "Patch"), { do: "press", key: "Control+f" }, { do: "fill", label: "Find in diff", value: "expire" }],
  },
  "changes-file-menu": {
    route: "#/threads/t-approval",
    steps: [...openTabs(["Changes"]), click("button", "Actions for", { prefix: true })],
  },
  "comment-select": {
    route: "#/threads/t-approval",
    steps: [...openTabs(["Changes"]), click("button", "Comment on line 87")],
  },

  // The file editor (Engineer Designs v106). A file of the mock thread's own worktree opens in it; binary, read-only (another worktree), large, comment,
  // save-error, conflict and narrow need data or a failing write the mock does not carry, so they are drawn from fixtures in the unit tests.
  "editor-default": { route: "#/threads/t-done", steps: openFile },
  "editor-unsaved": { route: "#/threads/t-done", steps: [...openFile, click("textbox", "Edit Cargo.toml"), { do: "press", key: "End" }, { do: "press", key: "x" }] },
  "editor-find": { route: "#/threads/t-done", steps: [...openFile, click("button", "Find ⌘F"), { do: "fill", label: "Find in file", value: "//" }] },
  "editor-goto": { route: "#/threads/t-done", steps: [...openFile, click("button", "Go to line ⌘G")] },
  "editor-tab-menu": { route: "#/threads/t-done", steps: [...openFile, click("button", "Tab menu")] },
  "editor-close-confirm": {
    route: "#/threads/t-done",
    steps: [...openFile, click("textbox", "Edit Cargo.toml"), { do: "press", key: "x" }, click("button", "Tab menu"), click("menuitem", "Close tab")],
  },
  "editor-loading": { route: "#/threads/t-done", pre: [{ route: "**/api/threads/*/files/content*", delay: 20000 }], steps: openFile, wait: 900 },

  // Usage, the Limits tab (mock scenarios "limits*"): only Codex reports a window; Claude Code and the others read Not reported.
  "usage-claude-windows": { route: "#/usage", scenario: "limits-claude", steps: [click("tab", "Limits")] },
  "usage-claude-windows-unavailable": { route: "#/usage", scenario: "limits", steps: [click("tab", "Limits")] },
  "usage-claude-windows-loading": { route: "#/usage", scenario: "limits", pre: [{ route: "**/api/limits*", delay: 20000 }], steps: [click("tab", "Limits")], wait: 900 },
  "usage-claude-windows-narrow": { route: "#/usage", scenario: "limits", width: 640, steps: [click("tab", "Limits")] },
  "usage-claude-windows-near": { route: "#/usage", scenario: "limits-near", steps: [click("tab", "Limits")] },
  "usage-claude-windows-reached": { route: "#/usage", scenario: "limits-reached", steps: [click("tab", "Limits")] },
  "usage-claude-windows-signedout": { route: "#/usage", scenario: "limits-signedout", steps: [click("tab", "Limits")] },
  "usage-providers": { route: "#/usage", scenario: "limits-providers", steps: [click("tab", "Limits")] },
  "usage-providers-error": { route: "#/usage", scenario: "limits-error", steps: [click("tab", "Limits")] },


  // Settings: one page for each section.
  "s-general": { route: "#/settings/general" },
  "s-appearance": { route: "#/settings/appearance" },
  "s-chat": { route: "#/settings/chat" },
  "s-diffs": { route: "#/settings/diffs" },
  "s-keybindings": { route: "#/settings/keybindings" },
  "s-providers": { route: "#/settings/providers" },
  "s-threads": { route: "#/settings/threads" },
  "s-tasks": { route: "#/settings/tasks" },
  "s-projects": { route: "#/settings/projects" },
  "s-source-control": { route: "#/settings/source-control" },
  "s-worktrees": { route: "#/settings/worktrees" },
  "s-maintenance": { route: "#/settings/maintenance" },
  "s-labs": { route: "#/settings/labs" },
  "s-about": { route: "#/settings/about" },
  "s-archive": { route: "#/settings/archive" },

  // Account and sync (mock scenario "account": the sign-in gate on, with the throwaway accounts in web/mock/account.ts).
  "settings-account-gate-off": { route: "#/settings/general" },
  "acct-settings-signed-out": { route: "#/settings/account", scenario: "account" },
  "acct-signin": { route: "#/settings/account", scenario: "account", steps: [click("button", "Sign in")] },
  "acct-signin-wrong-password": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "nope" },
      click("button", "Sign in"),
    ],
  },
  "acct-signin-network": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "down@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-signup-invite": { route: "#/settings/account", scenario: "account", steps: [click("button", "Create account")] },
  "acct-signup-details": { route: "#/settings/account", scenario: "account", steps: [...signUpTo("details")] },
  "acct-signup-password": { route: "#/settings/account", scenario: "account", steps: [...signUpTo("password")] },
  "acct-signup-password-weak": {
    route: "#/settings/account",
    scenario: "account",
    steps: [...signUpTo("password"), ...passwords("short pw", "short pw"), click("button", "Continue")],
  },
  "acct-signup-password-mismatch": {
    route: "#/settings/account",
    scenario: "account",
    steps: [...signUpTo("password"), ...passwords("a throwaway phrase", "another phrase here"), click("button", "Continue")],
  },
  "acct-signup-password-network": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      ...signUpTo("password"),
      ...passwords("a throwaway phrase", "a throwaway phrase"),
      { do: "route", route: "**/api/account/sign-up/start", status: 503, code: "account_offline" },
      click("button", "Continue"),
    ],
  },
  "acct-signup-check-failed": {
    route: "#/settings/account",
    scenario: "account",
    steps: [...signUpTo("password", "hard@example.com"), ...passwords("a throwaway phrase", "a throwaway phrase"), click("button", "Continue")],
  },
  "acct-signup-email-code": { route: "#/settings/account", scenario: "account", steps: [...signUpTo("code")] },
  "acct-signup-code-wrong": {
    route: "#/settings/account",
    scenario: "account",
    steps: [...signUpTo("code"), { do: "fill", label: "8-character code", value: "K7QMR4TX" }, click("button", "Verify")],
  },
  "acct-signup-code-network": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      ...signUpTo("code"),
      { do: "fill", label: "8-character code", value: "K7QMR4TW" },
      { do: "route", route: "**/api/account/sign-up/code", status: 503, code: "account_offline" },
      click("button", "Verify"),
    ],
  },
  "acct-signup-done": {
    route: "#/settings/account",
    scenario: "account",
    steps: [...signUpTo("code"), { do: "fill", label: "8-character code", value: "ABCDEFGH" }, click("button", "Verify")],
  },
  "acct-2fa-code": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "two@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-settings-signed-in": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "sync-popover": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Account and sync", { prefix: true }),
    ],
  },
  "devices-list": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Manage devices"),
    ],
  },
  "devices-remove-confirm": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Manage devices"),
      click("button", "Device menu", { first: true }),
      click("menuitem", "Remove"),
    ],
  },
  "signout-confirm": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Sign out", { exact: true }),
    ],
  },
  "signout-everywhere": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Sign out everywhere…"),
    ],
  },
  "acct-device-waiting": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "late@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "devices-waiting-approval": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Manage devices"),
    ],
  },
  "acct-device-approve-prompt": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Manage devices"),
      click("button", "Review"),
    ],
  },
  "acct-device-approve-digits": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Manage devices"),
      click("button", "Review"),
      click("button", "Allow"),
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Confirm"),
    ],
  },
  "acct-reauth-network": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "dev@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Sign out everywhere…"),
      { do: "fill", label: "Password", value: "correct horse battery" },
      { do: "route", route: "**/api/account/confirm", status: 503, code: "account_offline" },
      click("button", "Confirm"),
    ],
  },
  "acct-device-approved": {
    route: "#/settings/account",
    scenario: "account",
    wait: 2600,
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "approve@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-device-denied": {
    route: "#/settings/account",
    scenario: "account",
    wait: 4200,
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "deny@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-device-timeout": {
    route: "#/settings/account",
    scenario: "account",
    wait: 3200,
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "expired@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-device-enter-digits": {
    route: "#/settings/account",
    scenario: "account",
    wait: 2600,
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "approve@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-device-codes-mismatch": {
    route: "#/settings/account",
    scenario: "account",
    wait: 2600,
    steps: [...approveAndType("529318"), { do: "wait", ms: 600 }],
  },
  "acct-device-digits-locked": {
    route: "#/settings/account",
    scenario: "account",
    wait: 2600,
    steps: [...approveAndType("529318"), ...typeDigits("529318"), ...typeDigits("529318"), { do: "wait", ms: 600 }],
  },
  "acct-device-check-failed": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "late@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      { do: "route", route: "**/api/account/sign-in/approval", status: 502 },
      click("button", "Sign in"),
      { do: "wait", ms: 7500 },
    ],
  },
  "acct-settings-not-connected": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "connect@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Not now"),
    ],
  },
  "sync-popover-not-connected": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "connect@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Not now"),
      click("button", "Account and sync", { prefix: true }),
    ],
  },
  "acct-connect-workspace": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "connect@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },
  "acct-connect-progress": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "connect@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Connect", { exact: true }),
    ],
  },
  "acct-connect-done": {
    route: "#/settings/account",
    scenario: "account",
    wait: 5500,
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "connect@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
      click("button", "Connect", { exact: true }),
    ],
  },
  "signout-other-device-removed": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Sign in"),
      { do: "fill", label: "Email", value: "removed@example.com" },
      { do: "fill", label: "Password", value: "correct horse battery" },
      click("button", "Sign in"),
    ],
  },

  // Crew, the wide Map tree and the swim lanes (Crew Designs' crew-map and lanes-main; built from the map and message reads).
  "crew-map": { route: "#/crew/map", scenario: "crew" },
  "lanes-main": { route: "#/crew/lanes", scenario: "crew" },

  // Live stage-list adaptations of the approved consent/badge frames.
  "flow-offline": { route: "#/crew/flow", scenario: "crew-review-consent", steps: [click("button", "Use next in the auth redirect", { prefix: true }), click("button", "security: Needs you")] },
  "lane-same-engine": { route: "#/crew/flow", scenario: "crew-review-same", steps: [click("button", "Use next in the auth redirect", { prefix: true })] },
  "lane-same-engine-full": { route: "#/crew/flow", scenario: "crew-review-full", steps: [click("button", "Use next in the auth redirect", { prefix: true })] },

  // Approved two-control additions @ git:6e509d39c0486c2c51a119416d24c5443937acd5.
  "lane-remove-reason": { route: "#/crew/flow", scenario: "crew-review-remove", steps: [click("button", "Review dependency licences", { prefix: true }), click("button", "Remove…"), { do: "fill", label: "Reason", value: "This change only updates documentation; the dependency licences are unchanged." }] },
  "settings-acp-tier-editor": { route: "#/settings/crew", scenario: "crew", steps: [click("button", "Edit tiers"), click("region", "Engine tiers")] },

  // Crew, Autopilot by area (Crew Designs v39). Settings › Crew only draws while the mock has Crew Preview on.
  "autonomy-table": { route: AREAS, scenario: "crew-areas" },
  "autonomy-tripped": { route: AREAS, scenario: "crew-areas-tripped" },
  "autonomy-demoted-ceiling-lowered": { route: AREAS, scenario: "crew-areas-ceiling-off" },
  "autonomy-recovering": { route: AREAS, scenario: "crew-areas-recovering" },
  "autonomy-criteria-met": { route: AREAS, scenario: "crew-areas-criteria-met" },
  "autonomy-restored": { route: AREAS, scenario: "crew-areas-restored" },
  "autonomy-lowdata": { route: AREAS, scenario: "crew-areas-lowdata" },
  "autonomy-empty": { route: AREAS, scenario: "crew-areas-empty" },
  "autonomy-repos": { route: AREAS, scenario: "crew" },
  "autonomy-entry": { route: "#/settings/crew", scenario: "crew-areas" },
  "settings-crew": { route: "#/settings/crew", scenario: "crew" },
  "settings-crew-worker-provider": { route: "#/settings/crew", scenario: "crew" },
  "settings-crew-worker-provider-any": { route: "#/settings/crew", scenario: "crew-worker-provider-any" },
  "settings-crew-worker-provider-narrow": { route: "#/settings/crew", scenario: "crew", width: 640 },
  "autonomy-override": { route: AREAS, scenario: "crew-areas-tripped", steps: [click("button", "Override…", { within: "Why web/auth moved from Full to On" })] },
  "autonomy-override-warning": {
    route: AREAS,
    scenario: "crew-areas-tripped",
    steps: [click("button", "Override…", { within: "Why web/auth moved from Full to On" }), click("menuitem", "Keep the ceiling"), click("checkbox", "I understand", { prefix: true })],
  },
  "autonomy-override-off-warning": {
    route: AREAS,
    scenario: "crew-areas-tripped",
    steps: [click("button", "Override…", { within: "Why web/auth moved from Full to On" }), click("menuitem", "Off"), click("checkbox", "I understand", { prefix: true })],
  },
  "autonomy-thresholds": { route: "#/settings/crew/areas/thresholds?repo=p-app", scenario: "crew-areas" },
  "autonomy-narrow": { route: AREAS, scenario: "crew-areas-tripped", width: 640 },
  // Crew, the guard, the path classes and routing (Crew Designs frames `guard-*`, `classes-*`, `route-log*`). Crew shows only while the mock has Preview on.
  // Not here: `guard-in-flow`, `guard-needsyou` and `route-class-*` are drawn inside Flow, Needs you and the Route card, which other work owns; `guard-approve-myself`
  // is the desktop shell's own dialog over `guard-denied-no-second`; `guard-narrow-panel` is the side panel's card list, and `route-class-*-narrow` the Route card at 320 px.
  "approval-one-vendor": { route: "#/crew/approvals", scenario: "crew-one-vendor", steps: [click("button", "Install zod", { prefix: true })] },
  "guard-held": { route: "#/crew/approvals", scenario: "crew-guard", steps: [click("button", 'psql "$PROD_DATABASE_URL" -c "DELETE FROM orders', { prefix: true })] },
  "guard-denied-no-second": { route: "#/crew/approvals", scenario: "crew-guard-full", steps: [click("button", 'psql "$PROD_DATABASE_URL" -c "TRUNCATE audit_log', { prefix: true })] },
  "guard-denied": { route: "#/crew/approvals", scenario: "crew-guard-full", steps: [click("button", 'psql "$PROD_DATABASE_URL" -c "ALTER TABLE users', { prefix: true })] },
  "guard-approved": { route: "#/crew/approvals", scenario: "crew-guard-full", steps: [click("button", 'psql "$PROD_DATABASE_URL" -c "DELETE FROM sessions', { prefix: true })] },
  "guard-approved-person": { route: "#/crew/approvals", scenario: "crew-guard", steps: [click("button", 'psql "$PROD_DATABASE_URL" -c "DELETE FROM audit_log', { prefix: true })] },
  "guard-approved-person-unknown": { route: "#/crew/approvals", scenario: "crew-guard-unknown", steps: [click("button", 'psql "$PROD_DATABASE_URL" -c "DELETE FROM audit_log', { prefix: true })] },
  "guard-settings": { route: "#/settings/crew", scenario: "crew-guard" },
  "classes-list": { route: "#/settings/crew", scenario: "crew-classes" },
  "classes-pinned-ref-blocked": { route: "#/settings/crew", scenario: "crew-classes-pinned", steps: [click("heading", "Crew › Path classes")] },
  "classes-error": { route: "#/settings/crew", scenario: "crew-classes-invalid" },
  "classes-test": { route: "#/settings/crew", scenario: "crew-classes", steps: [click("button", "Test a path"), { do: "fill", label: "Paths to test, one per line", value: "auth/redirect.ts" }] },
  "classes-edit": { route: "#/settings/crew", scenario: "crew-classes", steps: [click("button", "billing", { exact: true })] },
  "classes-narrow": { route: "#/settings/crew", scenario: "crew-classes", width: 640 },
  "route-log-none": { route: "#/crew/approvals", scenario: "crew-route-none", steps: [click("button", "Routing", { prefix: true })] },
  "route-log": { route: "#/crew/approvals", scenario: "crew-route-blocked", steps: [click("button", "Routing", { prefix: true })] },

  // Crew, the background create form and its daily budget (Crew Designs frames background-new, background-new-label,
  // background-new-schedule and background-create-daily-*). The mock's crew-background scenario lists crews, so the
  // header's New background crew opens the form with the default template, budget and daily budget.
  "background-new": { route: "#/crew/background", scenario: "crew-background", steps: [click("button", "New background crew")] },
  "background-new-label": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [click("button", "New background crew"), click("radio", "An issue gets a label"), { do: "fill", label: "Label", value: "flaky-test", exact: true }],
  },
  "background-new-schedule": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [click("button", "New background crew"), click("radio", "On a schedule")],
  },
  "background-create-daily-max": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      click("radio", "On a schedule"),
      { do: "fill", label: "Turns", value: "1000", exact: true },
      { do: "fill", label: "Minutes", value: "1440", exact: true },
      { do: "fill", label: "Runs a day", value: "1000", exact: true },
      { do: "fill", label: "Turns a day", value: "10000", exact: true },
    ],
  },
  "background-create-daily-min": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      click("radio", "An issue gets a label"),
      { do: "fill", label: "Label", value: "crew-fix", exact: true },
      { do: "fill", label: "Turns", value: "1", exact: true },
      { do: "fill", label: "Minutes", value: "1", exact: true },
      { do: "fill", label: "Runs a day", value: "1", exact: true },
      { do: "fill", label: "Turns a day", value: "1", exact: true },
    ],
  },
  "background-create-daily-invalid": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      { do: "fill", label: "Runs a day", value: "1001", exact: true },
      { do: "fill", label: "Turns a day", value: "0", exact: true },
    ],
  },
  "background-create-daily-range-per-run": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      { do: "fill", label: "Turns", value: "0", exact: true },
      { do: "fill", label: "Minutes", value: "1441", exact: true },
    ],
  },
  "background-create-daily-range-empty": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      click("radio", "An issue gets a label"),
      { do: "fill", label: "Label", value: "crew-fix", exact: true },
      { do: "fill", label: "Runs a day", value: "", exact: true },
      { do: "fill", label: "Turns a day", value: "1.5", exact: true },
    ],
  },
  "background-create-daily-range-hours": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      click("radio", "On a schedule"),
      { do: "fill", label: "Every N hours", value: "0", exact: true },
    ],
  },
  "background-create-daily-range-name-label": {
    route: "#/crew/background",
    scenario: "crew-background",
    steps: [
      click("button", "New background crew"),
      click("radio", "An issue gets a label"),
      { do: "fill", label: "Name", value: "", exact: true },
    ],
  },
  "background-create-daily-range-narrow": {
    route: "#/crew/background",
    scenario: "crew-background",
    width: 520,
    steps: [
      click("button", "New background crew"),
      { do: "fill", label: "Turns", value: "0", exact: true },
      { do: "fill", label: "Minutes", value: "1441", exact: true },
      { do: "fill", label: "Runs a day", value: "1001", exact: true },
      { do: "fill", label: "Turns a day", value: "0", exact: true },
    ],
  },

  /** Crew, the escape log (Crew Designs v79), on the real escape routes over the mock's crew-escapes scenarios. A detail
   *  frame names its escape in the hash; escape-gap opens the same escape with its Set-the-gap dialog. */
  "escape-log": { route: "#/preview/crew/escape-log", scenario: "crew-escapes", wait: 2500 },
  "escape-empty": { route: "#/preview/crew/escape-empty", scenario: "crew-escapes-empty" },
  "escape-low-sample": { route: "#/preview/crew/escape-low-sample", scenario: "crew-escapes-low" },
  "escape-error": { route: "#/preview/crew/escape-error", scenario: "crew-escapes-unreadable" },
  "escape-possible": { route: "#/preview/crew/escape-possible", scenario: "crew-escapes" },
  "escape-detail": { route: "#/preview/crew/escape-detail?id=esc-returnto", scenario: "crew-escapes" },
  "escape-detail-tooling": { route: "#/preview/crew/escape-detail?id=esc-next", scenario: "crew-escapes" },
  "escape-confirmed": { route: "#/preview/crew/escape-confirmed?id=esc-session", scenario: "crew-escapes" },
  "escape-dismissed": { route: "#/preview/crew/escape-dismissed?id=esc-docs", scenario: "crew-escapes" },
  "escape-gap": { route: "#/preview/crew/escape-gap?id=esc-session", scenario: "crew-escapes", steps: [click("button", "Set the gap…")] },
  /** Crew, the reviewer backtest (Crew Designs v23), on the real 4n routes over the mock's crew-backtest scenarios. A report
   *  frame names its run in the hash, the way an escape detail names its escape. The estimate draws its dialog open. */
  "backtest-estimate": { route: "#/preview/crew/backtest-estimate", scenario: "crew-backtest", wait: 2500 },
  "backtest-report": { route: "#/preview/crew/backtest-report?id=run-4n", scenario: "crew-backtest-report" },
  "backtest-stopped": { route: "#/preview/crew/backtest-stopped?id=run-stopped", scenario: "crew-backtest-stopped" },
  /** The backtest page on the start-view read: a project with no run, a finished run, a stopped run and a narrow window. */
  "entry-crew-backtest-page": { route: "#/settings/crew/backtest", scenario: "crew-backtest-none", wait: 2500 },
  "entry-crew-backtest-page-last-finished": { route: "#/settings/crew/backtest", scenario: "crew-backtest", wait: 2500 },
  "entry-crew-backtest-page-last-stopped": { route: "#/settings/crew/backtest", scenario: "crew-backtest-stopped", wait: 2500 },
  "entry-crew-backtest-page-running": { route: "#/settings/crew/backtest", scenario: "crew-backtest-running", wait: 2500 },
  "entry-crew-backtest-page-narrow-window": { route: "#/settings/crew/backtest", scenario: "crew-backtest-none", width: 640, wait: 2500 },
  /** The lanes and budgets states: reading, an error with Try again, a lane with no kind, an engine with no budget and a failed last run. */
  "backtest-lanes-reading": { route: "#/settings/crew/backtest", scenario: "crew-backtest", pre: [{ route: "**/api/crew/backtest/start-view*", delay: 20000 }], wait: 900 },
  "backtest-lanes-error": { route: "#/settings/crew/backtest", scenario: "crew-backtest", pre: [{ route: "**/api/crew/backtest/start-view*", status: 500 }] },
  "backtest-lanes-kind-not-recorded": { route: "#/settings/crew/backtest", scenario: "crew-backtest-kind-none" },
  "backtest-lanes-budget-not-set": { route: "#/settings/crew/backtest", scenario: "crew-backtest-budget-none" },
  "backtest-lanes-last-failed": { route: "#/settings/crew/backtest", scenario: "crew-backtest-failed" },
  "backtest-lanes-narrow-window": { route: "#/settings/crew/backtest", scenario: "crew-backtest", width: 640 },
  /** Crew, the escape check frames (Crew Designs v79): one escape's own page with its check in the state the mock leaves it. */
  "escape-check-proposed": { route: "#/preview/crew/escape-check-proposed?id=esc-proposed", scenario: "crew-escapes-checks" },
  "escape-check-writing": { route: "#/preview/crew/escape-check-writing?id=esc-writing", scenario: "crew-escapes-checks" },
  "escape-check-approve": { route: "#/preview/crew/escape-check-approve?id=esc-proof", scenario: "crew-escapes-checks" },
  "escape-check-proven": { route: "#/preview/crew/escape-check-proven?id=esc-proven", scenario: "crew-escapes-checks" },
  "escape-check-invalid": { route: "#/preview/crew/escape-check-invalid?id=esc-session", scenario: "crew-escapes-checks" },
  "escape-check-retry": { route: "#/preview/crew/escape-check-retry?id=esc-session", scenario: "crew-escapes-checks", steps: [click("button", "Retry")] },
  "escape-check-withdraw": { route: "#/preview/crew/escape-check-withdraw?id=esc-proven", scenario: "crew-escapes-checks", steps: [click("button", "Withdraw")] },
  "escape-check-withdrawn": { route: "#/preview/crew/escape-check-withdrawn?id=esc-withdrawn", scenario: "crew-escapes-checks" },
  "escape-limit": { route: "#/preview/crew/escape-limit", scenario: "crew-escapes-limit", wait: 2500 },
  /** The same log at a narrow width (Crew Designs v79): a 640 px window and the 320 px side panel, cards instead of the table. */
  "escape-narrow-window": { route: "#/preview/crew/escape-narrow-window", scenario: "crew-escapes", width: 640 },
  "escape-narrow-panel": { route: "#/preview/crew/escape-narrow-panel", scenario: "crew-escapes", width: 320 },
  /** The settings sheet on the real thresholds read (the defaults; `crew-escapes-stored` draws a person's stored values). */
  "escape-settings": { route: "#/preview/crew/escape-settings", scenario: "crew-escapes" },
  /** Drawn samples with no read behind them, matched to Crew Designs v79: the mark dialog (no Review queue to open it from),
   *  the dashboard (no report route) and the Route escape counts (no per-area counts route). */
  "escape-report": { route: "#/preview/crew/escape-report" },
  "escape-dashboard": { route: "#/preview/crew/escape-dashboard" },
  "escape-route-counts": { route: "#/preview/crew/escape-route-counts" },
  "escape-route-shifted": { route: "#/preview/crew/escape-route-shifted" },

  /** Crew, Settings › Review capacity (Crew Designs v77), on the real capacity and settings routes: the editor that
   *  Change the limit and Set a limit open, that editor in a 640 px window, and a save the service refused for want of
   *  the person key. The mock's crew-capacity scenarios answer the two reads and the Person-only settings write. */
  "capacity-settings-editor": { route: "#/preview/crew/capacity-live-settings", scenario: "crew-capacity", steps: [click("button", "Change the limit")] },
  "capacity-settings-editor-narrow": { route: "#/preview/crew/capacity-live-settings", scenario: "crew-capacity", width: 640, steps: [click("button", "Change the limit")] },
  "capacity-settings-editor-error": { route: "#/preview/crew/capacity-live-settings", scenario: "crew-capacity-save-fails", steps: [click("button", "Change the limit"), click("button", "Save")] },
  // Transcript export and import (frames transcript-*): the two menus, the export toasts, the import sheet's states,
  // its four refusals, and an imported thread. The mock's transcript-import scenarios carry a project default engine.
  "transcript-export-menu": { route: "#/threads/t-done", steps: [click("button", "Thread menu")] },
  "transcript-export-menu-sidebar": {
    route: "#/threads/t-done",
    steps: [{ do: "contextmenu", role: "link", name: "Explain how the event log resumes", prefix: true }],
  },
  "transcript-export-exporting": {
    route: "#/threads/t-done",
    pre: [{ route: "**/api/threads/t-done/export", delay: 20000 }],
    steps: [click("button", "Thread menu"), click("menuitem", "Export transcript")],
  },
  "transcript-export-saved": {
    route: "#/threads/t-done",
    steps: [click("button", "Thread menu"), click("menuitem", "Export transcript")],
  },
  "transcript-export-failed": {
    route: "#/threads/t-done",
    pre: [{ route: "**/api/threads/t-done/export", status: 500 }],
    steps: [click("button", "Thread menu"), click("menuitem", "Export transcript")],
  },
  "transcript-import-project-menu": {
    route: "#/threads/t-done",
    scenario: "transcript-import",
    steps: [{ do: "contextmenu", selector: ".folder-toggle", text: "orbyn-engineering-app" }],
  },
  "transcript-import-drop": {
    route: "#/threads/t-done",
    scenario: "transcript-import",
    steps: [
      {
        do: "drag",
        selector: ".folder-toggle",
        name: "Redirect loop on logout.orbyn.json",
        mime: "application/vnd.orbyn.transcript+json",
        text: "orbyn-engineering-app",
        content: transcriptBody(engineThread),
      },
    ],
  },
  "transcript-import-reading": {
    route: "#/threads/t-done",
    scenario: "transcript-import",
    pre: [{ route: "**/api/threads/import*", delay: 20000 }],
    steps: importSteps("Redirect loop on logout.orbyn.json", engineThread),
  },
  "transcript-import-preview": importRoute("Redirect loop on logout.orbyn.json", engineThread),
  "transcript-import-preview-dropped": importRoute("Redirect loop on logout dropped.orbyn.json", engineThread),
  "transcript-import-importing": importRoute("Redirect loop on logout.orbyn.json", engineThread, {}, [
    { do: "route", route: "**/api/threads/import*", delay: 20000 },
    click("button", "Import as new thread"),
    { do: "wait", ms: 300 },
  ]),
  "transcript-import-engine-menu": importRoute("Redirect loop on logout.orbyn.json", engineThread, {}, [
    click("combobox", "Engine"),
  ]),
  "transcript-import-preview-no-engine": importRoute("Redirect loop on logout.orbyn.json", { title: TRANSCRIPT_TITLE }),
  "transcript-import-error-too-large": importRoute("too large.orbyn.json", engineThread, {}, [], TOO_LARGE),
  "transcript-import-error-too-new": importRoute("newer.orbyn.json", engineThread),
  "transcript-import-error-unreadable": importRoute("notes.orbyn.json", engineThread),
  "transcript-import-error-key-missing": importRoute("nokey.orbyn.json", engineThread),
  "transcript-imported-thread": { route: "#/threads/t-imported", scenario: "transcript-imported" },

};
