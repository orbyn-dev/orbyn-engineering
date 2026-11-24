import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../mock/repos";
import { ApiFailure } from "../../api/client";
import type { Thread } from "../../api/types";
import { setViewportWidth } from "../../test-support";
import { ReposChip } from "./ReposChip";
import { setReposApi } from "./api";

afterEach(() => { setReposApi(null); vi.unstubAllGlobals(); });
const thread = (id: string) => ({ id, project_id: "p1" }) as Thread;

it("draws nothing, and no error, when the thread's repos route answers 404", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const { container } = render(<ReposChip thread={thread("thread-1")} />);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
});

it("draws no chip for a thread in one repository", async () => {
  setReposApi(createReposMock({ threads: { t: [{ repo_id: "a", name: "a", branch: "x", added: 0, deleted: 0, trusted: true }] } }));
  const { container } = render(<ReposChip thread={thread("t")} />);
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
});

it("counts the repositories and opens one row per repository with its branch, counts and trust state", async () => {
  setReposApi(createReposMock());
  render(<ReposChip thread={thread("thread-1")} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repos 2" }));
  const popover = await screen.findByRole("dialog", { name: "Repositories" });
  expect(within(popover).getByText("Working in 2 repositories")).toBeVisible();
  expect(within(popover).getAllByText("fix/login")).toHaveLength(3); // two rows and the task folder line
  expect(within(popover).getByLabelText("4 added")).toBeVisible();
  expect(within(popover).getAllByText("Trusted")).toHaveLength(2);
  expect(within(popover).getByText("Each repository has its own branch for this thread")).toBeVisible();
});

it("adds only a trusted repository of the project, with one write", async () => {
  const mock = createReposMock({ threads: { t: [
    { repo_id: "orbyn-engineering", name: "orbyn-engineering", branch: "fix/login", added: 1, deleted: 0, trusted: true },
    { repo_id: "orbyn-notes", name: "orbyn-notes", branch: "fix/login", added: 0, deleted: 0, trusted: true },
  ] } });
  setReposApi(mock);
  render(<ReposChip thread={thread("t")} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repos 2" }));
  await user.click(await screen.findByRole("button", { name: "Add a repository to this thread…" }));
  expect(await screen.findAllByRole("menuitem")).toHaveLength(1);
  await user.click(screen.getByRole("menuitem", { name: "orbyn-planner" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-add:t:orbyn-planner"]));
  expect(await screen.findByRole("button", { name: "Repos 3" })).toBeVisible();
});

it("opens as a sheet on a phone", async () => {
  setViewportWidth(390);
  setReposApi(createReposMock());
  render(<ReposChip thread={thread("thread-1")} />);
  await userEvent.click(await screen.findByRole("button", { name: "Repos 2" }));
  const sheet = await screen.findByRole("dialog", { name: "Repositories" });
  expect(within(sheet).getByRole("button", { name: "Close" })).toBeVisible();
  expect(within(sheet).getByText("Working in 2 repositories")).toBeVisible();
});

it("shows the task folder as text with its one branch, and copies the path on request", async () => {
  setReposApi(createReposMock());
  render(<ReposChip thread={thread("thread-1")} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repos 2" }));
  const popover = await screen.findByRole("dialog", { name: "Repositories" });
  const line = await within(popover).findByLabelText("Task folder");
  expect(within(line).getByText("~/Library/Application Support/Orbyn/threads/thread-1")).toBeVisible();
  expect(line.querySelector("a")).toBeNull();
  await user.click(within(line).getByRole("button", { name: "Copy task folder path" }));
  await waitFor(async () => expect(await navigator.clipboard.readText()).toBe("~/Library/Application Support/Orbyn/threads/thread-1"));
});

it("draws no task folder line when the service has no task folder route", async () => {
  const mock = createReposMock();
  mock.taskFolder = async () => null;
  setReposApi(mock);
  render(<ReposChip thread={thread("thread-1")} />);
  await userEvent.click(await screen.findByRole("button", { name: "Repos 2" }));
  const popover = await screen.findByRole("dialog", { name: "Repositories" });
  expect(within(popover).queryByLabelText("Task folder")).toBeNull();
});

it("says how many repositories the task folder holds out of the most a thread can have, and updates after an add", async () => {
  setReposApi(createReposMock({ maxPerThread: 5, threads: { t: [
    { repo_id: "orbyn-engineering", name: "orbyn-engineering", branch: "fix/login", added: 1, deleted: 0, trusted: true },
    { repo_id: "orbyn-notes", name: "orbyn-notes", branch: "fix/login", added: 0, deleted: 0, trusted: true },
  ] } }));
  render(<ReposChip thread={thread("t")} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repos 2" }));
  const popover = await screen.findByRole("dialog", { name: "Repositories" });
  expect(await within(popover).findByText("2 of 5 repositories")).toBeVisible();
  await user.click(within(popover).getByRole("button", { name: "Add a repository to this thread…" }));
  await user.click(screen.getByRole("menuitem", { name: "orbyn-planner" }));
  expect(await within(popover).findByText("3 of 5 repositories")).toBeVisible();
});

it("keeps the thread as it was and says why when the service refuses an add", async () => {
  const mock = createReposMock({ threads: { t: [
    { repo_id: "orbyn-engineering", name: "orbyn-engineering", branch: "fix/login", added: 1, deleted: 0, trusted: true },
    { repo_id: "orbyn-notes", name: "orbyn-notes", branch: "fix/login", added: 0, deleted: 0, trusted: true },
  ] } });
  setReposApi(mock);
  mock.rejectNext("thread-add", new ApiFailure("invalid", "orbyn-planner already has a branch named fix/login.", 400));
  render(<ReposChip thread={thread("t")} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repos 2" }));
  await user.click(await screen.findByRole("button", { name: "Add a repository to this thread…" }));
  await user.click(screen.getByRole("menuitem", { name: "orbyn-planner" }));
  await waitFor(() => expect(mock.writes).toEqual(["thread-add:t:orbyn-planner"]));
  expect(await screen.findByRole("button", { name: "Repos 2" })).toBeVisible();
});
