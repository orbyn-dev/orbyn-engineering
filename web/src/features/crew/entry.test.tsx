import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { useConnection } from "../../product/state/connection";
import { SettingsPage } from "../settings/SettingsPage";
import { installMockFetch } from "../../test-support";

/** Crew Designs v87, the `entry-crew-*` frames and the four entry rows of `settings-crew`, reached from the app behind Crew Preview. */

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false, crewPreview: true });
  window.location.hash = "";
});
afterEach(() => {
  window.location.hash = "";
});

/** Answers one route itself and leaves every other one to the mock, keeping the calls watchable on the new fetch. */
function answer(match: RegExp, reply: () => Response | Promise<Response>) {
  const base = fetch;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => (match.test(String(input)) ? reply() : base(input, init))),
  );
}
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status });
const estimate = (over: Record<string, unknown> = {}) => ({ project: "p-app", history_days: 180, outcome_days: 30, eligible_prs: 64, lanes: [], outcomes_measured: true, running: null, ...over });

/** Every non-GET the page sent to Crew; the Settings shell has its own reads. */
const writes = () => vi.mocked(fetch).mock.calls.filter(([url, init]) => (init?.method ?? "GET") !== "GET" && String(url).includes("/api/crew")).map(([url]) => String(url));
const reads = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const open = (hash: string) => {
  window.location.hash = hash;
  return render(<SettingsPage section="crew" onClose={() => undefined} />);
};

describe("Settings › Crew: the four entry rows (settings-crew, entry-crew-settings-narrow-window)", () => {
  it("draws Review capacity, Escapes, Independent oracle and Reviewer backtest in that order, and reads nothing", async () => {
    mock.reset("crew-escapes");
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    const rows = await screen.findAllByRole("region", { name: /Review capacity|Escapes|Independent oracle|Reviewer backtest/ });
    expect(rows.map((row) => within(row).getByRole("button").textContent)).toEqual(["Open Review capacity", "Open Escapes", "Open Independent oracle", "Open Reviewer backtest"]);
    expect(reads("/api/crew/escapes")).toHaveLength(0);
    expect(reads("/api/crew/backtest")).toHaveLength(0);
    expect(writes()).toEqual([]);
  });

  it("the Escapes row has no value chip and opens the escape log", async () => {
    mock.reset("crew-escapes");
    const user = userEvent.setup();
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    const row = await screen.findByRole("region", { name: "Escapes" });
    expect(within(row).getByText(/Every revert, and every bug found after a merge that you don’t revert, is a possible escape/)).toBeInTheDocument();
    expect(within(row).queryByText("Defaults")).toBeNull();
    expect(within(row).queryByText("Reading")).toBeNull();
    await user.click(within(row).getByRole("button", { name: "Open Escapes" }));
    expect(window.location.hash).toBe("#/settings/crew/escapes");
  });

  it("Independent oracle is Planned: its button is disabled and says why, and clicking sends nothing", async () => {
    mock.reset("crew-escapes");
    const user = userEvent.setup();
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    const row = await screen.findByRole("region", { name: /Independent oracle/ });
    expect(within(row).getByText("Planned")).toBeInTheDocument();
    const button = within(row).getByRole("button", { name: "Open Independent oracle" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", expect.stringContaining("Planned"));
    expect(within(row).queryByText("Another engine · gate on")).toBeNull();
    await user.click(button);
    expect(window.location.hash).toBe("");
  });

  it("Reviewer backtest says the live runner is not approved, draws no budget value, and opens its page", async () => {
    mock.reset("crew-backtest");
    const user = userEvent.setup();
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    const row = await screen.findByRole("region", { name: "Reviewer backtest" });
    expect(within(row).getByText(/The live runner is not approved, so a backtest can’t start yet\./)).toBeInTheDocument();
    expect(within(row).queryByText("Budget not set")).toBeNull();
    await user.click(within(row).getByRole("button", { name: "Open Reviewer backtest" }));
    expect(window.location.hash).toBe("#/settings/crew/backtest");
  });

  it("with Crew Preview off, none of the pages is there and nothing is read", async () => {
    mock.reset("crew-escapes");
    useConnection.setState({ crewPreview: false });
    open("#/settings/crew/escapes");
    expect(await screen.findByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThan(0));
    expect(reads("/api/crew/escapes")).toHaveLength(0);
  });
});

describe("Settings › Crew › Escapes (entry-crew-settings-escapes-*, entry-crew-coverage-link-log)", () => {
  it("opens the live escape log by its address, with a Coverage link before the project chip for the same project", async () => {
    mock.reset("crew-escapes");
    open("#/settings/crew/escapes");
    expect((await screen.findAllByText("Escape log")).length).toBeGreaterThan(0);
    const link = await screen.findByRole("link", { name: "Coverage" });
    expect(link).toHaveAttribute("href", "#/settings/crew/capacity/coverage?repo=p-app");
    expect((await screen.findAllByText(/Old returnTo links stopped redirecting/)).length).toBeGreaterThan(0);
    expect(writes()).toEqual([]);
  });

  it("an escape opens its page in the app, not a preview frame, and the page is read only until a click", async () => {
    mock.reset("crew-escapes");
    const user = userEvent.setup();
    open("#/settings/crew/escapes");
    await user.click(await screen.findByRole("button", { name: /Old returnTo links stopped redirecting/ }));
    expect(window.location.hash).toBe("#/settings/crew/escapes/detail?id=esc-returnto");
    expect((await screen.findAllByText(/Old returnTo links stopped redirecting/)).length).toBeGreaterThan(0);
    expect(window.location.hash).not.toContain("preview");
    expect(writes()).toEqual([]);
  });

  it("an unreadable log draws its own error and Try again, and the Coverage link stays", async () => {
    mock.reset("crew-escapes-unreadable");
    open("#/settings/crew/escapes");
    expect(await screen.findByRole("alert")).toHaveTextContent("Can’t read escapes");
    expect(screen.getByRole("link", { name: "Coverage" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });
});

describe("Settings › Crew › Review capacity › Coverage (entry-crew-coverage-link-capacity)", () => {
  it("the Review coverage row has a Coverage link beside its value that opens the coverage map for the project", async () => {
    mock.reset("crew-capacity");
    const user = userEvent.setup();
    open("#/settings/crew/capacity");
    expect(await screen.findByText("Recorded at merge")).toBeVisible();
    const link = screen.getByRole("link", { name: "Coverage" });
    expect(link).toHaveAttribute("href", "#/settings/crew/capacity/coverage?repo=p-app");
    await user.click(link);
    expect(await screen.findByText("Review coverage")).toBeVisible();
    expect(reads("/api/crew/capacity/coverage").length).toBeGreaterThan(0);
    expect(writes()).toEqual([]);
  });

  it("the coverage map opens by its own address for the project named, and falls back to the first one", async () => {
    mock.reset("crew-capacity");
    open("#/settings/crew/capacity/coverage?repo=not-a-project");
    expect(await screen.findByText("Review coverage")).toBeVisible();
    expect(reads("project=p-app").length).toBeGreaterThan(0);
  });
});

describe("Settings › Crew › Reviewer backtest (entry-crew-backtest-page and its states)", () => {
  it("shows the not-approved line first, then the rows from the estimate, and Planned rows as Not shown", async () => {
    mock.reset("crew-backtest");
    open("#/settings/crew/backtest");
    expect(await screen.findByText("Crew › Reviewer backtest")).toBeVisible();
    expect(screen.getByText("The live runner is not approved.")).toBeVisible();
    expect(await screen.findByText("64 pull requests")).toBeVisible();
    expect(screen.getByText("Measured")).toBeVisible();
    for (const title of ["Which lanes", "Run budget", "Last backtest"]) {
      const row = screen.getByText(title).closest(".cx-setrow") as HTMLElement;
      expect(within(row).getByText("Planned")).toBeInTheDocument();
      expect(within(row).getByText("Not shown")).toBeInTheDocument();
    }
    expect(screen.getByText("What this tells you, and what it doesn’t")).toBeVisible();
  });

  it("draws no start and no stop, and opening it sends only the estimate read", async () => {
    mock.reset("crew-backtest");
    open("#/settings/crew/backtest");
    await screen.findByText("64 pull requests");
    expect(screen.queryByRole("button", { name: /start|stop|run a backtest/i })).toBeNull();
    expect(writes()).toEqual([]);
    expect(reads("/api/crew/backtest").map(([url]) => String(url))).toEqual(["/api/crew/backtest/estimate?project=p-app"]);
  });

  it("Open the estimate goes to the estimate in the app, and the estimate's Settings button returns here", async () => {
    mock.reset("crew-backtest");
    const user = userEvent.setup();
    open("#/settings/crew/backtest");
    await user.click(await screen.findByRole("link", { name: "Open the estimate" }));
    expect(window.location.hash).toBe("#/settings/crew/backtest/estimate");
    await user.click(await screen.findByRole("link", { name: "Settings" }));
    expect(window.location.hash).toBe("#/settings/crew/backtest");
  });

  it("says Not measured when the escape log cannot say (-not-measured)", async () => {
    mock.reset("crew-backtest");
    answer(/\/api\/crew\/backtest\/estimate/, () => json(200, estimate({ outcomes_measured: false })));
    open("#/settings/crew/backtest");
    expect(await screen.findByText("Not measured")).toBeVisible();
    expect(screen.getByText(/the report shows finding counts only, never a zero/)).toBeVisible();
  });

  it("with no pull request old enough, says none yet and disables Open the estimate with its reason (-no-history)", async () => {
    mock.reset("crew-backtest");
    answer(/\/api\/crew\/backtest\/estimate/, () => json(200, estimate({ eligible_prs: 0 })));
    open("#/settings/crew/backtest");
    expect(await screen.findByText("None yet")).toBeVisible();
    expect(screen.getByText(/No merged pull requests are 30 days old or more yet\./)).toBeVisible();
    const button = screen.getByRole("button", { name: "Open the estimate" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "No merged pull request is old enough to replay yet");
  });

  it("a run in progress replaces the not-approved line with a banner that opens its report (-running)", async () => {
    mock.reset("crew-backtest");
    answer(/\/api\/crew\/backtest\/estimate/, () => json(200, estimate({ running: "run-live" })));
    open("#/settings/crew/backtest");
    expect(await screen.findByText("A backtest is running.")).toBeVisible();
    expect(screen.queryByText("The live runner is not approved.")).toBeNull();
    expect(screen.getByRole("link", { name: "Open the running backtest" })).toHaveAttribute("href", "#/settings/crew/backtest/report?id=run-live");
  });

  it("draws the title, intro and not-approved line while the estimate is read, with the status read out (-loading)", async () => {
    mock.reset("crew-backtest");
    answer(/\/api\/crew\/backtest\/estimate/, () => new Promise<Response>(() => undefined));
    open("#/settings/crew/backtest");
    expect(await screen.findByText("Crew › Reviewer backtest")).toBeVisible();
    expect(screen.getByText("The live runner is not approved.")).toBeVisible();
    expect(await screen.findByText("Reading the backtest estimate")).toBeInTheDocument();
    expect(screen.queryByText("What it replays")).toBeNull();
  });

  it("fails closed when the estimate can't be read: one red banner, no rows, and Try again reads again (-error)", async () => {
    mock.reset("crew-backtest");
    const user = userEvent.setup();
    answer(/\/api\/crew\/backtest\/estimate/, () => json(500, { code: "unreadable", message: "the backtest estimate can’t be read on this device" }));
    open("#/settings/crew/backtest");
    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Can’t read the estimate");
    expect(banner).toHaveTextContent("no backtest was started");
    expect(screen.getByText("unreadable: the backtest estimate can’t be read on this device")).toBeVisible();
    expect(screen.queryByText("What it replays")).toBeNull();
    const before = reads("/api/crew/backtest/estimate").length;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(reads("/api/crew/backtest/estimate").length).toBe(before + 1));
    expect(writes()).toEqual([]);
  });

  it("the report opens by its address in the app", async () => {
    mock.reset("crew-backtest-report");
    open("#/settings/crew/backtest/report?id=run-4n");
    await waitFor(() => expect(reads("/api/crew/backtest/run-4n/report")).toHaveLength(1));
    expect(await screen.findByText("Finished")).toBeVisible();
  });
});
