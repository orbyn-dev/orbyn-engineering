import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { FlowCard } from "../../../generated/FlowCard";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { useWrites } from "../writes/store";
import { NO_WORKER_RAN } from "../tool";
import { couldNotStartItems, NEVER_RAN, type CouldNotStartCard } from "./assemble";
import { NeedsYouInbox } from "./NeedsYouInbox";
import { NeedsCard } from "./NeedsYou";

let mock: MockApi;
beforeAll(async () => {
  installMockFetch();
  await import("../../../app/App");
  // App's first import is the cold module graph of the whole app; under a full-suite run it passes the 10 s default hook timeout.
}, 60_000);
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  useWrites.setState({ ops: {} });
  window.location.hash = "";
  window.localStorage.clear();
});

const posts = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method === "POST");
const REFUSED = "Crew workers on Codex are not available yet.";

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<NeedsYouInbox />);
  const list = await screen.findByRole("list", { name: "Needs you" });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0));
  return { user, list };
}

const card = (over: Partial<FlowCard> = {}): FlowCard => ({
  id: "t1", name: "Build it", kind: "implement", owner: "Lead", state: "needs_person", updated_at: "2026-11-07T14:00:00Z",
  could_not_start: true, can_cancel: true, can_rerun: false, ...over,
});

/** A could-not-start card whose refusal the service named in its structured answer, never in the reason. */
const diskCard = (over: Partial<CouldNotStartCard> = {}): CouldNotStartCard => ({
  ...card(), code: "disk_low", free_bytes: 3_400_000_000, floor_gb: 5, reason: "A different sentence.", ...over,
});

describe("couldNotStartItems", () => {
  it("makes a Could not start item with the reason and only Retry and Dismiss, and skips a card that did start", () => {
    const items = couldNotStartItems([card({ reason: REFUSED, engine: "codex" }), card({ id: "t2", could_not_start: false })]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "could_not_start", title: "Build it", from: "Task Build it · codex", cost: REFUSED, ref: "t1" });
    expect(items[0]!.actions.map((a) => a.label)).toEqual(["Retry on another engine", "Dismiss"]);
    expect(items[0]!.actions.some((a) => a.key === "approve" || a.key === "deny")).toBe(false);
  });
  it("titles the item with the task's name, or its id when it has none", () => {
    expect(couldNotStartItems([card({ name: "" })])[0]!.title).toBe("t1");
    expect(couldNotStartItems([card({ name: "   " })])[0]).toMatchObject({ title: "t1", from: "Task t1" });
  });
  it("says the worker never ran when the service gave no reason", () => {
    expect(couldNotStartItems([card()])[0]!.cost).toBe(NEVER_RAN);
  });
});

describe("a task refused for low disk", () => {
  it("writes the disk sentence from the numbers, not from the reason, with the muted line and Dismiss only", () => {
    const [one] = couldNotStartItems([diskCard({ reason: "The service said something else." })]);
    expect(one).toMatchObject({
      cost: "Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again.",
      hint: NO_WORKER_RAN,
    });
    expect(one!.actions.map((action) => action.label)).toEqual(["Dismiss"]);
    expect(one!.actions.some((action) => action.key === "retry")).toBe(false);
  });

  it("writes free space in whole MB below 1 GB and the floor in whole GB", () => {
    expect(couldNotStartItems([diskCard({ free_bytes: 640 * 1024 * 1024, floor_gb: 5 })])[0]!.cost).toBe(
      "Not started: 640 MB free, Crew needs 5 GB. Free space and try again.",
    );
  });

  it("keeps the reason and Retry for every other code", () => {
    const [one] = couldNotStartItems([diskCard({ code: "not_authorized", reason: REFUSED })]);
    expect(one).toMatchObject({ cost: REFUSED, hint: undefined });
    expect(one!.actions.map((action) => action.label)).toEqual(["Retry on another engine", "Dismiss"]);
  });

  it("draws Dismiss only on the narrow card, and no engine line", () => {
    const [one] = couldNotStartItems([diskCard({ name: "Rename returnTo to next", engine: "codex" })]);
    const { container } = render(<ul><NeedsCard item={one!} handlers={{ act: () => undefined }} /></ul>);
    expect(container).toHaveTextContent("Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again.");
    expect(container).toHaveTextContent(NO_WORKER_RAN);
    expect(screen.getByRole("button", { name: "Dismiss: Rename returnTo to next" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Retry/ })).toBeNull();
  });
});

describe("the narrow card of a task that could not start", () => {
  it("reads kind, the task's name as the title, then the reason, and never the engine in the name's place", () => {
    const [one] = couldNotStartItems([card({ name: "Rename returnTo to next", reason: REFUSED, engine: "codex" })]);
    const { container } = render(<ul><NeedsCard item={one!} handlers={{ act: () => undefined }} /></ul>);
    const parts = [...container.querySelectorAll("li > span")].map((node) => node.textContent);
    expect(parts).toEqual(["Could not start", "Rename returnTo to next", REFUSED]);
    expect(container).not.toHaveTextContent("codex");
    expect(screen.getByRole("button", { name: "Retry on another engine: Rename returnTo to next" })).toBeVisible();
  });
});

describe("a task that could not start in the Needs you inbox", () => {
  it("is its own item: the reason, Retry on another engine and Dismiss, and no approval wording", async () => {
    const { list } = await open("crew-could-not-start");
    const rows = within(list).getAllByRole("listitem").filter((row) => row.getAttribute("data-kind") === "could_not_start");
    expect(rows).toHaveLength(2);
    for (const row of rows) {
      expect(row).toHaveTextContent("Could not start");
      expect(row).toHaveTextContent(REFUSED);
      expect(within(row).getByRole("button", { name: /^Retry on another engine/ })).toBeEnabled();
      expect(within(row).getByRole("button", { name: /^Dismiss/ })).toBeEnabled();
      expect(within(row).queryByRole("button", { name: /Approve|Deny/ })).toBeNull();
      expect(row.textContent).not.toMatch(/approv/i);
    }
    expect(screen.getByRole("button", { name: /^Could not start 2/ })).toBeVisible();
  });

  it("asks before it dismisses, then sends the cancel once and says nothing was started", async () => {
    const { user, list } = await open("crew-could-not-start");
    const row = within(list).getAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "could_not_start")!;
    await user.click(within(row).getByRole("button", { name: /^Dismiss/ }));
    expect(await screen.findByRole("heading", { name: "Dismiss this task?" })).toBeVisible();
    expect(posts()).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Dismiss task" }));
    await waitFor(() => expect(posts()).toHaveLength(1));
    expect(String(posts()[0]![0])).toMatch(/^\/api\/crew\/tasks\/b-refused[12]\/cancel$/);
  });

  it("retries through the retry route after a confirm", async () => {
    const { user, list } = await open("crew-could-not-start");
    const row = within(list).getAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "could_not_start")!;
    await user.click(within(row).getByRole("button", { name: /^Retry on another engine/ }));
    await user.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByText(/Its lead is asked to start it again/)).toBeVisible();
    expect(String(posts()[0]![0])).toMatch(/\/retry-elsewhere$/);
  });

  it("keeps Retry off with the service's reason when no other engine is allowed, and Dismiss still works", async () => {
    const { user, list } = await open("crew-could-not-start-no-engine");
    const row = within(list).getAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "could_not_start")!;
    await user.click(within(row).getByRole("button", { name: /^Retry on another engine/ }));
    await user.click(await screen.findByRole("button", { name: "Retry" }));
    const status = await screen.findByText(/No other engine is set up to run crew workers/);
    expect(status).toBeVisible();
    await waitFor(() => expect(within(row).getByRole("button", { name: /^Retry on another engine/ })).toBeDisabled());
    expect(within(row).getByRole("button", { name: /^Dismiss/ })).toBeEnabled();
  });

  it("leaves a task that waits on a real decision out of this kind", async () => {
    const { list } = await open("crew");
    expect(within(list).queryAllByRole("listitem").filter((row) => row.getAttribute("data-kind") === "could_not_start")).toHaveLength(0);
  });

  it("writes the disk sentence for a disk_low row in the inbox, with the muted line and Dismiss only", async () => {
    const { list } = await open("crew-could-not-start-disk");
    const rows = within(list).getAllByRole("listitem").filter((row) => row.getAttribute("data-kind") === "could_not_start");
    expect(rows).toHaveLength(2);
    for (const row of rows) {
      expect(row).toHaveTextContent("Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again.");
      expect(row).toHaveTextContent(NO_WORKER_RAN);
      expect(within(row).getByRole("button", { name: /^Dismiss/ })).toBeEnabled();
      expect(within(row).queryByRole("button", { name: /Retry/ })).toBeNull();
    }
  });

  it("names the source when the tasks that could not start cannot be read, and keeps the rest", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/crews", 1, 503, "unavailable");
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can’t read tasks that could not start. The rest is shown.")).toBeVisible();
  });
});
