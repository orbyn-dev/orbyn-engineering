import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../generated/Project";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { BackgroundTab } from "./Background";
import { useWatchFocus } from "./watchFocus";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project], loading: false, error: null } });
});

/** The fact rows one panel draws, as the design's label-and-value pairs, in their order. */
const factsOf = (panel: HTMLElement) =>
  [...panel.querySelectorAll("dl.ap-facts > div")].map((row) => [row.querySelector("dt")?.textContent, row.querySelector("dd")?.textContent]);

/** Opens the Background tab on a scenario and the watch page of the crew it holds. */
async function openWatch(scenario: string, name = "Main build") {
  mock.reset(scenario as Parameters<MockApi["reset"]>[0]);
  const user = userEvent.setup();
  render(<BackgroundTab />);
  await screen.findByRole("table", { name: "Background crews" });
  await user.click(screen.getByRole("button", { name: `Open the ${name} watch` }));
  return { user, panel: await screen.findByRole("region", { name: `${name} watch` }) };
}

/** The exact sentence core keeps for a refused template start, code by code (docs/crew.md, crates/orbyn-core/src/crew/watch.rs). */
const REFUSED: string[] = [
  "Could not start Fix failing CI: Not started: not enough free space. Free space and try again.",
  "Could not start Fix failing CI: No engine is set up for this template. Choose one in Settings.",
  "Could not start Fix failing CI: This folder is not trusted yet. Trust it in Settings.",
  "Could not start Fix failing CI: A run of this crew is already running.",
  "Could not start Fix failing CI: Your settings do not allow this start.",
  "Could not start Fix failing CI: This watch no longer exists.",
  "Could not start Fix failing CI: The start was refused. Open the run list for details.",
];

describe("a background run's own page", () => {
  it("draws the eight rows of its one read, with no log, tail, cost or action", async () => {
    const { user } = await openWatch("crew-background-run-detail");
    await user.click(screen.getByRole("button", { name: "Open the run" }));
    const run = await screen.findByRole("region", { name: "Main build run" });
    const time = expect.stringMatching(/^\w{3} \d{1,2} \w{3}, \d\d:\d\d$/);
    expect(factsOf(run)).toEqual([
      ["Crew", "Main build"],
      ["Template", "Fix failing CI"],
      ["Ran at", time],
      ["Ended", time],
      ["State", "Stopped · Budget"],
      ["Spent", "40 of 40 turns · 31 of 60 min"],
      ["Review waiting", "Yes"],
      ["Crew id", "bg-7f3a21"],
    ]);
    // The header chip and the State row carry the table's own words; the crew id is the read's own value.
    expect(within(run).getByText("Stopped · Budget", { selector: ".ap-chip" })).toHaveClass("ap-chip-am");
    expect(within(run).getByText("bg-7f3a21")).toHaveClass("ap-mono");
    // Only the run's facts: no log, tail, table, cost or action.
    expect(run.querySelector("pre")).toBeNull();
    expect(within(run).queryByRole("table")).toBeNull();
    expect(within(run).queryByText(/Latest output|Change detail|Cost/)).toBeNull();
    expect(within(run).getAllByRole("button").map((button) => button.textContent)).toEqual(["Recent runs"]);
    // The page drew the read, not something else: one run read for the newest run the watch's list carried.
    expect(vi.mocked(fetch).mock.calls.map(([input]) => String(input))).toContain("/api/crew/background/runs/r-main-1");
  });

  it("opens the run a change started and Recent runs goes back to the list", async () => {
    const { user } = await openWatch("crew-background-run-detail");
    await user.click(screen.getByRole("button", { name: "Open the run" }));
    const run = await screen.findByRole("region", { name: "Main build run" });
    expect(within(run).getByText("bg-7f3a21")).toBeVisible();
    expect(within(run).getByText("40 of 40 turns · 31 of 60 min")).toBeVisible();
    await user.click(within(run).getByRole("button", { name: "Recent runs" }));
    expect(await screen.findByRole("table", { name: "Background crews" })).toBeVisible();
    expect(screen.queryByRole("region", { name: "Main build run" })).toBeNull();
  });

  it("leaves a run's page when another project is picked, and shows that project's list", async () => {
    const { user } = await openWatch("crew-background-run-detail");
    await user.click(screen.getByRole("button", { name: "Open the run" }));
    await screen.findByRole("region", { name: "Main build run" });
    const app = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;
    const other = { id: "p-two", name: "other-repo", trusted: true, is_git: true, path: "/fixture/two" } as Project;
    useWorkspace.setState({ projects: { data: [app, other], loading: false, error: null } });
    // Another project's watch is asked for (a Needs you row): the page moves to that project, whose list it shows.
    act(() => { useWatchFocus.getState().ask("bg-other01", "p-two"); });
    await screen.findByRole("table", { name: "Background crews" });
    expect(screen.queryByRole("region", { name: "Main build run" })).toBeNull();
  });

  it("offers no Open the run for a watch whose runs read holds none", async () => {
    const { panel } = await openWatch("crew-background-watch-refused");
    expect(within(panel).queryByRole("button", { name: "Open the run" })).toBeNull();
  });

  it("draws nothing of a run the read does not have, and the list stays", async () => {
    mock.reset("crew-background-run-detail");
    mock.fail("/api/crew/background/runs/r-main-1", 1, 404, "not_found", "No such background run.");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Background crews" });
    await user.click(screen.getByRole("button", { name: "Open the Main build watch" }));
    await screen.findByRole("region", { name: "Main build watch" });
    await user.click(screen.getByRole("button", { name: "Open the run" }));
    // The read's not_found draws no run page: the honest fallback is the list, with its own rows unchanged.
    expect(await screen.findByRole("table", { name: "Background crews" })).toBeVisible();
    expect(screen.queryByRole("region", { name: "Main build run" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Background run" })).toBeNull();
    await waitFor(() => expect(screen.getByRole("table", { name: "Recent runs" })).toBeVisible());
    expect(screen.getByRole("table", { name: "Recent runs" })).toHaveTextContent("40 of 40 turns · 31 of 60 min");
  });
});

describe("a refused template start's sentence on the change row", () => {
  it("draws each of the seven codes as core's own sentence, in the service's order", async () => {
    const { panel } = await openWatch("crew-background-watch-refused");
    const changes = within(panel).getByRole("table", { name: "Watch changes" });
    expect(within(changes).getAllByRole("row").slice(1).map((row) => within(row).getAllByRole("cell")[3]?.textContent)).toEqual(REFUSED);
  });

  it("keeps Raised in Needs you. for a change that kept no refusal", async () => {
    const { panel } = await openWatch("crew-background-watch-failing", "Redirect tests");
    const changes = within(panel).getByRole("table", { name: "Watch changes" });
    expect(within(changes).getAllByRole("row").slice(1).map((row) => within(row).getAllByRole("cell")[3]?.textContent)).toEqual([
      "Raised in Needs you.",
      expect.stringMatching(/^Resolved the Needs you row from /),
      "Raised in Needs you.",
      "Recorded. A first check never raises.",
    ]);
  });
});
