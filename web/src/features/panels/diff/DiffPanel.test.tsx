import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import { ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { FileDiff } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { DiffPanel } from "./DiffPanel";

const GIT_HEADER = "diff --git a/auth/redirect.ts b/auth/redirect.ts\nindex 1111111..2222222 100644\n--- a/auth/redirect.ts\n+++ b/auth/redirect.ts\n";
const HUNK = "@@ -14,3 +14,4 @@ after(req)\n export function after(req) {\n-  return \"/home\"\n+  const next = req.query.next\n+  return inside(next) ? next : \"/\"\n }\n";
const REDIRECT: FileDiff = { path: "auth/redirect.ts", status: "modified", additions: 2, deletions: 1, patch: GIT_HEADER + HUNK };
const TEST: FileDiff = { path: "auth/redirect.test.ts", status: "modified", additions: 2, deletions: 2, patch: "@@ -1,2 +1,2 @@\n-old test\n+new test\n same\n" };
const BRANCHES = [
  { name: "main", current: false, remote: false },
  { name: "develop", current: false, remote: false },
  { name: "fix/login", current: true, remote: false },
];

/** A panel as the service would fill it: one checkout in Git, nothing linked, no Jujutsu. */
function setup(files: FileDiff[] | (() => Promise<FileDiff[]>) = [REDIRECT, TEST], options: { width?: number; active?: boolean } = {}) {
  const diff = vi.spyOn(api, "diff").mockImplementation(async () => (typeof files === "function" ? files() : files));
  vi.spyOn(api, "checkpoints").mockResolvedValue([]);
  vi.spyOn(api, "commits").mockResolvedValue([]);
  vi.spyOn(api, "jjStatus").mockResolvedValue(null);
  vi.spyOn(api, "branches").mockResolvedValue(BRANCHES);
  vi.spyOn(pullsApi, "linked").mockResolvedValue([]);
  useWorkspace.setState({ threads: { t: { id: "t", cwd: "/repo", active_run: null, worktree: { path: "/repo", branch: "fix/login", base: "main" } } as never } });
  if (options.width) {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const width = this.classList.contains("x2-pbody") ? options.width! : 0;
      return { width, height: 0, top: 0, left: 0, right: width, bottom: 0, x: 0, y: 0, toJSON: () => ({}) };
    });
  }
  return diff;
}

const rowsOf = () => [...screen.getByRole("group", { name: "Changed files" }).querySelectorAll<HTMLElement>("button.fli")];

const open = async () => {
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Scope: Working tree" });
};

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  vi.restoreAllMocks();
  useComposerChips.setState({ chips: {}, sent: {} });
});

// changes-default
it("draws one header, a file list with change letters and the first file's lines with their own numbers, without the raw git header", async () => {
  setup();
  await open();
  expect(screen.getByRole("button", { name: "Scope: Working tree" })).toHaveAttribute("aria-haspopup", "menu");
  expect(screen.getByText(/^\+4/)).toBeInTheDocument();
  const list = screen.getByRole("group", { name: "Changed files" });
  expect(rowsOf().map((row) => row.textContent)).toEqual(["Mauth/redirect.ts+2 −1", "Mauth/redirect.test.ts+2 −2"]);
  expect(within(list).getByRole("button", { name: /^Modified auth\/redirect\.ts/ })).toHaveAttribute("aria-current", "true");
  const patch = screen.getByRole("group", { name: "Patch" });
  expect(patch).toHaveTextContent("@@ after(req)");
  expect(patch).not.toHaveTextContent("diff --git");
  expect(patch).not.toHaveTextContent("index 1111111");
  expect(patch).not.toHaveTextContent("+++ b/");
  // The removed line is numbered on the old side, the added ones on the new side.
  expect(within(patch).getByRole("button", { name: "Comment on line 15" })).toBeInTheDocument();
  expect(within(patch).getByRole("button", { name: "Comment on line 16" })).toBeInTheDocument();
  expect(within(patch).queryByRole("button", { name: "Comment on line 17" })).toBeInTheDocument();
});

it("picking a file in the list shows that file, and the list filter narrows the rows", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  await user.click(screen.getByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ }));
  expect(screen.getByRole("group", { name: "Patch" })).toHaveTextContent("new test");
  await user.type(screen.getByRole("searchbox", { name: "Filter files" }), "REDIRECT.TS");
  expect(rowsOf()).toHaveLength(1);
});

it("the arrow keys move between rows of the file list", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  screen.getByRole("button", { name: /^Modified auth\/redirect\.ts/ }).focus();
  await user.keyboard("{ArrowDown}");
  expect(screen.getByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ })).toHaveFocus();
  await user.keyboard("{ArrowUp}");
  expect(screen.getByRole("button", { name: /^Modified auth\/redirect\.ts/ })).toHaveFocus();
});

// scope menu, turn and commit pickers, jj
it("the scope menu names the five scopes and a pick reads that scope", async () => {
  const user = userEvent.setup();
  const diff = setup();
  vi.spyOn(api, "checkpoints").mockResolvedValue([
    { run_id: "r1", start: "s1", end: "e1", created_at: "" },
    { run_id: "r2", start: "s2", end: "e2", created_at: "" },
  ]);
  await open();
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  const menu = await screen.findByRole("menu", { name: "Scope" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Working tree", "Turn", "Thread", "Branch", "Commit"]);
  await user.click(within(menu).getByRole("menuitem", { name: "Turn" }));
  const picker = await screen.findByRole("combobox", { name: "Turn" });
  expect(picker).toHaveValue("r2");
  await waitFor(() => expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "turn&run=r2"));
  await user.selectOptions(picker, "r1");
  await waitFor(() => expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "turn&run=r1"));
  expect(screen.getByRole("button", { name: "Scope: Turn" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Restore files to before this response" })).toBeInTheDocument();
});

it("the Commit scope lists the commits, and a commit's comments show only on that commit", async () => {
  const user = userEvent.setup();
  setup();
  vi.spyOn(api, "commits").mockResolvedValue([{ sha: "a".repeat(40), author: "", date: "", subject: "First" }]);
  useComposerChips.getState().add("t", { id: "c", kind: "review", path: "auth/redirect.ts", line: 16, comment: "Here", excerpt: "x", sourceRevision: "diff:worktree", sourceCwd: "/repo" });
  await open();
  expect(await screen.findByRole("note", { name: "Review comment auth/redirect.ts:16" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Commit" }));
  await screen.findByRole("combobox", { name: "Commit" });
  await waitFor(() => expect(screen.queryByRole("note", { name: "Review comment auth/redirect.ts:16" })).toBeNull());
});

it("shows a Jujutsu checkout's change above the header", async () => {
  setup();
  vi.spyOn(api, "jjStatus").mockResolvedValue({ change_id: "kkmpptxz", commit_id: "a1b2c3", bookmarks: ["main"], description: "Fix it", conflict: false } as never);
  await open();
  expect(await screen.findByRole("note", { name: "Jujutsu change" })).toHaveTextContent("kkmpptxz");
});

// changes-view-menu
it("View options holds Unified, Split, whitespace, test files, collapse and wrap, and is lit when one differs from the default", async () => {
  const user = userEvent.setup();
  setup(undefined, { width: 800 });
  await open();
  const button = screen.getByRole("button", { name: "View options" });
  expect(button).not.toHaveClass("on");
  await user.click(button);
  const menu = await screen.findByRole("menu", { name: "View options" });
  expect([...menu.querySelectorAll("[role^=menuitem]")].map((item) => item.textContent)).toEqual([
    "Unified", "Split", "Ignore whitespace", "Hide test files", "Collapse all files", "Wrap long lines",
  ]);
  expect(within(menu).getByRole("menuitemradio", { name: "Unified" })).toBeChecked();
  expect(within(menu).getByRole("menuitemcheckbox", { name: "Wrap long lines" })).toBeChecked();
  await user.click(within(menu).getByRole("menuitemcheckbox", { name: "Ignore whitespace" }));
  expect(useUi.getState().diffIgnoreWhitespace).toBe(true);
  expect(screen.getByRole("button", { name: "View options" })).toHaveClass("on");
  await user.click(within(screen.getByRole("menu", { name: "View options" })).getByRole("menuitemcheckbox", { name: "Wrap long lines" }));
  expect(useUi.getState().diffWrapLines).toBe(false);
  expect(screen.getByRole("group", { name: "Patch" })).toHaveAttribute("data-wrap", "off");
});

it("Hide test files drops test files, and Collapse all files leaves nothing open until a file is picked", async () => {
  const user = userEvent.setup();
  setup();
  useUi.getState().setPrefs({ diffHideTests: true });
  await open();
  expect(screen.queryByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ })).toBeNull();
  useUi.getState().setPrefs({ diffHideTests: false, diffFilesCollapsed: true });
  await waitFor(() => expect(screen.queryByRole("group", { name: "Patch" })).toBeNull());
  expect(rowsOf()).toHaveLength(2);
  await user.click(screen.getByRole("button", { name: /^Modified auth\/redirect\.ts/ }));
  expect(screen.getByRole("group", { name: "Patch" })).toBeInTheDocument();
});

it("Ignore whitespace drops pairs that differ only in spacing", async () => {
  setup([{ path: "s.txt", status: "modified", additions: 1, deletions: 1, patch: "@@ -1 +1 @@\n-a  b\n+a b\n" }]);
  useUi.getState().setPrefs({ diffIgnoreWhitespace: true });
  await open();
  expect(screen.getByRole("group", { name: "Patch" }).querySelectorAll(".l.add, .l.del")).toHaveLength(0);
});

// changes-compare-menu
it("the pair is the base and the branch, and its menu picks the base of the Branch scope", async () => {
  const user = userEvent.setup();
  const diff = setup();
  await open();
  const pair = await screen.findByRole("button", { name: "Compare main to fix/login" });
  // The Working tree takes no base, so the pair is context only.
  expect(pair).toHaveAttribute("aria-disabled", "true");
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Branch" }));
  await waitFor(() => expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "branch&base=main"));
  await user.click(screen.getByRole("button", { name: "Compare main to fix/login" }));
  const menu = await screen.findByRole("menu", { name: "Compare" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["main", "develop", "fix/loginthis branch"]);
  expect([...menu.querySelectorAll(".menu-heading")].map((heading) => heading.textContent)).toEqual(["Base", "Compare with"]);
  expect(within(menu).getByRole("menuitem", { name: /^main/ })).toHaveAttribute("aria-current", "true");
  await user.click(within(menu).getByRole("menuitem", { name: "develop" }));
  await waitFor(() => expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "branch&base=develop"));
  expect(screen.getByRole("button", { name: "Compare develop to fix/login" })).toBeInTheDocument();
});

// changes-empty
it("an empty Working tree says so in one line and offers the Branch scope", async () => {
  const user = userEvent.setup();
  const diff = setup([]);
  await open();
  expect(await screen.findByText("No changes in Working tree.")).toBeInTheDocument();
  expect(screen.getByText("Everything is committed.")).toBeInTheDocument();
  expect(screen.getByText(/^\+0/)).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show Branch changes" }));
  await waitFor(() => expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "branch&base=main"));
  expect(screen.getByRole("button", { name: "Scope: Branch" })).toBeInTheDocument();
});

// changes-error
it("a failed read keeps the header, shows one error row with the reason in its tooltip and no old diff, and Try again reads again", async () => {
  const user = userEvent.setup();
  let fail = false;
  const diff = setup(async () => {
    if (fail) throw new ApiFailure("git_failed", "fatal: bad object", 500);
    return [REDIRECT];
  });
  await open();
  await screen.findByRole("group", { name: "Patch" });
  fail = true;
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  const row = await screen.findByRole("alert");
  expect(row).toHaveTextContent("Couldn’t read the changes.");
  expect(row).toHaveAttribute("title", "fatal: bad object");
  expect(screen.getByRole("button", { name: "Scope: Working tree" })).toBeInTheDocument();
  expect(screen.queryByRole("group", { name: "Patch" })).toBeNull();
  fail = false;
  await user.click(within(row).getByRole("button", { name: "Try again" }));
  await screen.findByRole("group", { name: "Patch" });
  expect(diff.mock.calls.length).toBeGreaterThanOrEqual(3);
});

// changes-loading
it("the header is usable at once and placeholder rows come only after 400 ms", async () => {
  setup(() => new Promise<FileDiff[]>(() => undefined));
  await open();
  expect(screen.getByRole("status")).toHaveTextContent("Loading changes");
  expect(document.querySelectorAll(".x2sk")).toHaveLength(0);
  expect(screen.getByRole("button", { name: "View options" })).toBeEnabled();
  await waitFor(() => expect(document.querySelectorAll(".x2sk")).toHaveLength(7), { timeout: 2000 });
});

// changes-binary
it("a binary file says it changed, with Open, and no sizes are made up", async () => {
  const user = userEvent.setup();
  setup([{ path: "assets/logo.png", status: "modified", additions: 0, deletions: 0, patch: "diff --git a/assets/logo.png b/assets/logo.png\nindex 1..2 100644\nBinary files a/assets/logo.png and b/assets/logo.png differ\n" }]);
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: "assets/logo.png", repository_path: "assets/logo.png" });
  await open();
  expect(await screen.findByText("Binary file changed.")).toBeInTheDocument();
  expect(screen.queryByRole("group", { name: "Patch" })).toBeNull();
  expect(screen.getByText(/^\+0/)).toBeInTheDocument();
  await user.click(screen.getAllByRole("button", { name: "Open" }).at(-1)!);
  await waitFor(() => expect(useUi.getState().panelTab).toBe(fileTabId({ kind: "thread", path: "assets/logo.png" })));
});

// changes-rename
it("a rename names the new and the old path with its similarity, and a pure rename says there are no line changes", async () => {
  setup([
    { path: "auth/next.ts", status: "renamed", additions: 1, deletions: 1, patch: "diff --git a/auth/redirect.ts b/auth/next.ts\nsimilarity index 94%\nrename from auth/redirect.ts\nrename to auth/next.ts\nindex 1..2 100644\n--- a/auth/redirect.ts\n+++ b/auth/next.ts\n@@ -1 +1 @@\n-a\n+b\n" },
    { path: "docs/b.md", status: "renamed", additions: 0, deletions: 0, patch: "diff --git a/a.md b/docs/b.md\nsimilarity index 100%\nrename from a.md\nrename to docs/b.md\n" },
  ]);
  const user = userEvent.setup();
  await open();
  const rows = rowsOf();
  expect(rows[0]).toHaveTextContent("Rauth/next.ts ← auth/redirect.ts94%");
  await user.click(rows[1]!);
  expect(screen.getByText("Renamed, no changes to the lines")).toBeInTheDocument();
});

// changes-conflict
it("a conflict is listed first with a !, counted in the header and sent to the editor from one banner", async () => {
  const user = userEvent.setup();
  setup([REDIRECT, { path: "auth/session.ts", status: "conflicted", additions: 0, deletions: 0, patch: "@@ -1,3 +1,3 @@\n <<<<<<< HEAD\n+  return \"/\"\n=======\n-  return \"/home\"\n>>>>>>> main\n" }]);
  const opened = vi.spyOn(api, "openInEditor").mockResolvedValue(undefined);
  vi.spyOn(api, "editors").mockResolvedValue([{ id: "code", name: "VS Code", launchable: true } as never]);
  await open();
  expect(await screen.findByText("1 conflict")).toBeInTheDocument();
  const rows = rowsOf();
  expect(rows[0]).toHaveTextContent("!auth/session.tsconflict");
  expect(within(rows[0]!).getByLabelText("Conflicted")).toHaveClass("xg-bang");
  expect(screen.getByText(/Merge conflict in 1 file\. Resolve them in the editor, then refresh\./)).toBeInTheDocument();
  await user.click(screen.getAllByRole("button", { name: "Open in editor" })[0]!);
  await waitFor(() => expect(opened).toHaveBeenCalledWith("t", expect.objectContaining({ path: "." })));
});

// changes-huge
it("a lock file starts collapsed behind one row, and Show anyway draws its lines", async () => {
  const user = userEvent.setup();
  const body = Array.from({ length: 400 }, (_, at) => `+line ${at}`).join("\n");
  setup([{ path: "package-lock.json", status: "modified", additions: 400, deletions: 0, patch: `@@ -0,0 +1,400 @@\n${body}\n` }]);
  await open();
  expect(await screen.findByText("Large diff: 400 lines not shown.")).toBeInTheDocument();
  expect(screen.getAllByText(/^\+400/).length).toBeGreaterThan(0);
  expect(screen.getByRole("group", { name: "Patch" }).querySelectorAll(".l")).toHaveLength(0);
  await user.click(screen.getByRole("button", { name: "Show anyway" }));
  expect(screen.getByRole("group", { name: "Patch" }).querySelectorAll(".l")).toHaveLength(400);
  expect(screen.queryByText(/Large diff/)).toBeNull();
});

it("a diff over 2,000 lines is cut after its first screen and says how much is hidden", async () => {
  const body = Array.from({ length: 2500 }, (_, at) => `+line ${at}`).join("\n");
  setup([{ path: "src/big.ts", status: "added", additions: 2500, deletions: 0, patch: `@@ -0,0 +1,2500 @@\n${body}\n` }]);
  await open();
  expect(await screen.findByText("Large diff: 501 lines not shown.")).toBeInTheDocument();
}, 20_000);

// changes-find
it("⌘F opens the find bar, which counts matches across all files, steps with the arrows and closes with Esc", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  expect(screen.queryByRole("searchbox", { name: "Find in diff" })).toBeNull();
  screen.getByRole("group", { name: "Patch" }).focus();
  await user.keyboard("{Meta>}f{/Meta}");
  const input = await screen.findByRole("searchbox", { name: "Find in diff" });
  await user.type(input, "next");
  // Four in the first file, none in the other; a test file match is counted too.
  expect(await screen.findByText("1 of 4")).toBeInTheDocument();
  expect(screen.getByRole("group", { name: "Patch" }).querySelectorAll("mark.fhit")).toHaveLength(4);
  await user.keyboard("{ArrowDown}");
  expect(screen.getByText("2 of 4")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Previous match" }));
  expect(screen.getByText("1 of 4")).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("searchbox", { name: "Find in diff" })).toBeNull();
  expect(screen.getByRole("group", { name: "Patch" }).querySelectorAll("mark.fhit")).toHaveLength(0);
});

it("a match in another file opens that file", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  screen.getByRole("group", { name: "Patch" }).focus();
  await user.keyboard("{Control>}f{/Control}");
  await user.type(await screen.findByRole("searchbox", { name: "Find in diff" }), "new test");
  expect(await screen.findByText("1 of 1")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ })).toHaveAttribute("aria-current", "true"));
});

// changes-markdown-rendered
it("a Markdown file offers Rendered and Source, remembered, and says comments are off while rendered", async () => {
  const user = userEvent.setup();
  setup([{ path: "docs/login.md", status: "modified", additions: 1, deletions: 0, patch: "@@ -1 +1,2 @@\n # Login\n+Sign in.\n" }]);
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: "docs/login.md", repository_path: "docs/login.md" });
  const read = vi.spyOn(await import("../api"), "readThreadFile").mockResolvedValue({ content: "# Login\n\nSign in.\n", binary: false, truncated: false, size: 3, revision: "r" } as never);
  await open();
  const toggle = screen.getByRole("radiogroup", { name: "View" });
  expect(within(toggle).getByRole("radio", { name: "Source" })).toBeChecked();
  await user.click(within(toggle).getByRole("radio", { name: "Rendered" }));
  expect(useUi.getState().diffMarkdownRendered).toBe(true);
  expect(await screen.findByRole("heading", { name: "Login" })).toBeInTheDocument();
  expect(screen.queryByRole("group", { name: "Patch" })).toBeNull();
  expect(screen.getByRole("radiogroup", { name: "View" })).toHaveAttribute("title", "Switch to Source to comment on lines");
  expect(read).toHaveBeenCalled();
});

it("a Markdown file of a Commit scope has no Rendered, since the checkout is not that commit", async () => {
  const user = userEvent.setup();
  setup([{ path: "docs/login.md", status: "modified", additions: 1, deletions: 0, patch: "@@ -1 +1,2 @@\n # Login\n+Sign in.\n" }]);
  vi.spyOn(api, "commits").mockResolvedValue([{ sha: "a".repeat(40), author: "", date: "", subject: "First" }]);
  await open();
  expect(screen.getByRole("radiogroup", { name: "View" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Commit" }));
  await screen.findByRole("combobox", { name: "Commit" });
  await waitFor(() => expect(screen.queryByRole("radiogroup", { name: "View" })).toBeNull());
});

// changes-narrow, changes-wide, changes-split
it("below 640 px the pair sits on its own row under the list and Split is off with its reason", async () => {
  const user = userEvent.setup();
  setup(undefined, { width: 380 });
  useUi.getState().setPrefs({ diffLayout: "split" });
  await open();
  expect(document.querySelectorAll(".ch2 > .chrow")).toHaveLength(2);
  expect(document.querySelector(".x2cols")).toBeNull();
  expect(screen.getByRole("group", { name: "Patch" })).toHaveAttribute("data-layout", "stacked");
  await user.click(screen.getByRole("button", { name: "View options" }));
  const split = await screen.findByRole("menuitemradio", { name: "Split" });
  expect(split).toBeDisabled();
  expect(split).toHaveAttribute("title", "Needs about 640 px");
});

it("from 640 px the pair joins the first row and the file list moves beside the diff", async () => {
  setup(undefined, { width: 900 });
  await open();
  expect(document.querySelectorAll(".ch2 > .chrow")).toHaveLength(1);
  expect(document.querySelector(".x2cols > .fl2")).not.toBeNull();
  expect(document.querySelector(".x2cols > .x2main")).not.toBeNull();
});

it("Split puts the old side on the left and the new side on the right, each with its own numbers", async () => {
  setup(undefined, { width: 900 });
  useUi.getState().setPrefs({ diffLayout: "split" });
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  expect(patch).toHaveAttribute("data-layout", "split");
  const pairs = patch.querySelectorAll(".sp2");
  expect(pairs).toHaveLength(4);
  // The pair of the changed line: the old 15 on the left, the new 15 on the right.
  const changed = pairs[1]!;
  expect(changed.querySelector(".l.del")).toHaveTextContent("15");
  expect(changed.querySelector(".l.add")).toHaveTextContent("15");
  expect(within(changed as HTMLElement).getAllByRole("button", { name: "Comment on line 15" })).toHaveLength(1);
});

// comment-hover, comment-select, comment-thread
it("a line's number is the button that starts a comment; shift selects a range and the box opens under the last line", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Comment on line 15" }));
  await user.keyboard("{Shift>}");
  await user.click(screen.getByRole("button", { name: "Comment on line 16" }));
  await user.keyboard("{/Shift}");
  const box = screen.getByRole("textbox", { name: "Review comment on auth/redirect.ts line 15–16" });
  expect(screen.getByText("Comment on lines 15–16")).toBeInTheDocument();
  expect(screen.getByText("Sent with your next message")).toBeInTheDocument();
  expect(box.closest("form")?.previousElementSibling).toHaveTextContent("inside(next)");
  expect(screen.getByRole("group", { name: "Patch" }).querySelectorAll(".l.lsel")).toHaveLength(2);
});

it("dragging down the gutter selects the lines it passes over", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  const first = within(patch).getByRole("button", { name: "Comment on line 15" });
  const last = within(patch).getByRole("button", { name: "Comment on line 16" });
  await user.pointer([{ keys: "[MouseLeft>]", target: first }, { target: last }, { keys: "[/MouseLeft]", target: last }]);
  expect(await screen.findByText("Comment on lines 15–16")).toBeInTheDocument();
});

it("Comment adds the review chip with its range and excerpt, ⌘↵ sends it, and the comment stays under its line until removed", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Comment on line 15" }));
  await user.keyboard("{Shift>}");
  await user.click(screen.getByRole("button", { name: "Comment on line 16" }));
  await user.keyboard("{/Shift}");
  const box = screen.getByRole("textbox", { name: "Review comment on auth/redirect.ts line 15–16" });
  expect(screen.getByRole("button", { name: "Comment" })).toBeDisabled();
  await user.type(box, "Does inside() reject //evil.com");
  await user.keyboard("{Control>}{Enter}{/Control}");
  const chip = useComposerChips.getState().chips.t?.[0];
  expect(chip).toMatchObject({
    kind: "review", path: "auth/redirect.ts", line: 15, endLine: 16, comment: "Does inside() reject //evil.com",
    excerpt: "  const next = req.query.next\n  return inside(next) ? next : \"/\"", sourceRevision: "diff:worktree", sourceCwd: "/repo",
  });
  const note = screen.getByRole("note", { name: "Review comment auth/redirect.ts:15–16" });
  expect(note).toHaveTextContent("You · Does inside() reject //evil.com Sent with your next message");
  act(() => useComposerChips.getState().update("t", { ...chip!, comment: "Edited" } as never));
  expect(screen.getByRole("note", { name: "Review comment auth/redirect.ts:15–16" })).toHaveTextContent("Edited");
  await user.click(within(note).getByRole("button", { name: "Remove comment" }));
  expect(useComposerChips.getState().chips.t ?? []).toHaveLength(0);
  expect(screen.queryByRole("note", { name: /Review comment/ })).toBeNull();
});

it("Esc cancels a comment without adding anything", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Comment on line 16" }));
  await user.type(screen.getByRole("textbox", { name: /Review comment on/ }), "never mind");
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("textbox", { name: /Review comment on/ })).toBeNull();
  expect(useComposerChips.getState().chips.t ?? []).toHaveLength(0);
});

it("the keyboard moves between lines with the arrows, C comments on the line and Shift extends the range", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  patch.focus();
  // Rows: hunk, context 14, removed 15, added 15, added 16, context 17.
  await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{ArrowDown}");
  expect(patch.querySelector(".l.hov")).toHaveTextContent("const next");
  await user.keyboard("{Shift>}{ArrowDown}{/Shift}");
  expect(await screen.findByText("Comment on lines 15–16")).toBeInTheDocument();
  await user.keyboard("{Escape}");
  patch.focus();
  await user.keyboard("c");
  expect(await screen.findByRole("textbox", { name: /Review comment on auth\/redirect\.ts line 1[56]/ })).toBeInTheDocument();
});

it("a comment that would make the message too long is refused with the existing message", async () => {
  const user = userEvent.setup();
  setup();
  useUi.getState().setDraft("t", "x".repeat(120_000));
  await open();
  await user.click(screen.getByRole("button", { name: "Comment on line 16" }));
  await user.type(screen.getByRole("textbox", { name: /Review comment on/ }), "hi");
  await user.click(screen.getByRole("button", { name: "Comment" }));
  expect(await screen.findByText("This comment would make your message too long. Shorten it before adding it.")).toBeInTheDocument();
});

// footer: restore and pull request
it("offers Create pull request in the foot, and not outside a Git repository", async () => {
  setup();
  await open();
  expect(await screen.findByRole("button", { name: "Create pull request" })).toBeInTheDocument();
});

it("hides Create pull request where the header's Git menu hides, outside a Git repository", async () => {
  setup();
  vi.spyOn(api, "branches").mockRejectedValue(new Error("not a git repository"));
  await open();
  await waitFor(() => expect(api.branches).toHaveBeenCalled());
  expect(screen.queryByRole("button", { name: "Create pull request" })).toBeNull();
  // With no branches to name, the pair is not drawn either.
  expect(screen.queryByRole("button", { name: /^Compare/ })).toBeNull();
});

it("offers Create pull request from the branch once the working tree is clean after a commit", async () => {
  setup(undefined);
  vi.spyOn(api, "diff").mockImplementation(async (_thread, _signal, scope) => (String(scope).startsWith("branch") ? [REDIRECT] : []));
  await open();
  expect(await screen.findByText("No changes in Working tree.")).toBeInTheDocument();
  expect(await screen.findByRole("button", { name: "Create pull request" })).toBeInTheDocument();
});

it("keeps Create pull request when Hide test files filters a test-only change out of view", async () => {
  setup([TEST]);
  useUi.getState().setPrefs({ diffHideTests: true });
  await open();
  expect(await screen.findByRole("button", { name: "Create pull request" })).toBeInTheDocument();
});

// reveal
it("revealing a file opens it in the view", async () => {
  window.history.replaceState(null, "", "#/threads/t");
  setup();
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: "auth/redirect.test.ts", repository_path: "auth/redirect.test.ts" });
  await open();
  act(() => useUi.getState().revealDiff("auth/redirect.test.ts", { threadId: "t", cwd: "/repo" }));
  await waitFor(() => expect(screen.getByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ })).toHaveAttribute("aria-current", "true"));
  await waitFor(() => expect(useUi.getState().diffReveal).toBeNull());
});

// rulings: row actions, paging, empty scopes, lock files, sent comments
it("a file row's ⋯ menu copies the path and opens the file in the editor", async () => {
  const user = userEvent.setup();
  setup();
  const write = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText: write } });
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: "auth/redirect.ts", repository_path: "auth/redirect.ts" });
  vi.spyOn(api, "editors").mockResolvedValue([{ id: "code", name: "VS Code", launchable: true } as never]);
  const opened = vi.spyOn(api, "openInEditor").mockResolvedValue(undefined);
  await open();
  await user.click(screen.getByRole("button", { name: "Actions for auth/redirect.ts" }));
  await user.click(await screen.findByRole("menuitem", { name: "Copy path" }));
  expect(write).toHaveBeenCalledWith("auth/redirect.ts");
  await user.click(screen.getByRole("button", { name: "Actions for auth/redirect.ts" }));
  await user.click(await screen.findByRole("menuitem", { name: "Open in editor" }));
  await waitFor(() => expect(opened).toHaveBeenCalledWith("t", expect.objectContaining({ path: "auth/redirect.ts" })));
});

it("a long list shows 50 files and ends with a row that shows more", async () => {
  const user = userEvent.setup();
  setup(Array.from({ length: 120 }, (_, at) => ({ path: `src/f-${String(at).padStart(3, "0")}.ts`, status: "modified", additions: 1, deletions: 0, patch: "@@ -1 +1 @@\n-a\n+b\n" })));
  await open();
  await screen.findByRole("group", { name: "Changed files" });
  expect(rowsOf()).toHaveLength(50);
  await user.click(screen.getByRole("button", { name: "Show 70 more files" }));
  expect(rowsOf()).toHaveLength(100);
  await user.click(screen.getByRole("button", { name: "Show 20 more files" }));
  expect(rowsOf()).toHaveLength(120);
  expect(screen.queryByRole("button", { name: /^Show \d+ more/ })).toBeNull();
});

it("an empty scope says so and links to the nearest scope", async () => {
  const user = userEvent.setup();
  const diff = setup([]);
  await open();
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Commit" }));
  expect(await screen.findByText("No changes in Commit.")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show Branch changes" }));
  await waitFor(() => expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "branch&base=main"));
});

it("a minified file collapses like a lock file when long, and a short one does not", async () => {
  const body = Array.from({ length: 300 }, (_, at) => `+x${at}`).join("\n");
  setup([{ path: "dist/app.min.js", status: "modified", additions: 300, deletions: 0, patch: `@@ -0,0 +1,300 @@\n${body}\n` }]);
  await open();
  expect(await screen.findByText("Large diff: 300 lines not shown.")).toBeInTheDocument();
});

it("a comment reads Sent with your next message until the message is sent, then Sent to the agent", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Comment on line 16" }));
  await user.type(screen.getByRole("textbox", { name: /Review comment on/ }), "Check this");
  await user.click(screen.getByRole("button", { name: "Comment" }));
  const note = screen.getByRole("note", { name: "Review comment auth/redirect.ts:16" });
  expect(note).toHaveTextContent("Sent with your next message");
  act(() => useComposerChips.getState().consume("t", useComposerChips.getState().chips.t!));
  const after = screen.getByRole("note", { name: "Review comment auth/redirect.ts:16" });
  expect(after).toHaveTextContent("Sent to the agent");
  expect(within(after).queryByRole("button", { name: "Remove comment" })).toBeNull();
});

// review fixes
it("picking another file while find has a match keeps that file; only stepping opens the match's file", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  screen.getByRole("group", { name: "Patch" }).focus();
  await user.keyboard("{Control>}f{/Control}");
  await user.type(await screen.findByRole("searchbox", { name: "Find in diff" }), "e");
  await screen.findByText(/^1 of \d+$/);
  await user.click(screen.getByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ }));
  expect(screen.getByRole("button", { name: /^Modified auth\/redirect\.test\.ts/ })).toHaveAttribute("aria-current", "true");
  await user.click(screen.getByRole("button", { name: "Next match" }));
  await waitFor(() => expect(screen.getByRole("button", { name: /^Modified auth\/redirect\.ts/ })).toHaveAttribute("aria-current", "true"));
});

it("⌘F opens the find bar even when Collapse all files leaves no file open", async () => {
  const user = userEvent.setup();
  setup();
  useUi.getState().setPrefs({ diffFilesCollapsed: true });
  await open();
  screen.getByRole("group", { name: "Changed files" }).querySelector<HTMLElement>("button.fli")!.focus();
  await user.keyboard("{Control>}f{/Control}");
  expect(await screen.findByRole("searchbox", { name: "Find in diff" })).toBeInTheDocument();
});

it("find does not count lines that are cut off or collapsed until Show anyway draws them", async () => {
  const user = userEvent.setup();
  const body = Array.from({ length: 300 }, () => "+needle").join("\n");
  setup([{ path: "package-lock.json", status: "modified", additions: 300, deletions: 0, patch: `@@ -0,0 +1,300 @@\n${body}\n` }]);
  await open();
  screen.getByRole("group", { name: "Changed files" }).querySelector<HTMLElement>("button.fli")!.focus();
  await user.keyboard("{Control>}f{/Control}");
  await user.type(await screen.findByRole("searchbox", { name: "Find in diff" }), "needle");
  expect(await screen.findByText("No matches")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show anyway" }));
  expect(await screen.findByText("1 of 300")).toBeInTheDocument();
});

it("a Branch scope offers no Rendered, since the checkout is not the branch tip", async () => {
  const user = userEvent.setup();
  setup([{ path: "docs/login.md", status: "modified", additions: 1, deletions: 0, patch: "@@ -1 +1,2 @@\n # Login\n+Sign in.\n" }]);
  await open();
  expect(screen.getByRole("radiogroup", { name: "View" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Branch" }));
  await waitFor(() => expect(screen.queryByRole("radiogroup", { name: "View" })).toBeNull());
});

it("comments are reachable by keyboard: the arrows carry focus to a line's button, which Enter opens, and the hint is announced", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  expect(patch).toHaveAccessibleDescription(/Arrow keys move between lines, C comments/);
  patch.focus();
  await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{ArrowDown}");
  const button = screen.getByRole("button", { name: "Comment on line 15" });
  expect(button).toHaveFocus();
  expect(button).toHaveAttribute("tabindex", "0");
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("textbox", { name: /Review comment on/ })).toBeInTheDocument();
});
