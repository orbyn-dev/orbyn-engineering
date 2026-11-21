import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { createReposMock } from "../../../mock/repos";
import { ApiFailure } from "../../api/client";
import type { Thread } from "../../api/types";
import { ReposChip } from "./ReposChip";
import { setReposApi } from "./api";
import type { ThreadRepo } from "./contract";

afterEach(() => setReposApi(null));
const thread = { id: "t", project_id: "orbyn-engineering" } as Thread;
const entry = (repo_id: string, added = 0): ThreadRepo => ({ repo_id, name: repo_id, branch: "fix/login", added, deleted: 0, trusted: true });
const draw = async (repos: ThreadRepo[] = [entry("orbyn-engineering"), entry("orbyn-planner"), entry("orbyn-notes")]) => {
  const mock = createReposMock({ threads: { t: repos } });
  setReposApi(mock);
  render(<ReposChip thread={thread} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: `Repos ${repos.length}` }));
  return { mock, user, popover: await screen.findByRole("dialog", { name: "Repositories" }) };
};

it("never offers to remove the primary repository, and offers it for every other one", async () => {
  const { popover } = await draw();
  expect(within(popover).queryByRole("button", { name: "Remove orbyn-engineering from this thread" })).toBeNull();
  expect(within(popover).getByRole("button", { name: "Remove orbyn-planner from this thread" })).toBeVisible();
  expect(within(popover).getByRole("button", { name: "Remove orbyn-notes from this thread" })).toBeVisible();
});

it("asks first, writes nothing on open or cancel, and removes with one write on confirm", async () => {
  const { mock, user, popover } = await draw();
  await user.click(within(popover).getByRole("button", { name: "Remove orbyn-planner from this thread" }));
  const sheet = await screen.findByRole("dialog", { name: "Remove orbyn-planner from this thread?" });
  expect(mock.writes).toEqual([]);
  await user.click(within(sheet).getByRole("button", { name: "Cancel" }));
  expect(mock.writes).toEqual([]);
  await user.click(within(popover).getByRole("button", { name: "Remove orbyn-planner from this thread" }));
  await user.dblClick(await screen.findByRole("button", { name: "Remove" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-remove:t:orbyn-planner"]));
  expect(await screen.findByRole("button", { name: "Repos 2" })).toBeVisible();
});

it("shows the service's refusal plainly and keeps the repository when it holds changes", async () => {
  const { mock, user, popover } = await draw([entry("orbyn-engineering"), entry("orbyn-planner", 3)]);
  await user.click(within(popover).getByRole("button", { name: "Remove orbyn-planner from this thread" }));
  await user.click(await screen.findByRole("button", { name: "Remove" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Commit or discard its changes first");
  expect(mock.writes).toEqual(["thread-remove:t:orbyn-planner"]);
  expect(screen.getByRole("dialog", { name: "Remove orbyn-planner from this thread?" })).toBeVisible();
});

it("shows any other refusal the same way", async () => {
  const { mock, user, popover } = await draw();
  mock.rejectNext("thread-remove", new ApiFailure("not_found", "This thread does not work in that repository.", 404));
  await user.click(within(popover).getByRole("button", { name: "Remove orbyn-notes from this thread" }));
  await user.click(await screen.findByRole("button", { name: "Remove" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("This thread does not work in that repository.");
});
