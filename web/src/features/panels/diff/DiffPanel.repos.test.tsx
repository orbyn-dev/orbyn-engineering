import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../../mock/repos";
import { api, ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { FileDiff } from "../../../api/types";
import { chipLabel, chipPrompt } from "../../../product/composer/composerContext";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { chooseRepo, setReposApi, type RepoRow, type ThreadRepo } from "../../repos";
import * as panelsApi from "../api";
import { DiffPanel } from "./DiffPanel";

// F70: the commit and branch pickers act on the repository chosen for the thread, and a refused hunk write offers the trust prompt.
const FILE: FileDiff = { path: "auth/redirect.ts", status: "modified", additions: 2, deletions: 1,
  patch: "diff --git a/auth/redirect.ts b/auth/redirect.ts\nindex 1111111..2222222 100644\n--- a/auth/redirect.ts\n+++ b/auth/redirect.ts\n@@ -14,3 +14,4 @@ after(req)\n export function after(req) {\n-  return \"/home\"\n+  const next = req.query.next\n+  return inside(next) ? next : \"/\"\n }\n" };
const DISK = `${"// filler\n".repeat(13)}export function after(req) {\n  const next = req.query.next\n  return inside(next) ? next : "/"\n}\n`;
const repos: ThreadRepo[] = [
  { repo_id: "p", name: "app", branch: "fix/login", base_branch: "main", added: 1, deleted: 0, trusted: true, files: ["auth/redirect.ts"] },
  { repo_id: "planner", name: "orbyn-planner", branch: "fix/plan", base_branch: "develop", added: 2, deleted: 1, trusted: true, files: ["auth/redirect.ts"] },
];
const row = (id: string, over: Partial<RepoRow> = {}): RepoRow => ({ id, name: id === "p" ? "app" : "orbyn-planner", path: `~/Github/${id}`, is_git: true, trusted: true,
  branch: "main", branches: ["main"], has_remote: false, primary: id === "p", clone: null, ...over });
const commit = (sha: string) => ({ sha, subject: `subject ${sha.slice(0, 3)}`, author: "Anh Dang", date: "2026-11-20T10:00:00Z" });

/** `only` is the repository whose read answers the files; every other repository's read is empty. */
function setup(files: FileDiff[] = [FILE], only?: string) {
  const diff = vi.spyOn(api, "diff").mockImplementation(async (_t, _s, _scope, repoId) => (only === undefined || repoId === only ? files : []));
  vi.spyOn(api, "checkpoints").mockResolvedValue([]);
  const commits = vi.spyOn(api, "commits").mockImplementation(async (_t, _s, repoId) => (repoId === "planner" ? [commit("bbb2222")] : [commit("aaa1111")]));
  vi.spyOn(api, "jjStatus").mockResolvedValue(null);
  const branches = vi.spyOn(api, "branches").mockImplementation(async (_t, _s, repoId) => [{ name: repoId === "planner" ? "fix/plan" : "fix/login", current: true, remote: false }, { name: repoId === "planner" ? "develop" : "main", current: false, remote: false }] as never);
  vi.spyOn(pullsApi, "linked").mockResolvedValue([]);
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: "auth/redirect.ts", repository_path: "auth/redirect.ts" });
  const read = vi.spyOn(panelsApi, "readThreadFile").mockResolvedValue({ path: "auth/redirect.ts", content: DISK, revision: "r1", size: DISK.length, binary: false, truncated: false });
  const save = vi.spyOn(panelsApi, "saveThreadFile").mockResolvedValue({ revision: "r2" } as never);
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/repo", active_run: null, worktree: { path: "/repo", branch: "fix/login", base: "main" } } as never } });
  return { diff, commits, branches, read, save };
}

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) { this.setAttribute("open", ""); };
});
afterEach(() => { chooseRepo("t", null); setReposApi(null); vi.restoreAllMocks(); });

const toCommitScope = async (user: ReturnType<typeof userEvent.setup>) => {
  await screen.findByRole("button", { name: "Scope: Working tree" });
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Commit" }));
};

it("the commit picker of a multi-repository thread lists the primary's commits first, then the chosen repository's, with repo_id on the query", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { commits, diff } = setup();
  render(<DiffPanel threadId="t" />);
  await toCommitScope(user);
  expect(await screen.findByRole("button", { name: "Repository: app" })).toBeVisible();
  await waitFor(() => expect(commits).toHaveBeenCalled());
  expect(commits.mock.calls.every((call) => call[2] === undefined)).toBe(true);
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "commit&sha=aaa1111")).toBe(true));

  await user.click(screen.getByRole("button", { name: "Repository: app" }));
  await user.click(await screen.findByRole("menuitem", { name: "orbyn-planner" }));
  await waitFor(() => expect(commits.mock.calls.some((call) => call[2] === "planner")).toBe(true));
  // A commit is one repository's: its diff is read in that repository only, with that repository's repo_id.
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "commit&sha=bbb2222" && call[3] === "planner")).toBe(true));
  expect(diff.mock.calls.some((call) => call[2] === "commit&sha=bbb2222" && call[3] !== "planner")).toBe(false);
});

it("the base picker of the branch comparison offers the chosen repository's branches", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches } = setup();
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Scope: Working tree" });
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Branch" }));
  await user.click(await screen.findByRole("button", { name: "Repository: app" }));
  await user.click(await screen.findByRole("menuitem", { name: "orbyn-planner" }));
  await waitFor(() => expect(branches.mock.calls.some((call) => call[2] === "planner")).toBe(true));
});

it("an ordinary thread shows no chooser in the commit scope and sends no repo_id", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: {} }));
  const { commits, branches } = setup();
  render(<DiffPanel threadId="t" />);
  await toCommitScope(user);
  await waitFor(() => expect(commits).toHaveBeenCalled());
  expect(screen.queryByRole("button", { name: /^Repository:/ })).toBeNull();
  expect(commits.mock.calls.every((call) => call.length === 2)).toBe(true);
  expect(branches.mock.calls.every((call) => call.length === 2)).toBe(true);
});

// Trust from a refused hunk write.
const revertHunk = async (user: ReturnType<typeof userEvent.setup>, grouped = true) => {
  // The repositories load first and the panel reads the diff again with them; revert only once the patch of the final read shows.
  if (grouped) await screen.findByRole("heading", { name: /orbyn-planner/ });
  await screen.findByRole("group", { name: "Patch" });
  await user.click(await screen.findByRole("button", { name: "Revert hunk" }));
  await user.click(within(await screen.findByRole("dialog", { hidden: true })).getByRole("button", { name: "Revert hunk" }));
};

it("a 409 repo_untrusted on a hunk revert offers the trust dialog; trusting is one write and the revert is not repeated", async () => {
  const user = userEvent.setup();
  const mock = createReposMock({ threads: { t: repos }, repositories: [row("p"), row("planner", { trusted: false })] });
  setReposApi(mock);
  const { save } = setup([FILE], "planner");
  save.mockRejectedValue(new ApiFailure("repo_untrusted", "Trust this repository before using it.", 409));
  render(<DiffPanel threadId="t" />);
  await revertHunk(user);
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Trust this repository before using it. Nothing was changed.");
  await user.click(within(alert).getByRole("button", { name: "Trust repository…" }));
  const dialog = await screen.findByRole("dialog", { name: "Trust orbyn-planner?", hidden: true });
  const confirm = within(dialog).getByRole("button", { name: "Trust folder" });
  await user.dblClick(confirm);
  await waitFor(() => expect(mock.writes.filter((write) => write === "trust:planner")).toHaveLength(1));
  // Nothing is retried for the person: the refused write stays one write, and the notice is cleared so they can try again.
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(save).toHaveBeenCalledTimes(1);
});

it("offers no trust dialog on an ordinary thread's refused write", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: {} }));
  const { save } = setup();
  save.mockRejectedValue(new ApiFailure("repo_untrusted", "Trust this repository before using it.", 409));
  render(<DiffPanel threadId="t" />);
  await revertHunk(user, false);
  expect(await screen.findByRole("alert")).toHaveTextContent("Nothing was changed.");
  expect(screen.queryByRole("button", { name: "Trust repository…" })).toBeNull();
});

it("says so when the repository is not in the project's list, and writes nothing", async () => {
  const user = userEvent.setup();
  const mock = createReposMock({ threads: { t: repos }, repositories: [row("p")] });
  setReposApi(mock);
  const { save } = setup([FILE], "planner");
  save.mockRejectedValue(new ApiFailure("repo_untrusted", "Trust this repository before using it.", 409));
  render(<DiffPanel threadId="t" />);
  await revertHunk(user);
  await user.click(within(await screen.findByRole("alert")).getByRole("button", { name: "Trust repository…" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: /^Trust/, hidden: true })).toBeNull());
  expect(mock.writes).toEqual([]);
});

// #1055: a file of another repository opens as a tab of that repository instead of a notice.
it.each([["Open in editor"], ["Actions for auth/redirect.ts"]])("%s on a file of another repository opens its tab, with its repo_id, and no notice", async (label) => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  setup([FILE], "planner");
  const resolve = vi.spyOn(api, "resolveThreadPath");
  useUi.setState({ panelTab: "diff", panelTabOrder: ["diff"], toasts: [] } as never);
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Repository: app" }).catch(() => undefined);
  await waitFor(() => expect(screen.getAllByText("orbyn-planner").length).toBeGreaterThan(0));
  if (label === "Open in editor") await user.click(await screen.findByRole("button", { name: "Open in editor" }));
  else { await user.click(await screen.findByRole("button", { name: label })); await user.click(await screen.findByRole("menuitem", { name: "Open in editor" })); }
  const id = fileTabId({ kind: "thread", path: "auth/redirect.ts", repoId: "planner" });
  await waitFor(() => expect(useUi.getState().panelTab).toBe(id));
  expect(useUi.getState().panelTabOrder).toContain(id);
  expect(JSON.stringify(useUi.getState())).not.toContain("isn't available yet");
  // The open path resolves the file in the repository it belongs to: the request names that repo_id,
  // so the path is never looked for in the primary checkout.
  await waitFor(() => expect(resolve).toHaveBeenCalledWith("t", expect.objectContaining({ path: "auth/redirect.ts", basis: "repository", repo_id: "planner" }), undefined));
});

// A file the repository's working tree does not hold (deleted there, say) still opens as its tab: the
// resolve answers no checkout path, and the read in the tab says why.
it("a file its repository's working tree does not hold still opens as that repository's tab", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  setup([{ ...FILE, path: "gone.ts" }], "planner");
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: null, repository_path: "gone.ts" });
  useUi.setState({ panelTab: "diff", panelTabOrder: ["diff"], toasts: [] } as never);
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(screen.getAllByText("orbyn-planner").length).toBeGreaterThan(0));
  await user.click(await screen.findByRole("button", { name: "Open in editor" }));
  const id = fileTabId({ kind: "thread", path: "gone.ts", repoId: "planner" });
  await waitFor(() => expect(useUi.getState().panelTab).toBe(id));
  expect(useUi.getState().toasts).toEqual([]);
});

// #1055: the turn picker's totals are the sum over the thread's trusted repositories.
const patchOf = (additions: number, deletions: number, path: string): FileDiff => ({ path, status: "modified", additions, deletions, patch: "" });

it("sums a turn's totals over the trusted repositories, names the one it did not count, and asks each repository with its own repo_id", async () => {
  const user = userEvent.setup();
  const all: ThreadRepo[] = [...repos, { repo_id: "docs", name: "docs", branch: "main", added: 0, deleted: 0, trusted: false, files: null }];
  setReposApi(createReposMock({ threads: { t: all } }));
  const { diff } = setup();
  vi.spyOn(api, "checkpoints").mockResolvedValue([{ run_id: "r1", start: "s1", end: "e1", created_at: "2026-11-12T14:12:00" }]);
  diff.mockImplementation(async (_t, _s, scope, repoId) => {
    if (typeof scope === "string" && scope.startsWith("turn")) return repoId === "planner" ? [patchOf(2, 1, "b.ts"), patchOf(0, 2, "c.ts")] : [patchOf(1, 0, "a.ts")];
    return [];
  });
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Scope: Working tree" });
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Turn" }));
  const picker = await screen.findByRole("menu", { name: "Turn" });
  await waitFor(() => expect(within(picker).getByRole("menuitem")).toHaveTextContent("14:12 · +3 −3 · 3 files · 1 repo not counted"));
  const turnReads = diff.mock.calls.filter((call) => call[2] === "turn&run=r1");
  expect(new Set(turnReads.map((call) => call[3]))).toEqual(new Set([undefined, "planner"]));
  expect(turnReads.every((call) => call[3] !== "docs")).toBe(true);
});

it("a repository whose turn read fails is not counted as zero changes", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { diff } = setup();
  vi.spyOn(api, "checkpoints").mockResolvedValue([{ run_id: "r1", start: "s1", end: "e1", created_at: "2026-11-12T14:12:00" }]);
  diff.mockImplementation(async (_t, _s, scope, repoId) => {
    if (typeof scope === "string" && scope.startsWith("turn")) { if (repoId === "planner") throw new ApiFailure("not_found", "No checkpoint.", 404); return [patchOf(1, 0, "a.ts")]; }
    return [];
  });
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Scope: Working tree" });
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Turn" }));
  const picker = await screen.findByRole("menu", { name: "Turn" });
  await waitFor(() => expect(within(picker).getByRole("menuitem")).toHaveTextContent("14:12 · +1 −0 · 1 file · 1 repo not counted"));
});

// #1055: a line comment made in Changes names the repository of its file, so the same path changed in two repositories
// keeps its own comments and the agent is told which repository the comment is about.
const commentOn = async (user: ReturnType<typeof userEvent.setup>, patch: HTMLElement, text: string) => {
  await user.click(within(patch).getByRole("button", { name: "Comment on line 15" }));
  await user.type(screen.getByRole("textbox", { name: /^Review comment on/ }), text);
  await user.keyboard("{Control>}{Enter}{/Control}");
};

const pickFile = async (user: ReturnType<typeof userEvent.setup>, group: string) => {
  await user.click(within(await screen.findByRole("group", { name: `${group} changes` })).getByRole("button", { name: /^Modified/ }));
};

it("a line comment on another repository's file carries that repository, and shows only on its own file", async () => {
  const user = userEvent.setup();
  useComposerChips.setState({ chips: {}, sent: {} });
  setReposApi(createReposMock({ threads: { t: repos } }));
  setup();
  render(<DiffPanel threadId="t" />);
  await pickFile(user, "orbyn-planner");
  await commentOn(user, await screen.findByRole("group", { name: "Patch" }), "Is this path checked?");
  const chip = useComposerChips.getState().chips.t?.[0];
  expect(chip).toMatchObject({ kind: "review", path: "auth/redirect.ts", line: 15, repoId: "planner", repoName: "orbyn-planner" });
  // The comment says where the file is: the repository's name is part of the label and of what the engine receives.
  expect(chipLabel(chip!)).toBe("orbyn-planner/auth/redirect.ts:15");
  expect(chipPrompt(chip!)).toContain("orbyn-planner/auth/redirect.ts:15");
  expect(screen.getByRole("note", { name: "Review comment orbyn-planner/auth/redirect.ts:15" })).toBeInTheDocument();

  // The primary repository changed a file of the same path: the other repository's comment is not under it.
  await pickFile(user, "app");
  expect(screen.queryByRole("note", { name: /^Review comment/ })).toBeNull();
  await pickFile(user, "orbyn-planner");
  expect(await screen.findByRole("note", { name: "Review comment orbyn-planner/auth/redirect.ts:15" })).toBeInTheDocument();
});

it("a line comment on the primary repository's file names it without a repo_id, and is not shown on another repository's same path", async () => {
  const user = userEvent.setup();
  useComposerChips.setState({ chips: {}, sent: {} });
  setReposApi(createReposMock({ threads: { t: repos } }));
  setup();
  render(<DiffPanel threadId="t" />);
  await pickFile(user, "app");
  await commentOn(user, await screen.findByRole("group", { name: "Patch" }), "Primary only");
  // The primary repository is the thread's own checkout, which names no repository anywhere else: the comment names it but
  // carries no repo_id, so it is the same file as the one a tab of this repository comments on and follows its edits.
  const chip = useComposerChips.getState().chips.t?.[0];
  expect(chip).toMatchObject({ kind: "review", path: "auth/redirect.ts", line: 15, repoName: "app" });
  expect("repoId" in chip!).toBe(false);
  expect(chipLabel(chip!)).toBe("app/auth/redirect.ts:15");
  expect(chipPrompt(chip!)).toContain("app/auth/redirect.ts:15");
  expect(screen.getByRole("note", { name: "Review comment app/auth/redirect.ts:15" })).toBeInTheDocument();
  await pickFile(user, "orbyn-planner");
  expect(screen.queryByRole("note", { name: /^Review comment/ })).toBeNull();
});

it("an ordinary thread's line comment names no repository, as before", async () => {
  const user = userEvent.setup();
  useComposerChips.setState({ chips: {}, sent: {} });
  setReposApi(createReposMock({ threads: {} }));
  setup();
  render(<DiffPanel threadId="t" />);
  await commentOn(user, await screen.findByRole("group", { name: "Patch" }), "No repository here");
  const chip = useComposerChips.getState().chips.t?.[0];
  expect(chip).toMatchObject({ kind: "review", path: "auth/redirect.ts", line: 15 });
  expect("repoId" in chip!).toBe(false);
  expect("repoName" in chip!).toBe(false);
  expect(chipLabel(chip!)).toBe("auth/redirect.ts:15");
});

// #1060 follow-up: a changed file of the thread's primary repository opens as its tab too, which names the repository
// and still resolves and reads as the thread's own: opening it sends no repo_id.
it("opens a file of the primary repository as its own named tab, resolving it as the thread's own", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const binary: FileDiff = { path: "logo.png", status: "modified", additions: 0, deletions: 0, patch: ["diff --git a/logo.png b/logo.png", "index 1111111..2222222 100644", "Binary files a/logo.png and b/logo.png differ", ""].join("\n") };
  const { diff } = setup();
  // Only the primary repository's read answers, and its read is the one without a repo_id.
  diff.mockImplementation(async (_t, _s, _scope, repoId) => (repoId === undefined ? [binary] : []));
  const resolve = vi.spyOn(api, "resolveThreadPath");
  resolve.mockResolvedValue({ cwd: "/repo", checkout_path: "logo.png", repository_path: "logo.png" });
  useUi.setState({ panelTab: "diff", panelTabOrder: ["diff"], toasts: [] } as never);
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("group", { name: "app changes" });
  const [open] = await screen.findAllByRole("button", { name: "Open" });
  await user.click(open!);
  const id = fileTabId({ kind: "thread", path: "logo.png", repoId: "p", primary: true });
  await waitFor(() => expect(useUi.getState().panelTab).toBe(id));
  // The path resolves in the primary repository because none is named, as the thread's own file does.
  await waitFor(() => expect(resolve).toHaveBeenCalledWith("t", expect.objectContaining({ path: "logo.png", basis: "repository" }), undefined));
  expect(resolve.mock.calls.at(-1)![1]).not.toHaveProperty("repo_id");
});

it("keeps Open in editor opening the outside editor for a primary repository's changed file", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { diff } = setup();
  // Only the primary repository's read answers, and its read is the one without a repo_id.
  diff.mockImplementation(async (_t, _s, _scope, repoId) => (repoId === undefined ? [FILE] : []));
  const resolve = vi.spyOn(api, "resolveThreadPath");
  resolve.mockResolvedValue({ cwd: "/repo", checkout_path: "auth/redirect.ts", repository_path: "auth/redirect.ts" });
  vi.spyOn(api, "editors").mockResolvedValue([{ id: "vscode", name: "VS Code", launch_style: "goto", file_manager: false }] as never);
  const opened = vi.spyOn(api, "openInEditor").mockResolvedValue(undefined as never);
  useUi.setState({ panelTab: "diff", panelTabOrder: ["diff"], toasts: [] } as never);
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("group", { name: "app changes" });
  await user.click(await screen.findByRole("button", { name: "Open in editor" }));
  // The primary repository's changed file still opens the person's own editor, as before; only a tab's Open names the repository.
  await waitFor(() => expect(opened).toHaveBeenCalledWith("t", { path: "auth/redirect.ts", editor: "vscode" }));
  expect(opened.mock.calls[0]).toHaveLength(2);
  expect(useUi.getState().panelTab).toBe("diff");
});

it("an ordinary thread's turn totals read once, with the three arguments they always had", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: {} }));
  const { diff } = setup();
  vi.spyOn(api, "checkpoints").mockResolvedValue([{ run_id: "r1", start: "s1", end: "e1", created_at: "2026-11-12T14:12:00" }]);
  diff.mockImplementation(async (_t, _s, scope) => (typeof scope === "string" && scope.startsWith("turn") ? [patchOf(1, 0, "a.ts")] : []));
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Scope: Working tree" });
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Turn" }));
  const picker = await screen.findByRole("menu", { name: "Turn" });
  await waitFor(() => expect(within(picker).getByRole("menuitem")).toHaveTextContent("14:12 · +1 −0 · 1 file"));
  expect(within(picker).getByRole("menuitem")).not.toHaveTextContent("not counted");
  expect(diff).toHaveBeenCalledWith("t", expect.any(AbortSignal), "turn&run=r1");
});

// The Changes footer's Create pull request is the thread's chosen repository's, like the header's Git menu: an ordinary
// thread and the primary repository are unchanged and send no repo_id.
it("creates the pull request in the chosen repository, whose links and branches are read with its repo_id", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches, diff } = setup([FILE], "planner");
  chooseRepo("t", "planner");
  const action = vi.spyOn(api, "gitAction").mockResolvedValue({ phases: [] } as never);
  render(<DiffPanel threadId="t" />);
  const create = await screen.findByRole("button", { name: "Create pull request" });
  await waitFor(() => expect(pullsApi.linked).toHaveBeenCalledWith("t", expect.any(AbortSignal), "planner"));
  await user.click(create);
  const dialog = await screen.findByRole("dialog", { name: "Create pull request", hidden: true });
  expect(within(dialog).getByText("Acts on orbyn-planner only.")).toBeVisible();
  await waitFor(() => expect(branches.mock.calls.some((call) => call[2] === "planner")).toBe(true));
  await user.click(within(dialog).getByRole("button", { name: "Create pull request" }));
  await waitFor(() => expect(action).toHaveBeenCalledWith("t", expect.objectContaining({ push: true, pull_request: expect.any(Object), repo_id: "planner" })));
  // The branch-ahead check, which decides whether there is anything to carry, is that repository's read too, against
  // the base that repository's worktree was cut from.
  expect(diff.mock.calls.some((call) => call[2] === "branch&base=develop" && call[3] === "planner")).toBe(true);
});

it("compares the branch-ahead check against each selected repository's own base", async () => {
  // The primary's worktree was cut from trunk and the planner's from develop. A multi-repository thread has no single
  // worktree base, so before migration 90 both were compared against "main".
  const based: ThreadRepo[] = [{ ...repos[0]!, base_branch: "trunk" }, { ...repos[1]!, base_branch: "develop" }];
  setReposApi(createReposMock({ threads: { t: based } }));
  const { diff } = setup([FILE], "planner");
  render(<DiffPanel threadId="t" />);
  // The primary repository's own base, read without a repo_id as the primary's calls are.
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "branch&base=trunk" && call.length === 3)).toBe(true));
  // The chosen repository's own base, read with its repo_id.
  act(() => chooseRepo("t", "planner"));
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "branch&base=develop" && call[3] === "planner")).toBe(true));
});

it("falls back to the older base when the repository's base was not recorded", async () => {
  // A repository stored before migration 90 serves no base, so the thread's worktree base stays the comparison base,
  // as it was before the service recorded one per repository.
  const unrecorded: ThreadRepo[] = repos.map((repo) => ({ ...repo, base_branch: null }));
  setReposApi(createReposMock({ threads: { t: unrecorded } }));
  const { diff } = setup([FILE], "planner");
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/repo", active_run: null, worktree: { path: "/repo", branch: "fix/login", base: "release" } } as never } });
  chooseRepo("t", "planner");
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "branch&base=release" && call[3] === "planner")).toBe(true));
  expect(diff.mock.calls.some((call) => String(call[2]).startsWith("branch&base=main"))).toBe(false);
});

it("an ordinary thread keeps comparing ahead against its worktree base, with no repository", async () => {
  setReposApi(createReposMock({ threads: {} }));
  const { diff } = setup();
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/repo", active_run: null, worktree: { path: "/repo", branch: "fix/login", base: "release" } } as never } });
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "branch&base=release" && call.length === 3)).toBe(true));
  expect(diff.mock.calls.filter((call) => String(call[2]).startsWith("branch&")).every((call) => call.length === 3)).toBe(true);
});

// The offer follows the chosen repository's Git availability: a primary checkout whose Git read fails is not the
// repository the button acts on, and a chosen repository whose own read fails keeps the button off.
it("offers Create pull request for a chosen repository that answers Git when the primary's read fails", async () => {
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches } = setup([FILE], "planner");
  branches.mockImplementation(async (_t, _s, repoId) => {
    if (repoId === undefined) throw new ApiFailure("unexpected", "Git is not available.", 0);
    return [{ name: "fix/plan", current: true, remote: false }, { name: "develop", current: false, remote: false }] as never;
  });
  chooseRepo("t", "planner");
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(branches.mock.calls.some((call) => call[2] === "planner")).toBe(true));
  expect(await screen.findByRole("button", { name: "Create pull request" })).toBeVisible();
});

it("hides Create pull request when the chosen repository's own Git read fails", async () => {
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches } = setup();
  branches.mockImplementation(async (_t, _s, repoId) => {
    if (repoId === "planner") throw new ApiFailure("unexpected", "Git is not available.", 0);
    return [{ name: "fix/login", current: true, remote: false }, { name: "main", current: false, remote: false }] as never;
  });
  chooseRepo("t", "planner");
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(branches.mock.calls.some((call) => call[2] === "planner")).toBe(true));
  expect(screen.queryByRole("button", { name: "Create pull request" })).toBeNull();
});

it("offers no Create pull request where the chosen repository already has one linked", async () => {
  setReposApi(createReposMock({ threads: { t: repos } }));
  setup([FILE], "planner");
  chooseRepo("t", "planner");
  vi.mocked(pullsApi.linked).mockImplementation(async (_threadId, _signal, repoId) => (repoId === "planner" ? [{ number: 4 } as never] : []));
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(pullsApi.linked).toHaveBeenCalledWith("t", expect.any(AbortSignal), "planner"));
  expect(screen.queryByRole("button", { name: "Create pull request" })).toBeNull();
});

it("an ordinary thread's Create pull request reads and sends no repository", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: {} }));
  setup();
  const action = vi.spyOn(api, "gitAction").mockResolvedValue({ phases: [] } as never);
  render(<DiffPanel threadId="t" />);
  await user.click(await screen.findByRole("button", { name: "Create pull request" }));
  const dialog = await screen.findByRole("dialog", { name: "Create pull request", hidden: true });
  await user.click(within(dialog).getByRole("button", { name: "Create pull request" }));
  await waitFor(() => expect(action).toHaveBeenCalledWith("t", expect.objectContaining({ push: true, pull_request: expect.any(Object) })));
  expect(action.mock.calls[0]![1]).not.toHaveProperty("repo_id");
  expect(vi.mocked(pullsApi.linked).mock.calls.every((call) => call.length === 2)).toBe(true);
});

it("counts only the chosen repository's own changes before offering Create pull request", async () => {
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { diff } = setup();
  // The working tree holds changes in the primary repository only; the chosen repository is clean and nothing is committed
  // ahead of its base there. Another repository's file in the same list is not the chosen repository's to carry.
  diff.mockImplementation(async (_t, _s, scope, repoId) =>
    (String(scope).startsWith("branch&") || repoId === "planner" ? [] : [FILE]));
  chooseRepo("t", "planner");
  render(<DiffPanel threadId="t" />);
  await waitFor(() => expect(pullsApi.linked).toHaveBeenCalledWith("t", expect.any(AbortSignal), "planner"));
  // The chosen repository's own read has answered before the offer is judged.
  await waitFor(() => expect(diff.mock.calls.some((call) => call[2] === "worktree" && call[3] === "planner")).toBe(true));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Create pull request" })).toBeNull());
});

it("keeps the pull request dialog on the repository it opened for when the repository read fails behind it", async () => {
  const user = userEvent.setup();
  const mock = createReposMock({ threads: { t: repos } });
  setReposApi(mock);
  const { diff } = setup([FILE], "planner");
  chooseRepo("t", "planner");
  const action = vi.spyOn(api, "gitAction").mockResolvedValue({ phases: [] } as never);
  const threadRepos = vi.spyOn(mock, "threadRepos");
  render(<DiffPanel threadId="t" />);
  await user.click(await screen.findByRole("button", { name: "Create pull request" }));
  const dialog = await screen.findByRole("dialog", { name: "Create pull request", hidden: true });
  expect(within(dialog).getByText("Acts on orbyn-planner only.")).toBeVisible();
  // A repository read fails behind the open dialog, as a refresh during a service hiccup can: the list is gone, the target is not.
  threadRepos.mockRejectedValueOnce(new ApiFailure("unexpected", "Something went wrong.", 0));
  act(() => useThreadLogs.getState().touchDiff("t"));
  await waitFor(() => expect(threadRepos.mock.calls.length).toBeGreaterThan(1));
  // The list really went: the changes read again names no repository, as it does with no list to choose from.
  await waitFor(() => {
    const worktree = diff.mock.calls.filter((call) => call[2] === "worktree");
    expect(worktree.at(-1)?.[3]).toBeUndefined();
  });
  const kept = await screen.findByRole("dialog", { name: "Create pull request", hidden: true });
  expect(within(kept).getByText("Acts on orbyn-planner only.")).toBeVisible();
  await user.click(within(kept).getByRole("button", { name: "Create pull request" }));
  await waitFor(() => expect(action).toHaveBeenCalledWith("t", expect.objectContaining({ repo_id: "planner" })));
});
