import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../../mock/repos";
import { api, ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { FileDiff } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { chooseRepo, setReposApi, type RepoRow, type ThreadRepo } from "../../repos";
import * as panelsApi from "../api";
import { DiffPanel } from "./DiffPanel";

// F70: the commit and branch pickers act on the repository chosen for the thread, and a refused hunk write offers the trust prompt.
const FILE: FileDiff = { path: "auth/redirect.ts", status: "modified", additions: 2, deletions: 1,
  patch: "diff --git a/auth/redirect.ts b/auth/redirect.ts\nindex 1111111..2222222 100644\n--- a/auth/redirect.ts\n+++ b/auth/redirect.ts\n@@ -14,3 +14,4 @@ after(req)\n export function after(req) {\n-  return \"/home\"\n+  const next = req.query.next\n+  return inside(next) ? next : \"/\"\n }\n" };
const DISK = `${"// filler\n".repeat(13)}export function after(req) {\n  const next = req.query.next\n  return inside(next) ? next : "/"\n}\n`;
const repos: ThreadRepo[] = [
  { repo_id: "p", name: "app", branch: "fix/login", added: 1, deleted: 0, trusted: true, files: ["auth/redirect.ts"] },
  { repo_id: "planner", name: "orbyn-planner", branch: "fix/plan", added: 2, deleted: 1, trusted: true, files: ["auth/redirect.ts"] },
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
