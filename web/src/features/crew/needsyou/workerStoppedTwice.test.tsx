import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { useWrites } from "../writes/store";
import { NeedsYouInbox } from "./NeedsYouInbox";

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
const TITLE = "Stopped: its worker stopped twice";
const LINE = "The worker’s engine stopped with an error. It had already resumed once, so it did not resume again.";
const NOTE = "A task that resumed after a restart is not listed: it is running again. Its card on the Board says so.";
const HINT = "Dismiss hides this stop on this device.";
const KEY = "orbyn.needsyou.dismissedStops";

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  const view = render(<NeedsYouInbox />);
  const list = await screen.findByRole("list", { name: "Needs you" });
  const row = await waitFor(() => {
    const found = within(list).queryAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "stopped");
    expect(found).toBeDefined();
    return found!;
  });
  return { user, list, row, view };
}

describe("a task whose worker stopped twice after its one resume (kc8-resumed-inbox)", () => {
  it("is a Stops row with its task, the service's sentence, the hint and Dismiss alone", async () => {
    const { row } = await open("crew-worker-stopped-twice");
    expect(row).toHaveTextContent(TITLE);
    expect(row).toHaveTextContent("Task 7a3e91c");
    expect(row).toHaveTextContent(LINE);
    expect(row).toHaveTextContent(HINT);
    expect(within(row).getByRole("button", { name: `Dismiss: ${TITLE}` })).toBeEnabled();
    expect(within(row).queryByRole("button", { name: /Open/ })).toBeNull();
    expect(within(row).getAllByRole("button").map((one) => one.textContent)).toEqual(["Dismiss"]);
  });

  it("counts in the Stops chip and the inbox total", async () => {
    const { list } = await open("crew-worker-stopped-twice");
    const bar = screen.getByRole("group", { name: "Filter the inbox" });
    expect(within(bar).getByRole("button", { name: "Stops 1" })).toBeVisible();
    expect(within(bar).getByRole("button", { name: "Needs you 1" })).toBeVisible();
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  });

  it("says under the list that a task that resumed again is not listed", async () => {
    await open("crew-worker-stopped-twice");
    const note = screen.getByText(NOTE);
    expect(note.tagName).toBe("P");
    expect(note).toHaveClass("ny-note");
    expect(note).toBeVisible();
  });

  it("Dismiss hides the row on this device and a remount keeps it hidden", async () => {
    const { user, row, view } = await open("crew-worker-stopped-twice");
    await user.click(within(row).getByRole("button", { name: `Dismiss: ${TITLE}` }));
    await waitFor(() => expect(screen.queryByText(TITLE)).toBeNull());
    expect(posts()).toHaveLength(0);
    expect(JSON.parse(window.localStorage.getItem(KEY) ?? "[]")).toHaveLength(1);
    view.unmount();
    render(<NeedsYouInbox />);
    // The only row stays hidden, so the inbox is empty once the read has answered again.
    expect(await screen.findByText("Nothing needs you")).toBeVisible();
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url) === "/api/crew/needs-you")).toHaveLength(2));
    expect(screen.queryByText(TITLE)).toBeNull();
    expect(screen.queryByText(NOTE)).toBeNull();
    expect(JSON.parse(window.localStorage.getItem(KEY) ?? "[]")).toHaveLength(1);
  });

  it("says the design's sentence when the service's line is empty", async () => {
    const { row } = await open("crew-worker-stopped-twice-no-reason");
    expect(row).toHaveTextContent(LINE);
  });

  it("draws no row and no error when the shared Needs you read is not served", async () => {
    mock.reset("crew-worker-stopped-twice");
    mock.fail("/api/crew/needs-you", 1, 404, "not_found", "Not found.");
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Nothing needs you")).toBeVisible();
    expect(screen.queryByRole("list", { name: "Needs you" })).toBeNull();
    expect(screen.queryByText(TITLE)).toBeNull();
    expect(screen.queryByText(NOTE)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("draws no note in a scenario whose read carries no such item", async () => {
    mock.reset("crew");
    render(<NeedsYouInbox />);
    await screen.findByRole("list", { name: "Needs you" });
    expect(screen.queryByText(NOTE)).toBeNull();
  });
});
