import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import * as fixtures from "../../../../mock/fixtures";
import { useWorkspace } from "../../../product/state/workspace";
import { Toasts } from "../../../product/ui/Toasts";
import { installMockFetch } from "../../../test-support";
import { DeleteThreadsDialog } from "./DeleteThreadsDialog";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

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
