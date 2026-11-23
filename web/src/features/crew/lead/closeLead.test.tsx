import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as fixtures from "../../../../mock/fixtures";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { archiveThreads, settleThreads } from "../../workspace/sidebar/threadActions";
import { useUndoNotice } from "../../workspace/sidebar/threadUndo";
import { CloseLeadDialog } from "./CloseLeadDialog";
import type { LiveTask } from "./api";
import { useCloseLead } from "./closeLead";

beforeEach(() => {
  vi.stubGlobal("__ORBYN_PREVIEWS__", true);
  installMockFetch();
  useConnection.setState({ remote: false, crewPreview: true });
  useUi.setState({ confirmArchive: false });
  useUndoNotice.setState({ notice: null });
  useCloseLead.setState({ pending: null });
  window.location.hash = "";
});

afterEach(() => vi.restoreAllMocks());

const thread = () => {
  const found = fixtures.threads().find((one) => one.id === "t-done");
  if (!found) throw new Error("the fixture thread is missing");
  return found;
};

const live = (tasks: LiveTask[]) => ({ tasks });

/** Answers the two lead routes itself and leaves every other route to the mock, keeping the calls watchable. */
function answerLead(tasks: LiveTask[], close: { closed: string[]; failed: string[] } = { closed: tasks.map((task) => task.id), failed: [] }) {
  const base = fetch;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/live")) return new Response(JSON.stringify(live(tasks)), { status: 200 });
      if (url.endsWith("/close")) return new Response(JSON.stringify(close), { status: 200 });
      return base(input, init);
    }),
  );
}

const seed = (target: ReturnType<typeof thread>) => useWorkspace.setState({ threads: { [target.id]: target } });
const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");
const tasks = (): LiveTask[] => [
  { id: "ct-one", name: "Use next in the auth redirect", state: "running" },
  { id: "ct-two", name: "Update the login docs", state: "needs_person" },
  { id: "ct-three", name: "Add a rate limit", state: "queued" },
];

describe("settling a lead thread with live Crew work", () => {
  it("settles as before and reads nothing while Crew Preview is off", async () => {
    useConnection.setState({ crewPreview: false });
    const target = thread();
    seed(target);
    render(<CloseLeadDialog />);
    await settleThreads([target]);
    expect(calls("/api/crew/lead")).toHaveLength(0);
    expect(useWorkspace.getState().threads[target.id]?.lifecycle?.settled_at).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useUndoNotice.getState().notice?.action).toBe("Settled");
  });

  it("keeps the old path when the live route answers 404", async () => {
    const target = thread();
    seed(target);
    render(<CloseLeadDialog />);
    await settleThreads([target]);
    expect(calls("/api/crew/lead")).toHaveLength(1);
    expect(writes().filter(([, init]) => init?.method === "PATCH")).toHaveLength(1);
    expect(useWorkspace.getState().threads[target.id]?.lifecycle?.settled_at).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useUndoNotice.getState().notice?.action).toBe("Settled");
  });

  it("asks with the exact words, focuses Cancel, and sends nothing when Cancel is pressed", async () => {
    const target = thread();
    const list = tasks();
    seed(target);
    answerLead(list);
    const user = userEvent.setup();
    render(<CloseLeadDialog />);
    const settled = settleThreads([target]);

    expect(await screen.findByRole("heading", { name: "Settle “Explain how the event log resumes”?" })).toBeVisible();
    expect(
      screen.getByText(
        "It started 3 tasks that are still live. Settling the thread cancels each one: its worker is stopped and the task ends as cancelled. Work already merged stays.",
      ),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "Will be cancelled (3)" })).toBeVisible();
    expect(screen.getByText("Use next in the auth redirect")).toBeVisible();
    expect(screen.getByText("needs person")).toBeVisible();
    expect(screen.getByRole("button", { name: "Settle and cancel 3 tasks" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await settled;
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("/close")).toHaveLength(0);
    expect(writes()).toHaveLength(0);
    expect(useWorkspace.getState().threads[target.id]?.lifecycle?.settled_at).toBeNull();
  });

  it("cancels on confirm, patches the thread, says how many, and leaves no Undo", async () => {
    const target = thread();
    seed(target);
    answerLead(tasks());
    const user = userEvent.setup();
    render(<CloseLeadDialog />);
    const settled = settleThreads([target]);
    await user.click(await screen.findByRole("button", { name: "Settle and cancel 3 tasks" }));
    await settled;

    const closes = calls("/close");
    expect(closes).toHaveLength(1);
    expect(String(closes[0]?.[0])).toBe("/api/crew/lead/t-done/close");
    expect(closes[0]?.[1]?.method).toBe("POST");
    expect(closes[0]?.[1]?.body).toBeUndefined();
    expect(useWorkspace.getState().threads[target.id]?.lifecycle?.settled_at).toBeTruthy();
    const toast = useUi.getState().toasts.at(-1);
    expect(toast?.text).toBe("Thread settled. 3 tasks cancelled.");
    expect(toast?.kind).toBe("done");
    expect(useUndoNotice.getState().notice).toBeNull();
  });

  it("shows five task rows and counts the rest in one line", async () => {
    const target = thread();
    seed(target);
    const many = Array.from({ length: 12 }, (_, index): LiveTask => ({
      id: `ct-${index + 1}`,
      name: `Task ${index + 1}`,
      state: index === 0 ? "needs_person" : "running",
    }));
    answerLead(many);
    const user = userEvent.setup();
    render(<CloseLeadDialog />);
    const settled = settleThreads([target]);

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByRole("listitem")).toHaveLength(5);
    expect(within(dialog).getByText("and 7 more")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Settle and cancel 12 tasks" })).toBeVisible();

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await settled;
  });

  it("says a failed task could not be stopped, with a way to open it", async () => {
    const target = thread();
    seed(target);
    const list = [
      { id: "ct-one", name: "Use next in the auth redirect", state: "running" },
      { id: "ct-two", name: "Update the login docs", state: "needs_person" },
    ];
    answerLead(list, { closed: ["ct-one"], failed: ["ct-two"] });
    const user = userEvent.setup();
    render(<CloseLeadDialog />);
    const settled = settleThreads([target]);
    await user.click(await screen.findByRole("button", { name: "Settle and cancel 2 tasks" }));
    await settled;

    const toast = useUi.getState().toasts.at(-1);
    expect(toast?.kind).toBe("failed");
    expect(toast?.text).toContain("Thread settled. 1 task cancelled.");
    expect(toast?.text).toContain("1 could not be stopped: Update the login docs.");
    expect(toast?.text).toContain("Its worker may still be running. Stop it again from the Crew board.");
    expect(toast?.action?.label).toBe("Open task");
    toast?.action?.run();
    expect(window.location.hash).toBe("#/crew/flow/task/ct-two");
  });
});

describe("archiving a lead thread with live Crew work", () => {
  it("uses the archive words and skips the old confirm when live tasks exist", async () => {
    useUi.setState({ confirmArchive: true });
    const confirm = vi.fn(() => true);
    vi.stubGlobal("confirm", confirm);
    const target = thread();
    seed(target);
    answerLead(tasks());
    const user = userEvent.setup();
    render(<CloseLeadDialog />);
    const archived = archiveThreads([target]);

    expect(await screen.findByRole("heading", { name: "Archive “Explain how the event log resumes”?" })).toBeVisible();
    expect(
      screen.getByText(
        "It started 3 tasks that are still live. Archiving the thread cancels each one: its worker is stopped and the task ends as cancelled. Work already merged stays.",
      ),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Archive and cancel 3 tasks" }));
    await archived;

    expect(confirm).not.toHaveBeenCalled();
    expect(useWorkspace.getState().threads[target.id]?.lifecycle?.archived_at).toBeTruthy();
    const toast = useUi.getState().toasts.at(-1);
    expect(toast?.text).toBe("Thread archived. 3 tasks cancelled.");
    expect(toast?.kind).toBe("done");
    expect(useUndoNotice.getState().notice).toBeNull();
  });

  it("still asks the old archive confirm when there are no live tasks", async () => {
    useUi.setState({ confirmArchive: true });
    const confirm = vi.fn(() => true);
    vi.stubGlobal("confirm", confirm);
    const target = thread();
    seed(target);
    render(<CloseLeadDialog />);
    await archiveThreads([target]);

    expect(confirm).toHaveBeenCalledWith("Archive this thread?");
    expect(useWorkspace.getState().threads[target.id]?.lifecycle?.archived_at).toBeTruthy();
    expect(useUndoNotice.getState().notice?.action).toBe("Archived");
  });
});
