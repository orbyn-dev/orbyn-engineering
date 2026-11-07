import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { installMockFetch } from "../../test-support";
import { ThreadAutomations } from "./ThreadAutomations";
import { navigate } from "../../state/route";
import { useUi } from "../../state/ui";

function failNextTaskRead() {
  const inner = globalThis.fetch;
  let fail = true;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    if (fail && input === "/api/tasks" && (init?.method ?? "GET") === "GET") {
      fail = false;
      return new Response(JSON.stringify({ code: "unavailable", message: "Refresh unavailable." }), { status: 503 });
    }
    return inner(input, init);
  }));
}

/** The list sits behind the Automations row; open it. */
async function openRow(user: ReturnType<typeof userEvent.setup>) {
  const row = await screen.findByRole("button", { name: "Automations" });
  if (row.getAttribute("aria-expanded") === "false") await user.click(row);
}
/** The pause or resume button of one task. */
const toggleOf = (name = "Daily review") => within(screen.getByRole("group", { name })).getByRole("button", { name: /^(Pause|Resume)$/ });

function deferred() {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => { release = resolve; });
  return { promise, release };
}

it.each(["thread change", "unmount"])("ignores a saved toggle after %s", async (change) => {
  const user = userEvent.setup();
  installMockFetch();
  const inner = globalThis.fetch;
  const post = deferred();
  let posted = false;
  let gets = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    if (input.endsWith("/enabled")) {
      posted = true;
      await post.promise;
    }
    if (input === "/api/tasks" && (init?.method ?? "GET") === "GET") gets += 1;
    return inner(input, init);
  }));
  const view = render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(toggleOf());
  expect(posted).toBe(true);
  if (change === "unmount") view.unmount();
  else {
    view.rerender(<ThreadAutomations threadId="t-empty" />);
    await waitFor(() => expect(gets).toBe(2));
  }
  const before = gets;
  await act(async () => { post.release(); await post.promise; });
  expect(gets).toBe(before);
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each([false, true])("preserves saved enabled=%s when the refresh fails and recovers", async (enabled) => {
  const user = userEvent.setup();
  const mock = installMockFetch();
  if (enabled) mock.handle("POST", "/api/tasks/task-review/enabled", { enabled: false });
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  const control = toggleOf();
  expect(control).toHaveAccessibleName(enabled ? "Resume" : "Pause");
  failNextTaskRead();
  await user.click(control);
  await screen.findByRole("alert");
  await waitFor(() => expect(control).toBeEnabled());
  expect(control).toHaveAccessibleName(enabled ? "Pause" : "Resume");
  if (!enabled) expect(screen.getByText(/Every .*Paused/)).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent(/(?:saved|paused|resumed).*refresh/i);
  expect(mock.handle("GET", "/api/tasks").body).toEqual(expect.arrayContaining([
    expect.objectContaining({ id: "task-review", enabled }),
  ]));

  // A later authoritative read must replace the saved row, including an edit
  // made elsewhere; an optimistic override must not mask it indefinitely.
  mock.handle("POST", "/api/tasks/task-review/enabled", { enabled: !enabled });
  act(() => {
    navigate({ name: "settings", section: "tasks" });
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  });
  await waitFor(() => expect(window.location.hash).toContain("settings/tasks"));
  act(() => {
    navigate({ name: "thread", id: "t-approval" });
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  });
  await waitFor(() => expect(control).toHaveAccessibleName(enabled ? "Resume" : "Pause"));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
});

it("merges the saved row without discarding another row refreshed during the save", async () => {
  const user = userEvent.setup();
  installMockFetch();
  const inner = globalThis.fetch;
  const post = deferred();
  let gets = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    const response = await inner(input, init);
    if (input.endsWith("/enabled")) await post.promise;
    if (input === "/api/tasks" && (init?.method ?? "GET") === "GET" && ++gets === 2) {
      const data = await response.json() as { id: string; name: string }[];
      return new Response(JSON.stringify(data.map((row) => row.id === "task-review" ? row : { ...row, name: "Updated hook" })));
    }
    return response;
  }));
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(toggleOf());
  act(() => {
    navigate({ name: "settings", section: "tasks" });
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  });
  act(() => {
    navigate({ name: "thread", id: "t-approval" });
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  });
  expect(await screen.findByText("Updated hook")).toBeVisible();
  failNextTaskRead();
  await act(async () => { post.release(); await post.promise; });
  await screen.findByRole("alert");
  await waitFor(() => expect(toggleOf()).toBeEnabled());
  expect(screen.getByText("Updated hook")).toBeVisible();
  expect(toggleOf()).toHaveAccessibleName("Resume");
});

it("rolls back a failed save and permits a later successful toggle", async () => {
  const user = userEvent.setup();
  const mock = installMockFetch();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  const control = toggleOf();
  mock.fail("/api/tasks/task-review/enabled", 1, 500, "internal", "Save rejected.");
  await user.click(control);
  await waitFor(() => expect(control).toBeEnabled());
  expect(control).toHaveAccessibleName("Pause");
  expect(useUi.getState().toasts.some((toast) => toast.text.includes("Couldn't pause Daily review"))).toBe(true);
  await user.click(control);
  await waitFor(() => expect(control).toBeEnabled());
  expect(control).toHaveAccessibleName("Resume");
});

it("resumes from the saved pause value after a failed refresh", async () => {
  const user = userEvent.setup();
  installMockFetch();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  const control = toggleOf();
  failNextTaskRead();
  await user.click(control);
  await screen.findByRole("alert");
  await waitFor(() => expect(control).toBeEnabled());
  await user.click(control);
  await waitFor(() => expect(control).toBeEnabled());
  expect(control).toHaveAccessibleName("Pause");
  expect(fetch).toHaveBeenLastCalledWith("/api/tasks", expect.objectContaining({ method: "GET" }));
  expect(fetch).toHaveBeenCalledWith("/api/tasks/task-review/enabled", expect.objectContaining({
    method: "POST", body: JSON.stringify({ enabled: true }),
  }));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(useUi.getState().toasts.some((toast) => toast.text.includes("Couldn't pause"))).toBe(false);
});

it("lists only the tasks bound to the thread", async () => {
  installMockFetch();
  const user = userEvent.setup();
  render(<ThreadAutomations threadId="t-approval" />);
  expect(await screen.findByText("2 tasks")).toBeVisible();
  await openRow(user);
  expect(screen.getByText("Daily review")).toBeVisible();
  expect(screen.getByText("Incoming hook")).toBeVisible();
  expect(screen.queryByText("Explain the event log")).toBeNull();
});

it("says there are none for a thread with no automations, and nothing while the list loads", async () => {
  installMockFetch();
  render(<ThreadAutomations threadId="t-empty" />);
  expect(screen.queryByText("No automations on this thread")).toBeNull();
  expect(await screen.findByText("No automations on this thread")).toBeVisible();
});

it("offers Run now for scheduled work but not for a webhook", async () => {
  installMockFetch();
  const user = userEvent.setup();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  expect(within(screen.getByRole("group", { name: "Daily review" })).getByRole("button", { name: "Run now" })).toBeVisible();
  expect(within(screen.getByRole("group", { name: "Incoming hook" })).queryByRole("button", { name: "Run now" })).toBeNull();
});

it("keeps the pause state until the reloaded row carries it", async () => {
  const user = userEvent.setup();
  installMockFetch();
  const inner = globalThis.fetch;
  let gets = 0;
  let release!: () => void;
  const reloadGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      if ((init?.method ?? "GET") === "GET" && input === "/api/tasks" && ++gets > 1) await reloadGate;
      return inner(input, init);
    }),
  );
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  const control = toggleOf();
  expect(control).toHaveAccessibleName("Pause");
  await user.click(control);
  await waitFor(() => expect(gets).toBe(2));
  expect(control).toHaveAccessibleName("Resume");
  release();
  await waitFor(() => expect(control).toBeEnabled());
  expect(control).toHaveAccessibleName("Resume");
});

it("runs a task now", async () => {
  const user = userEvent.setup();
  installMockFetch();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(within(screen.getByRole("group", { name: "Daily review" })).getByRole("button", { name: "Run now" }));
  await waitFor(() =>
    expect(fetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/tasks/task-review/run"),
      expect.objectContaining({ method: "POST" }),
    ),
  );
});

it("refreshes the row after a failed run", async () => {
  const user = userEvent.setup();
  const mock = installMockFetch();
  const inner = globalThis.fetch;
  let gets = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      if ((init?.method ?? "GET") === "GET" && input === "/api/tasks") gets += 1;
      return inner(input, init);
    }),
  );
  mock.fail("/api/tasks/task-review/run", 1, 500, "internal", "The engine did not start.");
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(within(screen.getByRole("group", { name: "Daily review" })).getByRole("button", { name: "Run now" }));
  await waitFor(() => expect(gets).toBe(2));
});

it("links Edit to the task", async () => {
  const user = userEvent.setup();
  installMockFetch();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(within(screen.getByRole("group", { name: "Daily review" })).getByRole("button", { name: "Edit" }));
  expect(window.location.hash).toBe("#/settings/tasks?task=task-review&edit=1");
});

it("keeps the section and offers Retry when the list fails", async () => {
  const user = userEvent.setup();
  const mock = installMockFetch();
  mock.fail("/api/tasks", 1, 500, "internal", "The service is unavailable.");
  render(<ThreadAutomations threadId="t-approval" />);
  expect(await screen.findByText("Couldn’t load automations.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("2 tasks")).toBeVisible();
});

it("ends the open list with a link to Scheduled tasks in Settings", async () => {
  const user = userEvent.setup();
  installMockFetch();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(screen.getByRole("button", { name: "Manage scheduled tasks" }));
  expect(window.location.hash).toBe("#/settings/tasks");
});

it("shows Resume with a different mark than Run now", async () => {
  const user = userEvent.setup();
  installMockFetch();
  render(<ThreadAutomations threadId="t-approval" />);
  await openRow(user);
  await user.click(toggleOf());
  await waitFor(() => expect(toggleOf()).toHaveAccessibleName("Resume"));
  const group = within(screen.getByRole("group", { name: "Daily review" }));
  expect(group.getByRole("button", { name: "Resume" }).innerHTML).not.toBe(group.getByRole("button", { name: "Run now" }).innerHTML);
});
