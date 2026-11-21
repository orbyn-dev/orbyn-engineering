import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../mock/repos";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { chooseRepo, setReposApi, type ThreadRepo } from "../repos";
import { BranchRow, GitActions } from "./GitControl";

// F70: the header Git menu and the branch picker act on the repository the person chose. `repo_id` is a query on the read and a body
// field on the writes (api/client.test.ts holds the wire); here every call is checked for the repo_id it was handed.
const thread: Thread = { id: "t", project_id: "p", title: "Task", engine: "claude", model: "chosen",
  cwd: "/checkout", worktree: null, skill_profile: null, active_run: null, last_run_state: null,
  created_at: "", updated_at: "", scratch: false };
const repos: ThreadRepo[] = [
  { repo_id: "p", name: "app", branch: "fix/login", added: 1, deleted: 0, trusted: true },
  { repo_id: "planner", name: "orbyn-planner", branch: "fix/plan", added: 2, deleted: 1, trusted: true },
];

beforeEach(() => {
  useWorkspace.setState({ threads: { t: thread as never } });
  HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) { this.setAttribute("open", ""); };
});
afterEach(() => { chooseRepo("t", null); setReposApi(null); vi.restoreAllMocks(); });

function spies() {
  const branches = vi.spyOn(api, "branches").mockResolvedValue([{ name: "fix/login", current: true, remote: false }, { name: "main", current: false, remote: false }] as never);
  const action = vi.spyOn(api, "gitAction").mockResolvedValue({ phases: [{ name: "commit", ok: true, message: null, url: null }] } as never);
  const create = vi.spyOn(api, "createBranch").mockResolvedValue(undefined);
  const swap = vi.spyOn(api, "switchBranch").mockResolvedValue(undefined);
  return { branches, action, create, swap };
}
const chooser = (name: string) => screen.findByRole("button", { name: `Repository: ${name}` });
const pickRepo = async (user: ReturnType<typeof userEvent.setup>, from: string, to: string) => {
  await user.click(await chooser(from));
  await user.click(await screen.findByRole("menuitem", { name: to }));
};

it("an ordinary thread (404 on the repositories) has no chooser and sends no repo_id", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: {} }));
  const { branches, action } = spies();
  render(<GitActions thread={thread} />);
  await user.click(await screen.findByRole("button", { name: "Git" }));
  await user.click(screen.getByRole("menuitem", { name: "Commit" }));
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Commit" }));
  expect(screen.queryByRole("button", { name: /^Repository:/ })).toBeNull();
  // No third argument at all: an ordinary thread's calls are the calls it always made.
  expect(branches.mock.calls.every((call) => call.length === 2)).toBe(true);
  expect(action).toHaveBeenCalledExactlyOnceWith("t", { commit_message: "", push: false });
});

it("a multi-repository thread defaults to the primary and sends no repo_id for it, then the chosen repository's id", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches, action } = spies();
  render(<GitActions thread={thread} />);
  expect(await chooser("app")).toBeVisible();
  await waitFor(() => expect(branches).toHaveBeenCalled());
  expect(branches.mock.calls.every((call) => call[2] === undefined)).toBe(true);

  await user.click(screen.getByRole("button", { name: "Git" }));
  await user.click(screen.getByRole("menuitem", { name: "Commit" }));
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Commit" }));
  await waitFor(() => expect(action).toHaveBeenCalledTimes(1));
  expect(action.mock.calls[0]![1]).toEqual({ commit_message: "", push: false });
  await user.click(await screen.findByRole("button", { name: "Close" }));

  await pickRepo(user, "app", "orbyn-planner");
  expect(await chooser("orbyn-planner")).toBeVisible();
  // The branch read for the Git menu is the chosen repository's: repo_id is the query of the read.
  await waitFor(() => expect(branches.mock.calls.some((call) => call[2] === "planner")).toBe(true));
  await user.click(screen.getByRole("button", { name: "Git" }));
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Push" }));
  await waitFor(() => expect(action).toHaveBeenCalledTimes(2));
  expect(action.mock.calls[1]![1]).toEqual({ push: true, repo_id: "planner" });
  // The dialog says which repository it acts on, and the pull request base list is that repository's.
  expect(branches.mock.calls.filter((call) => call[2] === "planner").length).toBeGreaterThan(0);
});

it("says in the menu that publishing is the primary repository's when another one is chosen", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  spies();
  render(<GitActions thread={thread} />);
  await pickRepo(user, "app", "orbyn-planner");
  await user.click(await screen.findByRole("button", { name: "Git" }));
  const publish = await screen.findByRole("menuitem", { name: "Publish repository" });
  expect(publish).toBeDisabled();
  expect(publish).toHaveAttribute("title", "Publishing applies to the project's primary repository.");
});

it("keeps the chooser when the chosen repository can't answer Git questions, so another can be picked", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches } = spies();
  branches.mockImplementation(async (_id, _signal, repoId) => {
    if (repoId === "planner") throw new (await import("../../api/client")).ApiFailure("repo_untrusted", "Not trusted.", 409);
    return [{ name: "main", current: true, remote: false }] as never;
  });
  render(<GitActions thread={thread} />);
  await pickRepo(user, "app", "orbyn-planner");
  expect(await chooser("orbyn-planner")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Git" }));
  expect(await screen.findByRole("menuitem", { name: "Commit" })).toBeDisabled();
  await pickRepo(user, "orbyn-planner", "app");
  await user.click(screen.getByRole("button", { name: "Git" }));
  await waitFor(() => expect(screen.getByRole("menuitem", { name: "Commit" })).toBeEnabled());
});

it("the branch picker reads, switches and creates in the chosen repository, and names that repository's branch on the row", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { branches, create, swap } = spies();
  render(<BranchRow thread={thread} branch="fix/login" />);
  await user.click(await screen.findByRole("button", { name: "Branch fix/login" }));
  const dialog = within(screen.getByRole("dialog"));
  await user.click(await dialog.findByRole("button", { name: "Repository: app" }));
  await user.click(await screen.findByRole("menuitem", { name: "orbyn-planner" }));
  await waitFor(() => expect(branches.mock.calls.some((call) => call[2] === "planner")).toBe(true));
  await user.click(await dialog.findByRole("button", { name: /^main/ }));
  expect(swap).toHaveBeenCalledExactlyOnceWith("t", "main", "planner");
  expect(create).not.toHaveBeenCalled();
  // The row now names the chosen repository's own branch.
  expect(await screen.findByRole("button", { name: "Branch fix/plan" })).toBeVisible();
});

it("the branch picker of the primary repository sends no repo_id", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos } }));
  const { create, swap } = spies();
  render(<BranchRow thread={thread} branch="fix/login" />);
  await user.click(await screen.findByRole("button", { name: "Branch fix/login" }));
  const dialog = within(screen.getByRole("dialog"));
  await user.type(dialog.getByLabelText("Search branches"), "feat/new");
  await user.click(await dialog.findByRole("button", { name: /^Create branch/ }));
  expect(create).toHaveBeenCalledExactlyOnceWith("t", "feat/new");
  expect(swap).not.toHaveBeenCalled();
});
