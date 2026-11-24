import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../generated/Project";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { BackgroundTab } from "./Background";
import { WATCH_SCREENS } from "./watchScreens";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project], loading: false, error: null } });
});

const crewsTable = () => screen.getByRole("table", { name: "Background crews" });
const rowOf = (name: string) => within(crewsTable()).getByText(name, { selector: "b" }).closest('[role="row"]') as HTMLElement;
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");

/** Opens the Background tab on a watch scenario and waits for its list. */
async function openBackground(scenario: string) {
  mock.reset(scenario as Parameters<MockApi["reset"]>[0]);
  const user = userEvent.setup();
  render(<BackgroundTab />);
  await screen.findByRole("table", { name: "Background crews" });
  return user;
}

describe("watches in the Background list", () => {
  it("lists a watch beside the other crews with its command, its schedule and action, and its state, in the same columns", async () => {
    await openBackground("crew-background-watch");
    expect(within(crewsTable()).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Crew", "Starts when", "Last run or check", "State", "Switch"]);
    const chip = (name: string) => [...rowOf(name).querySelectorAll(".ap-chip")].map((one) => one.textContent);
    expect(rowOf("Redirect tests")).toHaveTextContent("Watch · no model · npm test web/nav/redirect");
    expect(rowOf("Redirect tests")).toHaveTextContent("Every 1 hour · raises Needs you");
    expect(rowOf("Redirect tests")).toHaveTextContent(/Checked .* · exit 2 · failing since /);
    expect(chip("Redirect tests")).toEqual(["Failing"]);
    expect(rowOf("Main build")).toHaveTextContent("Every 1 hour · starts Fix failing CI");
    expect(chip("Main build")).toEqual(["Passing"]);
    expect(chip("Lockfile check")).toEqual(["Could not run"]);
    expect(chip("Docs links")).toEqual(["Paused"]);
    expect(within(rowOf("Docs links")).getByRole("switch", { name: "Docs links on" })).toHaveAttribute("aria-checked", "false");
    // The designer's ruling: the screens no longer say triggers are planned, and no watch row claims Planned.
    expect(screen.queryByText("Not built yet")).toBeNull();
    expect(screen.queryByText("Triggers not built yet")).toBeNull();
  });

  it("opens a watch's page from its row, with the state, the facts a check keeps, the redacted tail and only the changes of result", async () => {
    const user = await openBackground("crew-background-watch-failing");
    await user.click(screen.getByRole("button", { name: "Open the Redirect tests watch" }));
    const panel = await screen.findByRole("region", { name: "Redirect tests watch" });
    expect(within(panel).getByText("Failing: raised in Needs you")).toBeVisible();
    expect(within(panel).getByRole("button", { name: "Open in Needs you" })).toBeVisible();
    expect(within(panel).getByText("npm test web/nav/redirect")).toBeVisible();
    expect(within(panel).getByText("orbyn-engineering · this device · the project’s checkout root")).toBeVisible();
    expect(within(panel).getByText("Every 1 hour · no network · 5 minutes at most")).toBeVisible();
    expect(within(panel).getByText("Raise Needs you")).toBeVisible();
    expect(within(panel).getByText("None. A check makes no model call.")).toBeVisible();
    // One output box: the page draws the latest tail, never the history frame's change detail as a second box.
    expect(within(panel).getAllByText(/Latest output · the check at \d\d:\d\d · exit 1/)).toHaveLength(1);
    expect(within(panel).getByText(/FAIL web\/nav\/redirect.test.ts/)).toBeVisible();
    expect(within(panel).queryByLabelText("Change detail")).toBeNull();
    const changes = within(panel).getByRole("table", { name: "Watch changes" });
    const when = expect.stringMatching(/^\w{3} \d{1,2} \w{3}, \d\d:\d\d$/);
    expect(within(changes).getAllByRole("row").slice(1).map((row) => within(row).getAllByRole("cell").map((cell) => cell.textContent))).toEqual([
      [when, "Passing → Failing", "1", "Raised in Needs you."],
      [when, "Passing again", "0", expect.stringMatching(/^Resolved the Needs you row from /)],
      [when, "Passing → Failing", "1", "Raised in Needs you."],
      [when, "Baseline: Passing", "0", "Recorded. A first check never raises."],
    ]);
  });

  it("turns a watch off from its page through the route, and keeps the page on what the read answered", async () => {
    const user = await openBackground("crew-background-watch-passing");
    await user.click(screen.getByRole("button", { name: "Open the Redirect tests watch" }));
    await screen.findByRole("region", { name: "Redirect tests watch" });
    await user.click(screen.getByRole("button", { name: "Turn off" }));
    await waitFor(() => expect(writes().map(([url]) => String(url))).toContain("/api/crew/background/bg-watch-redirect/disable"));
    expect(await screen.findByRole("button", { name: "Turn on" })).toBeVisible();
  });

  it("draws a watch's page only when its own read answered: a 404 leaves the crew's plain row and no watch page", async () => {
    mock.reset("crew-background-watch-passing");
    mock.fail("/api/crew/background/bg-watch-redirect/watch", 1, 404, "not_found", "Not found.");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Background crews" });
    expect(screen.queryByRole("button", { name: "Open the Redirect tests watch" })).toBeNull();
    const row = rowOf("Redirect tests");
    expect(row).toHaveTextContent("Template: fix failing CI");
    expect(within(row).queryByText("Passing")).toBeNull();
  });

  it("offers Open the run for a watch that started a template run, once the read holds that run", async () => {
    const user = await openBackground("crew-background-watch-template");
    await user.click(screen.getByRole("button", { name: "Open the Main build watch" }));
    const panel = await screen.findByRole("region", { name: "Main build watch" });
    expect(within(panel).getByText("Failing: started Fix failing CI")).toBeVisible();
    expect(within(panel).getByRole("button", { name: "Open the run" })).toBeVisible();
  });

  it("a paused watch says no check runs and what the next check is compared with", async () => {
    const user = await openBackground("crew-background-watch-paused");
    await user.click(screen.getByRole("button", { name: "Open the Docs links watch" }));
    const panel = await screen.findByRole("region", { name: "Docs links watch" });
    expect(within(panel).getByText("Paused: it is off")).toBeVisible();
    expect(within(panel).getByText(/No check runs while it is off\./)).toBeVisible();
    expect(within(panel).getByText(/compared with that last result/)).toBeVisible();
    expect(within(panel).getByRole("button", { name: "Turn on" })).toBeVisible();
    expect(within(panel).getByRole("table", { name: "Watch changes" })).toHaveTextContent("Turned off by you");
  });

  it("a check that could not run is its own page, with no tail and its own words", async () => {
    const user = await openBackground("crew-background-watch-could-not-run");
    await user.click(screen.getByRole("button", { name: "Open the Lockfile check watch" }));
    const panel = await screen.findByRole("region", { name: "Lockfile check watch" });
    expect(within(panel).getByText("Could not run: timed out")).toBeVisible();
    expect(within(panel).getByText(/This is not a failing result\./)).toBeVisible();
    expect(within(panel).getByText("3 checks in a row")).toBeVisible();
    expect(within(panel).getByText("Next check")).toBeVisible();
    expect(within(panel).queryByText(/Latest output/)).toBeNull();
  });

  it("a watch that just passed again says so, names the row it resolved, and keeps its changes", async () => {
    const user = await openBackground("crew-background-watch-recovered");
    await user.click(screen.getByRole("button", { name: "Open the Redirect tests watch" }));
    const panel = await screen.findByRole("region", { name: "Redirect tests watch" });
    expect(within(panel).getByRole("heading", { name: "Passing again" })).toBeVisible();
    expect(within(panel).getByText(/It was failing from /)).toBeVisible();
    expect(within(panel).getByRole("table", { name: "Watch changes" })).toHaveTextContent("Passing again");
  });

  it("a failing baseline reads Failing, says a first failing check never raises, and offers no Needs you action", async () => {
    const user = await openBackground("crew-background-watch-failing-baseline");
    await user.click(screen.getByRole("button", { name: "Open the Redirect tests watch" }));
    const panel = await screen.findByRole("region", { name: "Redirect tests watch" });
    expect(within(panel).getByRole("heading", { name: "Failing: baseline" })).toBeVisible();
    expect(within(panel).getByText(/first failing check never raises/)).toBeVisible();
    expect(within(panel).queryByRole("button", { name: "Open in Needs you" })).toBeNull();
    expect(within(panel).getByText(/FAIL web\/nav\/redirect.test.ts/)).toBeVisible();
  });

  it("a quiet pass says nothing is raised and names the baseline", async () => {
    const user = await openBackground("crew-background-watch-passing");
    await user.click(screen.getByRole("button", { name: "Open the Redirect tests watch" }));
    const panel = await screen.findByRole("region", { name: "Redirect tests watch" });
    expect(within(panel).getByRole("heading", { name: "Passing" })).toBeVisible();
    expect(within(panel).getByText(/nothing is raised and no run starts/)).toBeVisible();
    expect(within(panel).getByText(/was a baseline: recorded, never raised/)).toBeVisible();
  });
});

describe("the watch history frame", () => {
  it("draws the newest kept tail as the change detail under the table, where a watch's page draws no second box", async () => {
    mock.reset("crew-background-watch-failing");
    const Screen = WATCH_SCREENS["watch-history"];
    render(<Screen />);
    expect(await screen.findByText(/Change detail ·/)).toBeVisible();
    expect(screen.getByText(/FAIL web\/nav\/redirect.test.ts/)).toBeVisible();
    // The page test above shows the same read on the watch's page: one Latest output box and no change detail there.
    expect(screen.queryByText(/Latest output/)).toBeNull();
  });

  it("a passing watch carries no note about checks that found the same result: that sentence is the history's, and only a failing watch's last check is not listed", async () => {
    mock.reset("crew-background-watch-passing");
    const Screen = WATCH_SCREENS["watch-history"];
    render(<Screen />);
    await screen.findByLabelText("Redirect tests watch changes");
    expect(screen.queryByText(/Checks that found the same result as the one before are not listed/)).toBeNull();
  });
});
