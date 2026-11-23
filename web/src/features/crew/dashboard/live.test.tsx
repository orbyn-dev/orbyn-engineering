import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../../../api/types";
import type { AutopilotAreas } from "../../../generated/AutopilotAreas";
import type { AutopilotTrust } from "../../../generated/AutopilotTrust";
import { useWorkspace } from "../../../product/state/workspace";
import { CrewSettingsSection } from "../autopilot/approvals/CrewSettings";
import { LiveDashboard } from "./LiveDashboard";
import { dashboardClient, setDashboardClient } from "./contract";

const app = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;
const planner = { id: "p-planner", name: "orbyn-planner", trusted: true, is_git: true, path: "/fixture/planner" } as Project;
const notes = { id: "p-notes", name: "plain-folder", trusted: true, is_git: false, path: "/fixture/notes" } as Project;

const rate = (merges: number, reverts: number) => ({ merges, reverts, revert_permille: merges ? Math.round((reverts / merges) * 1000) : 0, counts: merges >= 10 });
const trust = (over: Partial<AutopilotTrust> = {}): AutopilotTrust => ({
  window_days: 60, min_sample: 10, high_revert_permille: 150, total: rate(38, 2),
  areas: [{ name: "web/auth", rate: rate(15, 2) }, { name: "docs/guides", rate: rate(14, 0) }],
  engines: [{ name: "claude", rate: rate(21, 1) }, { name: "codex", rate: rate(12, 1) }, { name: "opencode", rate: rate(5, 0) }],
  autonomy_by_area: true, truncated: false, ...over,
});
const areas: AutopilotAreas = { project: "p-app", ceiling: "full", as_of: 1_762_500_000, enforced: true, total: 0, rows: [] };

type Answer = { status: number; body?: unknown } | "never";
let answers: { preview: Answer; trust: Answer; trustBy: Record<string, Answer>; dashboard?: Answer };
const sent = () => vi.mocked(fetch).mock.calls.map(([url, init]) => `${init?.method ?? "GET"} ${String(url)}`);
const tile = (label: string) => document.querySelector<HTMLElement>(`[data-tile="${label}"]`)!;

beforeEach(() => {
  window.localStorage.clear();
  window.location.hash = "";
  setDashboardClient(null);
  answers = { preview: { status: 200, body: { enabled: true } }, trust: { status: 200, body: trust() }, trustBy: {} };
  useWorkspace.setState({ projects: { data: [app, planner, notes], loading: false, error: null } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const url = String(input);
      const project = new URL(url, "http://x").searchParams.get("project") ?? "";
      const answer: Answer = url.startsWith("/api/projects") ? { status: 200, body: [app, planner, notes] }
        : url.includes("/api/crew/preview") ? answers.preview
        : url.includes("/api/crew/dashboard") ? (answers.dashboard ?? { status: 404 })
        : url.includes("/api/crew/trust") ? (answers.trustBy[project] ?? answers.trust)
        : url.includes("/api/crew/autopilot/areas/history") ? { status: 200, body: { project, area: "", events: [], truncated: false } }
        : url.includes("/api/crew/autopilot/areas") ? { status: 200, body: areas }
        : url.includes("/api/crew/autopilot/thresholds") ? { status: 200, body: { window_days: 14, min_merges: 5, trip_reverts: 2, recovery_days: 7, recovery_merges: 5, proposed: true } }
        : { status: 404 };
      if (answer === "never") return new Promise<Response>(() => undefined);
      return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
    }),
  );
});

describe("the production client", () => {
  it("asks for the history read with the period and the person's offset, and falls back to the trust record on a 404", async () => {
    expect(dashboardClient().history).not.toBeNull();
    render(<LiveDashboard />);
    await screen.findByText("5%");
    const asked = sent().filter((call) => call.includes("/api/crew/dashboard"));
    expect(asked).toHaveLength(1);
    expect(asked[0]).toMatch(/^GET \/api\/crew\/dashboard\?project=p-app&days=60&utc_offset_minutes=-?\d+$/);
  });
});

describe("Settings › Crew › Dashboard on the real reads", () => {
  it("draws the revert rate from the trust record and Planned for what no read holds, with no zeros in their place", async () => {
    render(<LiveDashboard />);
    await screen.findByText("5%");
    expect(sent()).toContain("GET /api/crew/trust?project=p-app");
    expect(tile("Revert rate")).toHaveTextContent("2 of 38 merges reverted");
    expect(tile("Revert rate")).toHaveTextContent("38 merges · last 60 days");
    for (const label of ["Autonomy", "Cycle time", "Cost"]) {
      expect(tile(label)).toHaveTextContent("Planned");
      expect(tile(label).textContent).not.toMatch(/\b0\b|0%/);
    }
    expect(tile("Autonomy")).toHaveTextContent("—");
    expect(tile("Cost")).toHaveTextContent("each in its own unit, never added up");
    // No bars are drawn from a series the service does not have.
    expect(screen.queryAllByRole("img", { name: /Merges per week/ })).toHaveLength(0);
    expect(screen.getByRole("heading", { name: /Over time/ })).toHaveTextContent("Planned");
    expect(screen.queryByText("Sample data")).toBeNull();
  });

  it("lists merges and reverts by engine and by area, with a rate only from the minimum sample, and no cost figure", async () => {
    render(<LiveDashboard />);
    await screen.findByText("5%");
    const text = document.body.textContent ?? "";
    expect(text).toContain("claude");
    expect(text).toContain("1 of 21");
    expect(text).toContain("reverted · 5%");
    expect(text).toContain("0 of 5");
    expect(text).toContain("too few for a rate");
    expect(text).toContain("web/auth");
    expect(text).toContain("2 of 15");
    expect(text).toContain("reverted · 13%");
    expect(text).not.toMatch(/\d+(\.\d+)?[kM]? tokens/);
    expect(text).toContain("Autonomy is Planned");
  });

  it("reads one window: it is the period drawn as chosen, and the others are disabled and Planned", async () => {
    render(<LiveDashboard />);
    await screen.findByText("5%");
    const group = screen.getByRole("radiogroup", { name: "Period" });
    expect(within(group).getByRole("radio", { name: "60 days" })).toHaveAttribute("aria-checked", "true");
    for (const name of ["30 days", "90 days"]) {
      const radio = within(group).getByRole("radio", { name });
      await waitFor(() => expect(radio).toBeDisabled());
      expect(radio).toHaveAttribute("aria-description", expect.stringContaining("Planned"));
    }
  });

  it("draws the served history read: autonomy and cycle time from the service, and the other periods choosable", async () => {
    const week = { start: 1_761_523_200, merges: 12, unaided: 9, reverted: 1 };
    answers.dashboard = {
      status: 200,
      body: {
        project: "p-app", days: 60, as_of: 1_762_500_000, min_rate_sample: 10, min_median_sample: 5, merges: 38, unaided: 30, reverted: 2, reverted_auto: 1, reverted_by_person: 1,
        cycle: { median_s: 5400, p90_s: 20000 }, weeks: [week],
        engines: [{ name: "claude", merges: 21, unaided: 18, reverted: 1, cost: { unit: "tokens", amount: 6_100_000 } }, { name: "codex", merges: 17, unaided: 12, reverted: 1 }],
        areas: [{ name: "web/auth", merges: 15, unaided: 10, reverted: 2 }], reverts: [{ title: "Revert login", at: 1_762_400_000, by: "person" }], recent: [], truncated: false,
      },
    };
    render(<LiveDashboard />);
    await waitFor(() => expect(tile("Autonomy")).not.toHaveTextContent("Planned"));
    expect(tile("Autonomy")).toHaveTextContent("79%");
    expect(tile("Cycle time")).not.toHaveTextContent("Planned");
    const group = screen.getByRole("radiogroup", { name: "Period" });
    expect(within(group).getByRole("radio", { name: "30 days" })).not.toBeDisabled();
  });

  it("takes the window the service reports, whatever it is", async () => {
    answers.trust = { status: 200, body: trust({ window_days: 45 }) };
    render(<LiveDashboard />);
    await screen.findByText("5%");
    expect(tile("Revert rate")).toHaveTextContent("38 merges · last 45 days");
    await waitFor(() => expect(within(screen.getByRole("radiogroup", { name: "Period" })).getAllByRole("radio").every((radio) => radio.getAttribute("aria-checked") === "false")).toBe(true));
  });

  it("shows the merges and no rate for a repository with fewer than 10 merges", async () => {
    answers.trust = { status: 200, body: trust({ total: rate(5, 0), areas: [], engines: [{ name: "claude", rate: rate(5, 0) }] }) };
    render(<LiveDashboard />);
    await screen.findByText(/This repo has 5 merges in the last 60 days/);
    expect(tile("Revert rate")).toHaveTextContent("0 of 5");
    expect(tile("Revert rate")).toHaveTextContent("too few for a rate");
    expect(tile("Revert rate").textContent).not.toContain("%");
  });

  it("says so, with no zero, when there are no merges", async () => {
    answers.trust = { status: 200, body: trust({ total: rate(0, 0), areas: [], engines: [] }) };
    render(<LiveDashboard />);
    expect(await screen.findByText("No merges yet")).toBeInTheDocument();
    expect(screen.queryByTestId("dashboard-tiles")).toBeNull();
  });

  it("draws only git projects, reads the one named in the address, and falls back to the first when it is not listed", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<LiveDashboard wanted="p-planner" />);
    await waitFor(() => expect(sent()).toContain("GET /api/crew/trust?project=p-planner"));
    await user.click(screen.getByRole("button", { name: /Project:/ }));
    expect(within(screen.getByRole("menu")).getAllByRole("menuitemradio").map((item) => item.textContent)).toEqual(["orbyn-engineering", "orbyn-planner", "All projects"]);
    unmount();
    vi.mocked(fetch).mockClear();
    render(<LiveDashboard wanted="p-notes" />);
    await waitFor(() => expect(sent()).toContain("GET /api/crew/trust?project=p-app"));
  });

  it("reads each project once for All projects, says which one could not be read, and never totals cost across engines", async () => {
    answers.trustBy["p-planner"] = { status: 500, body: { code: "boom", message: "The record is damaged." } };
    window.localStorage.setItem("orbyn.crew-dashboard", JSON.stringify({ project: "*", days: 60 }));
    render(<LiveDashboard />);
    const table = await screen.findByTestId("all-projects");
    expect(table).toHaveTextContent("orbyn-engineering");
    expect(table).toHaveTextContent("5%2 of 38");
    expect(table).toHaveTextContent("Couldn’t readThe record is damaged.");
    expect(sent().filter((call) => call.startsWith("GET /api/crew/trust?project="))).toEqual(["GET /api/crew/trust?project=p-app", "GET /api/crew/trust?project=p-planner"]);
    expect(document.body.textContent).toContain("Cost per engine");
    expect(document.body.textContent).not.toMatch(/\d tokens/);
  });

  it("says it plainly when the trust record can't be read, and a retry reads again", async () => {
    const user = userEvent.setup();
    answers.trust = { status: 500, body: { code: "boom", message: "The store is busy." } };
    render(<LiveDashboard />);
    expect(await screen.findByText("Couldn’t read the run history on this device. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getAllByText("Not shown while the history can’t be read")).toHaveLength(4);
    answers.trust = { status: 200, body: trust() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("5%")).toBeInTheDocument();
    expect(sent().filter((call) => call.startsWith("GET /api/crew/trust?project=p-app"))).toHaveLength(2);
  });

  it("draws nothing and reads no Crew route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveDashboard />);
    await waitFor(() => expect(sent()).toContain("GET /api/crew/preview"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    expect(sent().filter((call) => call.includes("/api/crew/trust"))).toHaveLength(0);
  });

  it("sends no write, ever", async () => {
    const user = userEvent.setup();
    render(<LiveDashboard />);
    await screen.findByText("5%");
    await user.click(screen.getByRole("button", { name: /Project:/ }));
    await user.click(screen.getByRole("menuitemradio", { name: "All projects" }));
    await screen.findByTestId("all-projects");
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
  });
});

describe("the Settings › Crew page", () => {
  it("opens the dashboard at its own address for the project the address names", async () => {
    window.location.hash = "#/settings/crew/dashboard?repo=p-planner";
    render(<CrewSettingsSection />);
    await waitFor(() => expect(sent()).toContain("GET /api/crew/trust?project=p-planner"));
    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
  });
});
