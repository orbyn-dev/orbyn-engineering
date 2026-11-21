import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SAMPLE_PROJECTS, createCrewDashboardMock, sampleHistory } from "../../../../mock/crewDashboard";
import { ApiFailure } from "../../../api/client";
import { Dashboard } from "./Dashboard";
import type { DashboardClient, DashboardHistory } from "./contract";
import { ALL } from "./prefs";

const tile = (label: string) => document.querySelector<HTMLElement>(`[data-tile="${label}"]`)!;
const fetchSpy = vi.fn();

beforeEach(() => {
  window.localStorage.clear();
  vi.stubGlobal("fetch", fetchSpy);
});
afterEach(() => {
  fetchSpy.mockReset();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const show = (props: Partial<Parameters<typeof Dashboard>[0]> = {}, client: DashboardClient = createCrewDashboardMock()) =>
  render(<Dashboard sample projects={SAMPLE_PROJECTS} client={client} {...props} />);

describe("one repository on the history read", () => {
  it("draws the four tiles with the sample under each", async () => {
    show();
    await screen.findByText("71%");
    expect(tile("Autonomy")).toHaveTextContent("merged without your action");
    expect(tile("Autonomy")).toHaveTextContent("27 of 38 merges · last 60 days");
    expect(tile("Cycle time")).toHaveTextContent("41 minmedian");
    expect(tile("Cycle time")).toHaveTextContent("90th percentile2 h 05 min");
    expect(tile("Cycle time")).toHaveTextContent("Fastest6 min");
    expect(tile("Cycle time")).toHaveTextContent("Slowest3 h 40 min");
    expect(tile("Cost")).toHaveTextContent("claude6.1M tokens");
    expect(tile("Cost")).toHaveTextContent("each in its own unit, never added up");
    expect(tile("Revert rate")).toHaveTextContent("5%");
    expect(tile("Revert rate")).toHaveTextContent("2 of 38 merges reverted");
    expect(tile("Revert rate")).toHaveTextContent("1 reverted automatically, 1 by you · 38 merges");
    expect(screen.getByText("Sample data")).toBeInTheDocument();
    expect(screen.getByText("This device only")).toBeInTheDocument();
  });

  it("never adds the engines' costs into one figure", async () => {
    const { container } = show();
    await screen.findByText("71%");
    expect(container.textContent).not.toContain("11.8M");
    expect(container.textContent).not.toMatch(/\$\d/);
  });

  it("says what the bars are and gives each week's merges", async () => {
    show();
    await screen.findByText("71%");
    const bars = screen.getAllByRole("img", { name: /Merges per week/ });
    expect(bars.length).toBeGreaterThanOrEqual(3);
    expect(bars[0]).toHaveAttribute("aria-label", expect.stringContaining("week 1: 3 merges, 2 needed nothing from you"));
  });

  it("shows engine and area rows that add up, with a rate only from 10 merges", async () => {
    show();
    await screen.findByText("71%");
    const text = document.body.textContent ?? "";
    expect(text).toContain("claude · Sonnet 5.5");
    expect(text).toContain("76% merged without you · 16 of 21");
    expect(text).toContain("3 of 5 merged without you · too few for a rate");
    expect(text).toContain("0 of 5");
    expect(text).toContain("web/auth");
    expect(text).toContain("53% merged without you · 8 of 15");
    expect(text).toContain("6 of 9 merged without you · too few for a rate");
    expect(text).toContain("claude’s own unit");
  });

  it("shows Autopilot by area: the mode each area is on now and the changes with their reasons", async () => {
    show();
    await screen.findByText("Autopilot by area");
    const changes = await screen.findByTestId("mode-changes");
    expect(changes).toHaveTextContent("web/auth");
    expect(changes).toHaveTextContent("Full → On");
    expect(changes).toHaveTextContent("2 reverts in 14 days, in 9 merges");
    expect(document.body.textContent).toContain("Repo ceiling: Full");
    expect(document.body.textContent).toContain("Not measured");
    expect(document.body.textContent).toContain("3 merges, below the 5 needed");
    expect(screen.getByRole("link", { name: "Open Autopilot by area" })).toHaveAttribute("href", "#/settings/crew/areas?repo=orbyn-engineering");
  });

  it("opens one metric by week and goes back", async () => {
    const user = userEvent.setup();
    show();
    await screen.findByText("71%");
    await user.click(screen.getByRole("button", { name: "Open revert rate by week" }));
    expect(screen.getByText("Reverted merges by week")).toBeInTheDocument();
    expect(screen.getByText("Rotate the signing key")).toBeInTheDocument();
    expect(screen.getByText("Reverted by you")).toBeInTheDocument();
    expect(document.body.textContent).toContain("Reverted automatically · cause certain");
    expect(document.body.textContent).toContain("Weeks with fewer than 10 merges are not turned into percentages.");
    await user.click(screen.getByRole("button", { name: "All metrics" }));
    expect(await screen.findByText("71%")).toBeInTheDocument();
  });

  it("changes the period and reads again for it", async () => {
    const user = userEvent.setup();
    const client = createCrewDashboardMock();
    show({}, client);
    await screen.findByText("71%");
    await user.click(screen.getByRole("radio", { name: "30 days" }));
    await waitFor(() => expect(client.reads).toContain("history:orbyn-engineering:30"));
    expect(screen.getByRole("radio", { name: "30 days" })).toHaveAttribute("aria-checked", "true");
    await waitFor(() => expect(tile("Autonomy")).toHaveTextContent("last 30 days"));
  });

  it("makes no request of its own", async () => {
    show();
    await screen.findByText("71%");
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("a repository with few or no merges", () => {
  it("says how many merges each tile rests on and lists them, with no percentage", async () => {
    show({ initial: { project: "orbyn-planner" } });
    await screen.findByText(/This repo has 5 merges in the last 60 days/);
    expect(tile("Autonomy")).toHaveTextContent("3 of 5");
    expect(tile("Autonomy")).toHaveTextContent("5 merges · too few for a rate");
    expect(tile("Cycle time")).toHaveTextContent("55 minmedian");
    expect(tile("Revert rate")).toHaveTextContent("0 of 5");
    expect(tile("Revert rate")).toHaveTextContent("5 merges · too few for a rate");
    expect(tile("Autonomy").textContent).not.toContain("%");
    const list = screen.getByTestId("recent-merges");
    expect(list).toHaveTextContent("Update the calendar feed tokenclaude · merged without you");
    expect(list).toHaveTextContent("Fix the flaky E2E shardcodex · you answered a question");
    expect(list).toHaveTextContent("Add a seed scriptcodex · you approved the request");
    expect(within(list).getAllByText("Passed")).toHaveLength(5);
  });

  it("shows no zeros as results when there are no merges", async () => {
    show({ initial: { project: "orbyn-engine" } });
    expect(await screen.findByText("No merges yet")).toBeInTheDocument();
    expect(document.body.textContent).toContain("When Crew merges a change in orbyn-engine");
    expect(screen.queryByTestId("dashboard-tiles")).toBeNull();
    expect(screen.getByRole("button", { name: "Open the Review queue" })).toBeDisabled();
    expect(document.body.textContent).toContain("Planned");
  });
});

describe("all projects", () => {
  it("groups the repos in one table with each one's sample, and totals cost one engine at a time", async () => {
    const user = userEvent.setup();
    show({ initial: { project: ALL } });
    const table = await screen.findByTestId("all-projects");
    expect(table).toHaveTextContent("orbyn-engineeringclaude, codex, opencode");
    expect(table).toHaveTextContent("71%27 of 38");
    expect(table).toHaveTextContent("41 min38 merges");
    expect(table).toHaveTextContent("5%2 of 38");
    expect(table).toHaveTextContent("3 of 5too few for a rate");
    expect(table).toHaveTextContent("0 merges");
    expect(table).toHaveTextContent("nothing to measure yet");
    expect(table).toHaveTextContent("86%12 of 14");
    expect(table).toHaveTextContent("claude 6.1M tokens");
    expect(document.body.textContent).toContain("claude 8.4M tokens · codex 4.3M tokens · opencode 2.3M tokens");
    expect(document.body.textContent).toContain("never added into one figure");
    await user.click(screen.getByRole("button", { name: "Open orbyn-planner" }));
    expect(await screen.findByText(/This repo has 5 merges/)).toBeInTheDocument();
  });

  it("says which repository could not be read and leaves the others", async () => {
    const client: DashboardClient = {
      ...createCrewDashboardMock(),
      history: async ({ project, days }) => {
        if (project === "orbyn-planner") throw new ApiFailure("history_unreadable", "That repository’s history is damaged.", 500);
        return sampleHistory(project, days);
      },
    };
    show({ initial: { project: ALL } }, client);
    const table = await screen.findByTestId("all-projects");
    expect(table).toHaveTextContent("Couldn’t readThat repository’s history is damaged.");
    expect(table).toHaveTextContent("71%27 of 38");
  });
});

describe("choosing the project and the period", () => {
  it("lists every project and All projects, checks the one shown, and reads the one chosen", async () => {
    const user = userEvent.setup();
    const client = createCrewDashboardMock();
    show({}, client);
    await screen.findByText("71%");
    await user.click(screen.getByRole("button", { name: "Project: orbyn-engineering" }));
    const menu = screen.getByRole("menu", { name: "Project" });
    expect(within(menu).getAllByRole("menuitemradio").map((item) => item.textContent)).toEqual(["orbyn-engineering", "orbyn-planner", "orbyn-engine", "orbyn-docs", "All projects"]);
    expect(within(menu).getByRole("menuitemradio", { name: "orbyn-engineering" })).toHaveAttribute("aria-checked", "true");
    await user.click(within(menu).getByRole("menuitemradio", { name: "orbyn-docs" }));
    expect(screen.queryByRole("menu")).toBeNull();
    await screen.findByText("86%");
    expect(client.reads).toContain("history:orbyn-docs:60");
  });

  it("closes the menu on Escape", async () => {
    const user = userEvent.setup();
    show();
    await user.click(screen.getByRole("button", { name: /Project:/ }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("remembers the choice per person, and a page that is not a sample restores it", async () => {
    const user = userEvent.setup();
    const view = render(<Dashboard projects={SAMPLE_PROJECTS} client={createCrewDashboardMock()} />);
    await screen.findByText("71%");
    await user.click(screen.getByRole("radio", { name: "90 days" }));
    await user.click(screen.getByRole("button", { name: /Project:/ }));
    await user.click(screen.getByRole("menuitemradio", { name: "orbyn-docs" }));
    view.unmount();
    const client = createCrewDashboardMock();
    render(<Dashboard projects={SAMPLE_PROJECTS} client={client} />);
    await waitFor(() => expect(client.reads).toContain("history:orbyn-docs:90"));
    expect(screen.getByRole("radio", { name: "90 days" })).toHaveAttribute("aria-checked", "true");
  });

  it("ignores a remembered choice that is nonsense or names a project that is gone", async () => {
    window.localStorage.setItem("orbyn.crew-dashboard", JSON.stringify({ project: "gone", days: 45 }));
    const client = createCrewDashboardMock();
    render(<Dashboard projects={SAMPLE_PROJECTS} client={client} />);
    await waitFor(() => expect(client.reads).toContain("history:orbyn-engineering:60"));
    window.localStorage.setItem("orbyn.crew-dashboard", "{{{");
  });

  it("survives browser storage that throws", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("full"); });
    const user = userEvent.setup();
    render(<Dashboard projects={SAMPLE_PROJECTS} client={createCrewDashboardMock()} />);
    await screen.findByText("71%");
    await user.click(screen.getByRole("radio", { name: "30 days" }));
    expect((await screen.findAllByText(/last 30 days/)).length).toBeGreaterThan(0);
    vi.restoreAllMocks();
  });
});

describe("waiting and failing", () => {
  it("shows nothing but the header for a quick read, and the shape of the tiles after 400 ms", async () => {
    vi.useFakeTimers();
    show({}, createCrewDashboardMock({ never: true }));
    expect(screen.getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(document.querySelector(".cx-t10sk")).toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(450); });
    expect(document.querySelectorAll(".cx-t10sk").length).toBeGreaterThan(0);
    expect(screen.getByText("Autonomy")).toBeInTheDocument();
  });

  it("says it plainly when the history can't be read, shows no old numbers and offers Try again", async () => {
    const user = userEvent.setup();
    const client = createCrewDashboardMock();
    client.failNext();
    show({}, client);
    expect(await screen.findByText("Couldn’t read the run history on this device. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getAllByText("Not shown while the history can’t be read")).toHaveLength(4);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("71%")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("copies the error text", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    show({}, createCrewDashboardMock({ failAll: true }));
    await user.click(await screen.findByRole("button", { name: "Copy the error" }));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("history_unreadable"));
  });

  it("lets the newest request win: a slow answer for the project left behind never lands", async () => {
    const user = userEvent.setup();
    let release: (value: DashboardHistory) => void = () => undefined;
    const slow = new Promise<DashboardHistory>((resolve) => { release = resolve; });
    const base = createCrewDashboardMock();
    const client: DashboardClient = {
      ...base,
      history: async (query, signal) => (query.project === "orbyn-engineering" ? slow : base.history!(query, signal)),
    };
    show({}, client);
    await user.click(screen.getByRole("button", { name: /Project:/ }));
    await user.click(screen.getByRole("menuitemradio", { name: "orbyn-docs" }));
    await screen.findByText("86%");
    await act(async () => { release(sampleHistory("orbyn-engineering", 60)); await Promise.resolve(); });
    expect(screen.getByText("86%")).toBeInTheDocument();
    expect(screen.queryByText("71%")).toBeNull();
  });
});

describe("text and numbers from outside", () => {
  const hostile = (): DashboardHistory => ({
    ...sampleHistory("orbyn-engineering", 60),
    engines: [{ name: "<img src=x onerror=alert(1)>" + "x".repeat(300), model: "m".repeat(300), merges: 38, unaided: -4, reverted: 1.5, cost: { unit: "javascript:alert(1)", amount: Number.NaN } }],
    areas: [{ name: "__proto__", merges: Number.POSITIVE_INFINITY, unaided: 3, reverted: 0 }],
    reverts: [{ title: "<b onmouseover=x>bold</b>", at: Number.NaN, by: "constructor" as never, certain: null }],
    weeks: [{ start: 1, merges: -1, unaided: 5, reverted: 9 }],
  });
  const client: DashboardClient = { ...createCrewDashboardMock(), history: async () => hostile() };

  it("draws names as capped text, never as markup, and an unknown value as unknown", async () => {
    const user = userEvent.setup();
    const { container } = show({}, client);
    await screen.findByText("71%");
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("[onmouseover]")).toBeNull();
    const row = [...container.querySelectorAll(".cx-aptb b")].find((b) => b.textContent?.startsWith("<img"));
    expect(row?.textContent?.length).toBeLessThanOrEqual(60 + 3 + 60);
    expect(container.textContent).toContain("__proto__");
    await user.click(screen.getByRole("button", { name: "Open revert rate by week" }));
    expect(container.querySelector("b[onmouseover]")).toBeNull();
    expect(screen.getByText("<b onmouseover=x>bold</b>")).toBeInTheDocument();
    expect(container.textContent).toContain("Reverted");
  });

  it("does not crash on a read that is not what the contract says", async () => {
    const broken: DashboardClient = { ...createCrewDashboardMock(), history: async () => ({ merges: "many", engines: null, weeks: 7 }) as unknown as DashboardHistory };
    show({}, broken);
    expect(await screen.findByText("No merges yet")).toBeInTheDocument();
  });
});

describe("the narrow panel", () => {
  it("is one column of tiles with their samples, and the trend and tables are behind Open in Crew", async () => {
    show({ compact: true });
    await screen.findByText("71%");
    expect(screen.queryAllByRole("img", { name: /Merges per week/ })).toHaveLength(0);
    expect(document.body.textContent).not.toContain("By engine");
    expect(document.body.textContent).not.toContain("Autopilot by area");
    expect(tile("Autonomy")).toHaveTextContent("27 of 38 merges · last 60 days");
    expect(screen.getByRole("link", { name: "Open in Crew" })).toHaveAttribute("href", "#/settings/crew/dashboard?repo=orbyn-engineering");
  });
});
