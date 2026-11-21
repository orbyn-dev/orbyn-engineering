import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { ApiFailure } from "../../api/client";
import { useWorkspace } from "../../product/state/workspace";
import { useUi } from "../../product/state/ui";
import { createReposMock } from "../../../mock/repos";
import type { ThreadRepo } from "./contract";
import { RepoGitMenu } from "./RepoGitMenu";
import { setReposApi } from "./api";

afterEach(() => { setReposApi(null); useWorkspace.setState({ threads: {} }); });

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

it("offers no merge, review or open-pull-request item: those routes take no repo_id yet", async () => {
  const { user } = draw();
  const menu = await openMenu(user);
  expect(within(menu).queryByRole("menuitem", { name: /merge|review|open pull request/i })).toBeNull();
});

it("shows no notice for a push, commit or pull request: the frames draw none", async () => {
  useUi.setState({ toasts: [] });
  const { mock, user } = draw();
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-git:thread-1:orbyn-planner:push"]));
  expect(toastTexts()).toEqual([]);
});
