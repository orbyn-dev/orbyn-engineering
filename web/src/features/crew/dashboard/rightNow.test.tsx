import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { SAMPLE_PROJECTS } from "../../../../mock/crewDashboard";
import { installMockFetch } from "../../../test-support";
import type { DashboardHistory } from "./contract";
import { Dashboard } from "./Dashboard";

/**
 * KC3's "Right now" row, black-box through the rendered dashboard. Only the network is faked (installMockFetch and the
 * mock's own dashboard answers); a scenario that does not carry a live row is changed at the same network edge.
 */
let mock: MockApi;
beforeEach(() => {
  window.localStorage.clear();
  mock = installMockFetch();
});

const show = (scenario: "crew" | `crew-${string}`, props: Partial<Parameters<typeof Dashboard>[0]> = {}) => {
  mock.reset(scenario);
  return render(<Dashboard sample projects={SAMPLE_PROJECTS} {...props} />);
};

const rightNow = () => screen.getByRole("group", { name: "Right now" });
const tile = (label: string) => document.querySelector<HTMLElement>(`[data-tile="${label}"]`)!;
const value = (label: string) => tile(label).querySelector(".cx-t10b")?.textContent;
const sub = (label: string) => tile(label).querySelector(".cx-t10s")?.textContent;
const foot = (label: string) => tile(label).querySelector(".cx-t10f")?.textContent;
const rows = (label: string) =>
  [...tile(label).querySelectorAll(".cx-t10e span")].map((row) => [row.querySelector("i")?.textContent, row.querySelector("b")?.textContent]);

describe("the Right now row", () => {
  it("draws the two live tiles with their words, rows and samples", async () => {
    show("crew");
    await screen.findByText("71%");
    const group = rightNow();
    expect(within(group).getByText("Right now")).toBeInTheDocument();
    expect(within(group).getByText("Live count. Ignores the 30, 60 and 90 day range.")).toBeInTheDocument();

    expect(value("Needs you")).toBe("2");
    expect(sub("Needs you")).toBe("waiting for you now");
    expect(rows("Needs you")).toEqual([["Asked a question", "1"], ["A merge that waits", "1"]]);
    expect(foot("Needs you")).toBe("This repository only. The Inbox may list more.");

    expect(value("Working")).toBe("2");
    expect(sub("Working")).toBe("workers own live work");
    expect(rows("Working")).toEqual([["Checks running", "1"], ["Retrying after a failed check", "1"]]);
    expect(foot("Working")).toBe("None of them waits for you");

    // Idle is not served and has no tile.
    expect(document.body.textContent).not.toMatch(/\bIdle\b/);
  });

  it("draws zeroes with no rows and its own lines when nothing waits and no worker works", async () => {
    show("crew", { initial: { project: "orbyn-planner" } });
    await screen.findByText(/This repo has 5 merges in the last 60 days/);
    expect(value("Needs you")).toBe("0");
    expect(sub("Needs you")).toBe("waiting for you now");
    expect(rows("Needs you")).toEqual([]);
    expect(foot("Needs you")).toBe("Nothing waits for you.");

    expect(value("Working")).toBe("0");
    expect(sub("Working")).toBe("no worker owns live work");
    expect(rows("Working")).toEqual([]);
    expect(foot("Working")).toBe("Nothing is running.");
    expect(document.body.textContent).not.toMatch(/\bIdle\b/);
  });

  it("says the working list is cut when the live read is truncated, and not for waits", async () => {
    show("crew-dashboard-live-truncated");
    await screen.findByText("71%");
    expect(value("Working")).toBe("1");
    expect(sub("Working")).toBe("worker owns live work");
    expect(rows("Working")).toEqual([["Running", "1"]]);
    expect(foot("Working")).toBe("None of them waits for you. Showing the newest workers only.");
    expect(foot("Needs you")).not.toContain("Showing the newest workers only.");
  });

  it("draws no row and no error when an older service sends no live block", async () => {
    show("crew-dashboard-live-none");
    await screen.findByText("71%");
    expect(screen.getByTestId("dashboard-tiles")).toBeInTheDocument();
    expect(screen.queryByTestId("dashboard-right-now")).toBeNull();
    expect(screen.queryByRole("group", { name: "Right now" })).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("keeps the row, unchanged, when the window changes from 60 to 30 days", async () => {
    const user = userEvent.setup();
    show("crew");
    await screen.findByText("71%");
    const waiting = value("Needs you");
    const working = value("Working");
    await user.click(screen.getByRole("radio", { name: "30 days" }));
    await waitFor(() => expect(tile("Autonomy")).toHaveTextContent("last 30 days"));
    expect(rightNow()).toBeInTheDocument();
    expect(value("Needs you")).toBe(waiting);
    expect(value("Working")).toBe(working);
    expect(rows("Needs you")).toEqual([["Asked a question", "1"], ["A merge that waits", "1"]]);
  });

  it("keeps an unknown kind and an unknown working reason as their raw words", async () => {
    mock.reset("crew");
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const response = await real(input, init);
      if (!String(input).includes("/api/crew/dashboard")) return response;
      const body = (await response.json()) as DashboardHistory;
      body.live = {
        needs_you: [{ kind: "future_wait", count: 2 }] as never,
        working: [{ why: "future_why", count: 1 }] as never,
        truncated: false,
      };
      return new Response(JSON.stringify(body), { status: response.status });
    });
    render(<Dashboard sample projects={SAMPLE_PROJECTS} />);
    await screen.findByText("71%");
    expect(rows("Needs you")).toEqual([["future wait", "2"]]);
    expect(rows("Working")).toEqual([["future_why", "1"]]);
  });
});
