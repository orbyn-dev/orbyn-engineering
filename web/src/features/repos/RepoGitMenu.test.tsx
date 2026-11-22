import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { ApiFailure } from "../../api/client";
import { useWorkspace } from "../../product/state/workspace";
import { useUi } from "../../product/state/ui";
import { createReposMock } from "../../../mock/repos";
import type { ThreadRepo } from "./contract";
import { RepoGitMenu } from "./RepoGitMenu";
import { pullsApi } from "../../api/pulls";
import type { PullRequest } from "../../generated/PullRequest";
import { setReposApi } from "./api";

afterEach(() => { setReposApi(null); useWorkspace.setState({ threads: {} }); vi.restoreAllMocks(); });

const planner: ThreadRepo = { repo_id: "orbyn-planner", name: "orbyn-planner", branch: "fix/login", added: 3, deleted: 0, trusted: true };
const draw = (repo: ThreadRepo = planner, onChanged = () => {}) => {
  const mock = createReposMock();
  setReposApi(mock);
  render(<RepoGitMenu threadId="thread-1" repo={repo} onChanged={onChanged} />);
  return { mock, user: userEvent.setup() };
};
const openMenu = async (user: ReturnType<typeof userEvent.setup>, name = "orbyn-planner") => {
  await user.click(screen.getByRole("button", { name: `Git actions for ${name}` }));
  return screen.findByRole("menu", { name: `Git menu for ${name}` });
};

it("names the repository and its branch at the top and offers that repository's own actions, with no all-repositories action", async () => {
  const { user } = draw();
  const menu = await openMenu(user);
  expect(within(menu).getByText("orbyn-planner · fix/login")).toBeVisible();
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Commit…", "Push", "Create pull request…"]);
  expect(screen.queryByText(/all repositories/i)).toBeNull();
});

it("writes nothing on open, and a Push is one write for one click", async () => {
  let changed = 0;
  const { mock, user } = draw(planner, () => { changed += 1; });
  await openMenu(user);
  expect(mock.writes).toEqual([]);
  await user.dblClick(screen.getByRole("menuitem", { name: "Push" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-git:thread-1:orbyn-planner:push"]));
  await waitFor(() => expect(changed).toBe(1));
});

it("commits only with a message, once, in this repository", async () => {
  const { mock, user } = draw();
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Commit…" }));
  const dialog = await screen.findByRole("dialog", { name: "Commit orbyn-planner" });
  expect(within(dialog).getByRole("button", { name: "Commit" })).toBeDisabled();
  await user.type(within(dialog).getByRole("textbox", { name: "Message" }), "Fix the feed");
  await user.dblClick(within(dialog).getByRole("button", { name: "Commit" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-git:thread-1:orbyn-planner:commit"]));
});

it("offers no way to remove the repository from the thread, and no pull", async () => {
  const { user } = draw();
  const menu = await openMenu(user);
  expect(within(menu).queryByRole("menuitem", { name: /remove/i })).toBeNull();
  expect(within(menu).queryByRole("menuitem", { name: "Pull" })).toBeNull();
});

it("leaves repo_id out for the project's primary repository", async () => {
  useWorkspace.setState({ threads: { "thread-1": { id: "thread-1", project_id: "orbyn-engineering" } as never } });
  const { mock, user } = draw({ ...planner, repo_id: "orbyn-engineering", name: "orbyn-engineering" });
  await openMenu(user, "orbyn-engineering");
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-git:thread-1:primary:push"]));
});

it("offers the shared trust prompt when the service answers repo_untrusted, and trusting it writes the trust route only", async () => {
  useWorkspace.setState({ threads: { "thread-1": { id: "thread-1", project_id: "proj" } as never } });
  const { mock, user } = draw();
  const base = mock.list.bind(mock);
  mock.list = async (id, signal) => { const view = await base(id, signal); return view && { ...view, repositories: view.repositories.map((row) => (row.id === "orbyn-planner" ? { ...row, trusted: false } : row)) }; };
  mock.threadGit = async () => { throw new ApiFailure("repo_untrusted", "orbyn-planner is not trusted.", 409); };
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  const prompt = await screen.findByRole("dialog", { name: /orbyn-planner/ });
  expect(within(prompt).getByText(/Trusting this folder does not trust/)).toBeVisible();
  await user.click(within(prompt).getByRole("button", { name: /^Trust/ }));
  await waitFor(() => expect(mock.writes).toEqual(["trust:orbyn-planner"]));
});

it("says plainly that a repository that is not the thread's was not found", async () => {
  const { mock, user } = draw();
  mock.threadGit = async () => { throw new ApiFailure("not_found", "gone", 404); };
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  await waitFor(() => expect(useUi.getState().toasts.some((toast) => /isn.t one of this thread.s repositories/.test(toast.text))).toBe(true));
});

it("runs nothing on a repository that is not trusted: every Git action is off and nothing is written", async () => {
  const { mock, user } = draw({ ...planner, trusted: false });
  await openMenu(user);
  for (const name of ["Commit…", "Push", "Create pull request…"]) expect(screen.getByRole("menuitem", { name })).toBeDisabled();
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  expect(mock.writes).toEqual([]);
});

it("shows hostile names as text", async () => {
  const { user } = draw({ ...planner, name: "<img src=x onerror=alert(1)>", branch: "<b>x</b>" });
  await user.click(screen.getByRole("button", { name: "Git actions for <img src=x onerror=alert(1)>" }));
  expect(await screen.findByText("<img src=x onerror=alert(1)> · <b>x</b>")).toBeVisible();
  expect(document.querySelector("img")).toBeNull();
});

// ---- notices: a push, commit or pull request shows none (the frames draw none) ----
const toastTexts = () => useUi.getState().toasts.map((toast) => toast.text);

it("offers no merge or review item for a repository with no pull request", async () => {
  const { user } = draw();
  const menu = await openMenu(user);
  expect(within(menu).queryByRole("menuitem", { name: /merge|review/i })).toBeNull();
});

it("shows no notice for a push, commit or pull request: the frames draw none", async () => {
  useUi.setState({ toasts: [] });
  const { mock, user } = draw();
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-git:thread-1:orbyn-planner:push"]));
  expect(toastTexts()).toEqual([]);
});

// ---- the repository's own pull request: its state, and review and merge routed with this repository's repo_id ----
const pull = (number: number, state = "open"): PullRequest => ({ number: BigInt(number), title: "Fix the feed", url: "https://example.test/pr", state, head: "fix/login", base: "main", author: null, draft: false, updated_at: null, labels: [], review_state: null, checks_state: null });
const withPull = (state = "open", number = 7): ThreadRepo => ({ ...planner, pull_request: pull(number, state) });
const itemNames = (menu: HTMLElement) => within(menu).getAllByRole("menuitem").map((item) => item.textContent);

it("shows the repository's open pull request and offers review and merge instead of creating another", async () => {
  const { user } = draw(withPull());
  const menu = await openMenu(user);
  expect(itemNames(menu)).toEqual(["Commit…", "Push", "Pull request #7Open", "Review…", "Merge…"]);
  expect(within(menu).getByRole("menuitem", { name: /Pull request #7/ })).toBeDisabled();
});

it.each([["merged", "Merged"], ["closed", "Closed"]])("offers a new pull request again once the repository's last one is %s, and says its state", async (state, word) => {
  const { user } = draw(withPull(state, 3));
  const menu = await openMenu(user);
  expect(itemNames(menu)).toEqual(["Commit…", "Push", `Pull request #3${word}`, "Create pull request…"]);
});

it("reviews with this repository's repo_id, once, and needs a note for requested changes", async () => {
  const review = vi.spyOn(pullsApi, "threadReview").mockResolvedValue(undefined);
  vi.spyOn(pullsApi, "refreshLinked").mockResolvedValue([]);
  let changed = 0;
  const { user } = draw(withPull(), () => { changed += 1; });
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Review…" }));
  const dialog = await screen.findByRole("dialog", { name: "Review pull request #7 in orbyn-planner" });
  await user.selectOptions(within(dialog).getByRole("combobox", { name: "Review" }), "request_changes");
  expect(within(dialog).getByRole("button", { name: "Submit review" })).toBeDisabled();
  await user.type(within(dialog).getByRole("textbox", { name: "Note" }), "Handle the empty feed");
  await user.dblClick(within(dialog).getByRole("button", { name: "Submit review" }));
  await waitFor(() => expect(review).toHaveBeenCalledTimes(1));
  expect(review).toHaveBeenCalledWith("thread-1", 7, "request_changes", "Handle the empty feed", "orbyn-planner");
  await waitFor(() => expect(changed).toBe(1));
});

it("merges with this repository's repo_id, with the method and branch choice, and leaves the credits to the account setting", async () => {
  const merge = vi.spyOn(pullsApi, "threadMerge").mockResolvedValue(undefined);
  vi.spyOn(pullsApi, "refreshLinked").mockResolvedValue([]);
  const { user } = draw(withPull());
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Merge…" }));
  const dialog = await screen.findByRole("dialog", { name: "Merge pull request #7 in orbyn-planner" });
  await user.selectOptions(within(dialog).getByRole("combobox", { name: "Merge method" }), "rebase");
  await user.click(within(dialog).getByRole("checkbox", { name: "Delete the branch" }));
  await user.dblClick(within(dialog).getByRole("button", { name: "Merge" }));
  await waitFor(() => expect(merge).toHaveBeenCalledTimes(1));
  expect(merge).toHaveBeenCalledWith("thread-1", 7, { method: "rebase", auto: false, delete_branch: true, strip_credits: false }, "orbyn-planner");
});

it("leaves repo_id out of the review and merge of the primary repository", async () => {
  useWorkspace.setState({ threads: { "thread-1": { id: "thread-1", project_id: "orbyn-engineering" } as never } });
  const review = vi.spyOn(pullsApi, "threadReview").mockResolvedValue(undefined);
  vi.spyOn(pullsApi, "refreshLinked").mockResolvedValue([]);
  const { user } = draw({ ...withPull(), repo_id: "orbyn-engineering", name: "orbyn-engineering" });
  await openMenu(user, "orbyn-engineering");
  await user.click(screen.getByRole("menuitem", { name: "Review…" }));
  await user.click(await screen.findByRole("button", { name: "Submit review" }));
  await waitFor(() => expect(review).toHaveBeenCalledWith("thread-1", 7, "approve", "", undefined));
});

it("shows the service's refusal of a merge and keeps the dialog open", async () => {
  vi.spyOn(pullsApi, "threadMerge").mockRejectedValue(new ApiFailure("not_mergeable", "Checks are still running.", 409));
  const { user } = draw(withPull());
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Merge…" }));
  await user.click(await screen.findByRole("button", { name: "Merge" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Checks are still running.");
});

it("turns review and merge off on a repository that is not trusted", async () => {
  const { user } = draw({ ...withPull(), trusted: false });
  await openMenu(user);
  expect(screen.getByRole("menuitem", { name: "Review…" })).toBeDisabled();
  expect(screen.getByRole("menuitem", { name: "Merge…" })).toBeDisabled();
});
