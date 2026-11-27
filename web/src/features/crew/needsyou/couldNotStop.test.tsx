import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { MOCK_CREW } from "../../../../mock/flow";
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
const urls = () => vi.mocked(fetch).mock.calls.map(([url]) => String(url));
const TITLE = "Add tests for the redirect, including //evil.com";

async function open(scenario: "crew" | `crew-${string}`, options: { boardFails?: boolean } = {}) {
  mock.reset(scenario);
  // The row is described by its item alone, so a board that cannot be read changes nothing; the old read may be gone.
  if (options.boardFails) mock.fail("/api/crew/flow/board", 4, 503, "unavailable");
  const user = userEvent.setup();
  render(<NeedsYouInbox />);
  const list = await screen.findByRole("list", { name: "Needs you" });
  const row = await waitFor(() => {
    const found = within(list).queryAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "could_not_stop");
    expect(found).toBeDefined();
    return found!;
  });
  return { user, list, row };
}

describe("a task whose worker could not be stopped (kc2-close-lead-could-not-stop)", () => {
  it("is a row with the task's name, its engine and model, the service's sentence, Stop again and Open task, and no Dismiss", async () => {
    // The board is failing too: every one of those fields comes from the needs-you item.
    const { row } = await open("crew-could-not-stop", { boardFails: true });
    expect(row).toHaveTextContent(`Could not stop: ${TITLE}`);
    expect(row).toHaveTextContent("codex · GPT-5.5");
    expect(row).toHaveTextContent("The worker could not be stopped. Stop it again.");
    expect(within(row).getByRole("button", { name: `Stop again: ${TITLE}` })).toBeEnabled();
    expect(within(row).getByRole("button", { name: `Open task: ${TITLE}` })).toBeEnabled();
    expect(within(row).queryByRole("button", { name: /Dismiss/ })).toBeNull();
    expect(screen.getByRole("button", { name: /^Could not stop 1/ })).toBeVisible();
  });

  it("Stop again sends the person's cancel for that task once, and says what the service answered", async () => {
    const { user, row } = await open("crew-could-not-stop");
    await user.click(within(row).getByRole("button", { name: `Stop again: ${TITLE}` }));
    await waitFor(() => expect(posts()).toHaveLength(1));
    expect(String(posts()[0]![0])).toBe("/api/crew/tasks/b-verifying/cancel");
    expect(await screen.findByText(/^Cancelled\./)).toBeVisible();
  });

  it("shows the service's own words when the worker still could not be stopped, and keeps the row", async () => {
    mock.reset("crew-could-not-stop");
    mock.fail("/api/crew/tasks/b-verifying/cancel", 1, 502, "engine_unavailable", "The task is cancelled, but its worker could not be stopped. Try again.", "POST");
    const user = userEvent.setup();
    render(<NeedsYouInbox />);
    const list = await screen.findByRole("list", { name: "Needs you" });
    const row = await waitFor(() => within(list).getAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "could_not_stop")!);
    await user.click(within(row).getByRole("button", { name: `Stop again: ${TITLE}` }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The task is cancelled, but its worker could not be stopped. Try again.");
    expect(within(list).getByRole("button", { name: `Stop again: ${TITLE}` })).toBeEnabled();
  });

  it("Open task opens the task on the Flow view", async () => {
    const { user, row } = await open("crew-could-not-stop");
    await user.click(within(row).getByRole("button", { name: `Open task: ${TITLE}` }));
    expect(window.location.hash).toBe("#/crew/flow/task/b-verifying");
  });

  it("keeps Stop again off from another computer, with the reason", async () => {
    useConnection.setState({ remote: true });
    const { row } = await open("crew-could-not-stop");
    const stop = within(row).getByRole("button", { name: `Stop again: ${TITLE}` });
    expect(stop).toBeDisabled();
    expect(stop).toHaveAttribute("title", "Not available from another computer");
  });

  it("names an item with no name by its id, never by the board's card", async () => {
    // The board still has a card named "Add tests for the redirect, including //evil.com"; the item has no name, so the id wins.
    const { row } = await open("crew-could-not-stop-no-name");
    expect(row).toHaveTextContent("Could not stop: Task b-verifying");
    expect(within(row).getByRole("button", { name: "Stop again: Task b-verifying" })).toBeEnabled();
    expect(within(row).getByRole("button", { name: "Open task: Task b-verifying" })).toBeEnabled();
  });

  it("makes no board read of its own for a stopped task", async () => {
    await open("crew-could-not-stop");
    // The could-not-start source reads the Needs you column; wait for that read, then look at every board read sent.
    await waitFor(() => expect(urls().some((url) => url.startsWith("/api/crew/flow/board"))).toBe(true));
    const boards = urls().filter((url) => url.startsWith("/api/crew/flow/board"));
    expect(boards.filter((url) => !url.includes("column=needs_you"))).toEqual([]);
    expect(urls()).not.toContain(`/api/crew/flow/board?crew=${MOCK_CREW}`);
  });

  it("keeps the could-not-stop row when the crew list that names the watches cannot be read", async () => {
    mock.reset("crew-could-not-stop-and-watch");
    mock.fail("/api/crew/background", 8, 503, "unavailable");
    render(<NeedsYouInbox />);
    const list = await screen.findByRole("list", { name: "Needs you" });
    const row = await waitFor(() => within(list).getAllByRole("listitem").find((one) => one.getAttribute("data-kind") === "could_not_stop")!);
    expect(within(row).getByRole("button", { name: /^Stop again/ })).toBeEnabled();
  });

  it("draws no row when the shared Needs you read is not served: an older service never sends the kind", async () => {
    mock.reset("crew-could-not-stop");
    mock.fail("/api/crew/needs-you", 1, 404, "not_found", "Not found.");
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Nothing needs you")).toBeVisible();
    expect(screen.queryByRole("list", { name: "Needs you" })).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("draws no row in a scenario whose read carries no such item", async () => {
    mock.reset("crew");
    render(<NeedsYouInbox />);
    const list = await screen.findByRole("list", { name: "Needs you" });
    expect(within(list).queryAllByRole("listitem").some((one) => one.getAttribute("data-kind") === "could_not_stop")).toBe(false);
  });
});
