import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { createReposMock } from "../../../mock/repos";
import type { ThreadRepo } from "./contract";
import { RepoGitMenu } from "./RepoGitMenu";
import { setReposApi } from "./api";

afterEach(() => setReposApi(null));

const planner: ThreadRepo = { repo_id: "orbyn-planner", name: "orbyn-planner", branch: "fix/login", added: 3, deleted: 0, trusted: true };
const draw = (repo: ThreadRepo = planner, onChanged = () => {}) => {
  const mock = createReposMock();
  setReposApi(mock);
  render(<RepoGitMenu threadId="thread-1" repo={repo} onChanged={onChanged} />);
  return { mock, user: userEvent.setup() };
};
const openMenu = async (user: ReturnType<typeof userEvent.setup>, name = "orbyn-planner") => {
  await user.click(screen.getByRole("button", { name: `Git actions for ${name}` }));
  return screen.findByRole("menu", { name: /Repository|orbyn-planner/ });
};

it("names the repository and its branch at the top and offers that repository's own actions, with no all-repositories action", async () => {
  const { user } = draw();
  const menu = await openMenu(user);
  expect(within(menu).getByText("orbyn-planner · fix/login")).toBeVisible();
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Commit…", "Push", "Pull", "Create pull request…", "Remove from this thread"]);
  expect(screen.queryByText(/all repositories/i)).toBeNull();
});

it("says Open pull request when the repository's branch has one", async () => {
  const { user } = draw({ ...planner, pull_request: { number: 142, url: "https://github.com/orbyn-dev/orbyn-planner/pull/142" } });
  await openMenu(user);
  expect(screen.getByRole("menuitem", { name: "Open pull request" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "Create pull request…" })).toBeNull();
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

it("removes the repository from the thread only after the dialog's confirm, and says the files stay", async () => {
  const { mock, user } = draw();
  await openMenu(user);
  await user.click(screen.getByRole("menuitem", { name: "Remove from this thread" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove orbyn-planner from this thread?" });
  expect(within(dialog).getByText(/files stay/i)).toBeVisible();
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Remove" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-remove:thread-1:orbyn-planner"]));
});

it("runs nothing on a repository that is not trusted: every Git action is off and nothing is written", async () => {
  const { mock, user } = draw({ ...planner, trusted: false });
  await openMenu(user);
  for (const name of ["Commit…", "Push", "Pull", "Create pull request…"]) expect(screen.getByRole("menuitem", { name })).toBeDisabled();
  await user.click(screen.getByRole("menuitem", { name: "Push" }));
  expect(mock.writes).toEqual([]);
});

it("shows hostile names as text", async () => {
  const { user } = draw({ ...planner, name: "<img src=x onerror=alert(1)>", branch: "<b>x</b>" });
  await user.click(screen.getByRole("button", { name: "Git actions for <img src=x onerror=alert(1)>" }));
  expect(await screen.findByText("<img src=x onerror=alert(1)> · <b>x</b>")).toBeVisible();
  expect(document.querySelector("img")).toBeNull();
});
