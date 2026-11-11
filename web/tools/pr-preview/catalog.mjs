// DEVELOPMENT ONLY. The screens a pull request can name in its "Frames built" line, and how the mock app is driven to each one.
//
// An id is the name of a screen (the design's frame id where the screen has one). Each entry says where to go and what to do there:
//   route     the hash route to open ("#/pulls")
//   scenario  the mock API's scenario (web/mock/api.ts); "default" when omitted
//   pre       network behaviour set up before the page opens: { route, delay } holds a request for `delay` ms, { route, status } answers it
//   steps     what to do once it has loaded, in order:
//               { do: "click", role, name, prefix?, exact? }   a button, tab, checkbox ... by its accessible name
//               { do: "fill", label, value }                  a field by its label
//               { do: "press", key }
//               { do: "route", route, delay | status }        network behaviour from here on
//               { do: "offline" }                             the device loses its network
//   wait      ms to wait after load, for states that show only after a delay (default 1500)
// Only data and a few verbs: nothing here imports the app. Add a screen by adding an entry, and its id to docs/match-kit.md.

const pull = (n) => `#/pulls/p-app/${n}`;
const click = (role, name, more = {}) => ({ do: "click", role, name, ...more });
const filterNeedsMyReview = [
  click("button", "Filters", { prefix: true }),
  click("checkbox", "Needs my review"),
  { do: "press", key: "Escape" },
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

  // Threads: run states, the tool call states, the details card.
  "thread-approval": { route: "#/threads/t-approval" },
  "thread-done": { route: "#/threads/t-done" },
  "thread-failed": { route: "#/threads/t-failed" },
  "thread-interrupted": { route: "#/threads/t-interrupted" },
  "thread-empty": { route: "#/threads/t-empty" },
  "thread-tools": { route: "#/threads/t-tools" },
  "thread-tools-live": { route: "#/threads/t-tools-live" },
  "thread-details": { route: "#/threads/t-done", steps: [click("button", "Details")] },

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
  "acct-signup-email-code": {
    route: "#/settings/account",
    scenario: "account",
    steps: [
      click("button", "Create account"),
      { do: "fill", label: "Invite code", value: "K7QM2XWH9QXA3C2D" },
      click("button", "Continue"),
      { do: "fill", label: "Email", value: "sam@example.com" },
      { do: "fill", label: "Password", value: "a throwaway phrase" },
      click("button", "Continue with a password"),
    ],
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
};
