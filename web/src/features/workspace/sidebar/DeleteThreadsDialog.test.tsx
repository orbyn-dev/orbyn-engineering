import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import * as fixtures from "../../../../mock/fixtures";
import { createReposMock } from "../../../../mock/repos";
import { useWorkspace } from "../../../product/state/workspace";
import { Toasts } from "../../../product/ui/Toasts";
import { installMockFetch } from "../../../test-support";
import { setReposApi } from "../../repos";
import { DeleteThreadsDialog } from "./DeleteThreadsDialog";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});
afterEach(() => setReposApi(null));

const deletes = () =>
  vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method === "DELETE").map(([url]) => String(url));

/** Two of the workspace's threads, so the refusal of one still leaves the other's delete in the batch. */
function twoThreads() {
  const all = fixtures.threads();
  const refused = all.find((thread) => thread.id === "t-done")!;
  const removed = all.find((thread) => thread.id === "t-tools")!;
  useWorkspace.setState({ threads: { [refused.id]: refused, [removed.id]: removed } });
  return { refused, removed };
}

it("removes the threads the service deleted, keeps the refused one, and says the service's reason in a toast", async () => {
  const { refused, removed } = twoThreads();
  mock.fail(`/api/threads/${refused.id}`, 1, 409, "thread_busy", "A thread in this family is running.");
  const user = userEvent.setup();
  render(<><DeleteThreadsDialog threads={[refused, removed]} onClose={() => undefined} /><Toasts /></>);

  await user.click(screen.getByRole("button", { name: "Delete" }));
  const message = await screen.findByText("Couldn't delete the thread. A thread in this family is running.");
  expect(message).toBeVisible();
  await waitFor(() => expect(useWorkspace.getState().threads[removed.id]).toBeUndefined());
  expect(useWorkspace.getState().threads[refused.id]).toEqual(refused);
  expect(deletes().sort()).toEqual([`/api/threads/${refused.id}`, `/api/threads/${removed.id}`].sort());
});

const threadById = (id: string) => fixtures.threads().find((thread) => thread.id === id)!;

/** The mock's repositories for one thread: two of them make it a multi-repository thread. */
const multiRepo = (threadId: string) => createReposMock({
  threads: {
    [threadId]: [
      { repo_id: "orbyn-engineering", name: "orbyn-engineering", branch: "orbyn/login", added: 2, deleted: 1, trusted: true },
      { repo_id: "orbyn-planner", name: "orbyn-planner", branch: "orbyn/login", added: 0, deleted: 0, trusted: true },
    ],
  },
});

it("names every repository's worktree for a single multi-repository thread", async () => {
  const thread = threadById("t-done");
  setReposApi(multiRepo(thread.id));
  render(<DeleteThreadsDialog threads={[thread]} onClose={() => undefined} />);

  expect(await screen.findByText("Its messages and checkpoints are deleted. Code changes in every repository's worktree stay.")).toBeVisible();
  expect(screen.queryByText("Its messages and checkpoints are deleted. Code changes in its worktree stay.")).toBeNull();
});

it("keeps the one worktree words for a thread in one repository", async () => {
  const thread = threadById("t-done");
  setReposApi(createReposMock());
  render(<DeleteThreadsDialog threads={[thread]} onClose={() => undefined} />);

  expect(await screen.findByText("Its messages and checkpoints are deleted. Code changes in its worktree stay.")).toBeVisible();
  expect(screen.queryByText(/every repository's worktree/)).toBeNull();
});

it("keeps the plural words for several threads and reads no repositories", async () => {
  const first = threadById("t-done");
  const second = threadById("t-tools");
  const repos = multiRepo(first.id);
  const read = vi.spyOn(repos, "threadRepos");
  setReposApi(repos);
  render(<DeleteThreadsDialog threads={[first, second]} onClose={() => undefined} />);

  expect(screen.getByText("Their messages and checkpoints are deleted. Code changes in their worktrees stay.")).toBeVisible();
  expect(read).not.toHaveBeenCalled();
});

it("keeps the one worktree words when the repository read fails", async () => {
  const thread = threadById("t-done");
  const repos = createReposMock();
  const read = vi.fn(async () => { throw new Error("offline"); });
  repos.threadRepos = read;
  setReposApi(repos);
  render(<DeleteThreadsDialog threads={[thread]} onClose={() => undefined} />);

  await waitFor(() => expect(read).toHaveBeenCalled());
  expect(screen.getByText("Its messages and checkpoints are deleted. Code changes in its worktree stay.")).toBeVisible();
});
