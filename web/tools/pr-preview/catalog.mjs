// DEVELOPMENT ONLY. The screens a pull request can name in its "Frames built" line, and how the mock app is driven to each one.
//
// An id is the name of a screen (the design's frame id where the screen has one). Each entry says where to go and what to do there:
//   route     the hash route to open ("#/pulls")
//   scenario  the mock API's scenario (web/mock/api.ts); "default" when omitted
//   pre       network behaviour set up before the page opens: { route, delay } holds a request for `delay` ms, { route, status } answers it
//   steps     what to do once it has loaded, in order:
//               { do: "click", role, name, prefix?, exact?, within? }   a button, tab, checkbox ... by its accessible name (`within`: only inside the region of that name)
//               { do: "fill", label, value, exact? }          a field by its label (exact when another label contains it)
//               { do: "press", key }
//               { do: "route", route, delay | status }        network behaviour from here on
//               { do: "offline" }                             the device loses its network
//               { do: "wait", ms }                            let time pass here, for a state that arrives after a step
//   width     the window width in px when the screen is drawn narrow (default 1400)
//   wait      ms to wait after load, for states that show only after a delay (default 1500)
// Only data and a few verbs: nothing here imports the app. Add a screen by adding an entry, and its id to docs/match-kit.md.

const pull = (n) => `#/pulls/p-app/${n}`;
const click = (role, name, more = {}) => ({ do: "click", role, name, ...more });
const note = [{ do: "fill", label: "What needs to change?", value: "Should this reject //evil.com? The check also accepts //evil.com/x." }];
const send = click("button", "Request changes", { within: "Request changes" });
const filterNeedsMyReview = [
  click("button", "Filters", { prefix: true }),
  click("checkbox", "Needs my review"),
  { do: "press", key: "Escape" },
];

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

export const FRAMES = {
  // The shell.
  "shell-home": { route: "#/" },
  "shell-new-thread": { route: "#/new" },
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
  "thread-interrupted": { route: "#/threads/t-interrupted" },
  "thread-empty": { route: "#/threads/t-empty" },
  "thread-tools": { route: "#/threads/t-tools" },
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
};
