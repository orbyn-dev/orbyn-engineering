import { act, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { MOCK_CREW } from "../../../../mock/flow";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { BoardView } from "./BoardView";
import { useBoard } from "./useBoard";

let mock: MockApi;
beforeEach(() => {
  // The card says how long ago a task changed: the clock stands at the moment the sample was taken.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-07T15:00:00Z"));
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
});

afterEach(() => vi.useRealTimers());

const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");
const urls = () => vi.mocked(fetch).mock.calls.map(([url]) => String(url));

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<BoardView />);
  await screen.findByRole("region", { name: /^Queued/ });
  return user;
}
const column = (name: RegExp) => screen.getByRole("region", { name });

describe("the Taskboard on the board route", () => {
  it("draws nothing and reads no board route while Crew Preview is off (the default scenario answers 404)", async () => {
    mock.reset("default");
    const { container } = render(<BoardView />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    expect(calls("/api/crew/flow")).toHaveLength(0);
  });

  it("draws the seven columns with their real counts and the work tasks as cards", async () => {
    await open("crew");
    const names = screen.getAllByRole("region").map((one) => one.getAttribute("aria-label"));
    expect(names.filter((name) => /,/.test(name ?? ""))).toEqual(["Queued, 1", "Routed, 1", "Running, 1", "Verifying, 1", "Needs you, 1", "Passed, 3", "Stopped · Cancelled, 2"]);
    const queued = within(column(/^Queued/));
    expect(queued.getByText("Update the login docs for the new redirect rule")).toBeVisible();
    // Not routed yet: no engine, no model.
    expect(queued.getByText("Not routed yet")).toBeVisible();
    expect(queued.getByText("docs")).toBeVisible();
    expect(queued.getByText(/updated 1m ago/)).toBeVisible();
    const running = within(column(/^Running/));
    expect(running.getByText("claude")).toBeVisible();
    expect(running.getByText("Sonnet 5.5 · Medium")).toBeVisible();
    expect(running.getByText("owner Lead")).toBeVisible();
    expect(within(column(/^Routed/)).getByText("Sonnet 5.5 · Low")).toBeVisible();
    // No checks bar yet (After first board), and no lease: the route carries neither.
    expect(document.body.textContent).not.toMatch(/of \d+ checks|read only/);
  });

  it("opens Passed and Stopped as rails, and a header opens or closes its column", async () => {
    const user = await open("crew");
    const passed = within(column(/^Passed/));
    expect(passed.getByRole("button", { name: /Passed/ })).toHaveAttribute("aria-expanded", "false");
    expect(passed.queryByText("Fix the typo in the README")).toBeNull();
    await user.click(passed.getByRole("button", { name: /Passed/ }));
    expect(within(column(/^Passed/)).getByText("Fix the typo in the README")).toBeVisible();
    // A task whose effort is not Low or Medium shows no effort.
    expect(within(column(/^Passed/)).getByText("GPT-5.5")).toBeVisible();
    await user.click(within(column(/^Passed/)).getByRole("button", { name: /Passed/ }));
    expect(within(column(/^Passed/)).queryByText("Fix the typo in the README")).toBeNull();
  });

  it("shows a needs_person card's reason read-only, as plain text, with no Answer button", async () => {
    await open("crew");
    const needs = within(column(/^Needs you/));
    expect(needs.getByText("Asked you a question: <b>which target wins</b>?")).toBeVisible();
    expect(column(/^Needs you/).querySelector(".bd-amber b")).toBeNull();
    expect(needs.queryByRole("button", { name: /Answer/ })).toBeNull();
  });

  it("filters by engine and by kind with reads, and the chips keep the whole crew's counts", async () => {
    const user = await open("crew");
    const bar = screen.getByRole("group", { name: "Filter the board" });
    expect(within(bar).getAllByRole("button").map((one) => one.textContent)).toEqual(["All 10", "claude 4", "codex 4", "opencode 1", "implement 4", "docs 3", "test 2", "research 1"]);
    await user.click(within(bar).getByRole("button", { name: "codex 4" }));
    await waitFor(() => expect(urls().some((url) => url.includes("engine=codex"))).toBe(true));
    await waitFor(() => expect(within(screen.getByRole("group", { name: "Filter the board" })).getByRole("button", { name: "codex 4" })).toHaveAttribute("aria-pressed", "true"));
    await waitFor(() => expect(screen.getByRole("region", { name: "Running, 0" })).toBeVisible());
    expect(within(screen.getByRole("group", { name: "Filter the board" })).getByRole("button", { name: "All 10" })).toBeVisible();
    await user.click(within(screen.getByRole("group", { name: "Filter the board" })).getByRole("button", { name: "All 10" }));
    await waitFor(() => expect(screen.getByRole("region", { name: "Running, 1" })).toBeVisible());
    expect(writes()).toHaveLength(0);
  });

  it("adds up: the kind chips are the total, and the engine chips are the routed tasks", async () => {
    await open("crew");
    const chips = within(screen.getByRole("group", { name: "Filter the board" })).getAllByRole("button").map((one) => one.textContent ?? "");
    const number = (text: string) => Number(/(\d+)$/.exec(text)![1]);
    const total = number(chips[0]!);
    const engines = chips.filter((text) => /^(codex|claude|opencode)/.test(text)).map(number);
    const kinds = chips.filter((text) => /^(implement|docs|test|research)/.test(text)).map(number);
    expect(kinds.reduce((a, b) => a + b, 0)).toBe(total);
    // The one task not routed yet is in no engine chip.
    expect(engines.reduce((a, b) => a + b, 0)).toBe(total - 1);
  });

  it("pages a column with +N more, from the real total, as reads", async () => {
    const user = await open("crew-board-many");
    const running = () => within(column(/^Running/));
    expect(running().getAllByRole("listitem")).toHaveLength(8);
    expect(screen.getByRole("region", { name: "Running, 12" })).toBeVisible();
    await user.click(running().getByRole("button", { name: "+4 more" }));
    await waitFor(() => expect(running().getAllByRole("listitem")).toHaveLength(12));
    expect(running().queryByRole("button", { name: /more$/ })).toBeNull();
    expect(urls().some((url) => url.includes("column=running") && url.includes("offset=8"))).toBe(true);
    expect(writes()).toHaveLength(0);
  });

  it("says there are no tasks yet, with no column drawn", async () => {
    mock.reset("crew-board-empty");
    render(<BoardView />);
    expect(await screen.findByText("No tasks yet")).toBeVisible();
    expect(screen.getByText("Cards are briefs the lead writes; a person can ask the lead for one.")).toBeVisible();
    expect(screen.queryByRole("region", { name: /^Queued/ })).toBeNull();
  });

  it("says the list was cut at the service's cap when it is truncated", async () => {
    await open("crew-board-truncated");
    expect(screen.getByRole("note")).toHaveTextContent("This crew has more tasks than one read covers; the newest are shown.");
  });

  it("names an unreadable service and retries with reads only", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/crews", 1);
    const user = userEvent.setup();
    render(<BoardView />);
    expect(await screen.findByText("Can’t read the board")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("region", { name: /^Queued/ })).toBeVisible();
    expect(writes()).toHaveLength(0);
  });

  it("names a board that cannot be read and retries it", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/board", 1);
    const user = userEvent.setup();
    render(<BoardView />);
    expect(await screen.findByText("Can’t read the board")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("region", { name: /^Queued/ })).toBeVisible();
    expect(writes()).toHaveLength(0);
  });

  it("draws the card menu with Cancel as a choice for a running task and the rest After the beta and inert", async () => {
    const user = await open("crew");
    await user.click(within(column(/^Running/)).getByRole("button", { name: /^Actions for Use next/ }));
    const menu = await screen.findByRole("menu", { name: "Task actions" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual([
      "Cancel",
      "Reassign engine After the beta",
      "Message into a lane After the beta",
      "Add reviewer lane After the beta",
      "Propose a check After the beta",
    ]);
    expect(items[0]).toBeEnabled();
    for (const item of items.slice(1)) expect(item).toBeDisabled();
    // Opening the menu sends nothing.
    expect(writes()).toHaveLength(0);
  });

  it("offers Re-run checks, inert, on a task whose checks may run again, and Answer only on one that needs you", async () => {
    const user = await open("crew");
    await user.click(within(column(/^Needs you/)).getByRole("button", { name: /^Actions for Which target/ }));
    const menu = await screen.findByRole("menu", { name: "Task actions" });
    expect(within(menu).getByRole("menuitem", { name: /Answer a worker’s question/ })).toBeDisabled();
    expect(within(menu).getByRole("menuitem", { name: /Cancel/ })).toBeEnabled();
    expect(within(menu).queryByRole("menuitem", { name: /Re-run checks/ })).toBeNull();
  });
});

describe("useBoard, the next page of a column", () => {
  it("sends one read when showMore is called twice before the first answer", async () => {
    mock.reset("crew-board-many");
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("column=running")) await new Promise((resolve) => setTimeout(resolve, 60));
      return real(input, init);
    });
    const { result } = renderHook(() => useBoard({ crew: MOCK_CREW, engine: null, kind: null }));
    await waitFor(() => expect(result.current.cardsOf("running")).toHaveLength(8));
    await act(async () => {
      void result.current.showMore("running");
      void result.current.showMore("running");
    });
    await waitFor(() => expect(result.current.cardsOf("running")).toHaveLength(12));
    expect(calls("column=running")).toHaveLength(1);
  });
});
