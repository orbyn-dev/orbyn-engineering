import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { Project } from "../../../generated/Project";
import { useConnection } from "../../../product/state/connection";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { NeedsYouInbox } from "./NeedsYouInbox";
import { NEEDS_REFRESH_MS } from "./useNeedsYou";

let mock: MockApi;
// App's cold module graph is fixture setup, not part of the live-frame assertion deadline.
// Install the owned mock first; importing does not render App or start its effects.
beforeAll(async () => {
  installMockFetch();
  await import("../../../app/App");
  // App's first import is the cold module graph of the whole app; under a full-suite run it passes the 10 s default hook timeout.
}, 60_000);
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
  window.localStorage.clear();
});

const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  const view = render(<NeedsYouInbox />);
  return { user, view };
}

/** The repository the areas read needs, as the areas page's own tests give it. */
const gitProject = (id: string, name: string) => ({ id, name, trusted: true, is_git: true, path: `/fixture/${id}` }) as Project;
function twoGitProjects() {
  useWorkspace.setState({ projects: { data: [gitProject("p-app", "orbyn-engineering"), gitProject("p-two", "other-repo")], loading: false, error: null } });
}
function oneGitProject() {
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project], loading: false, error: null } });
}

const list = () => screen.findByRole("list", { name: "Needs you" });

describe("the Needs you inbox on live data", () => {
  it("draws nothing while Crew Preview is off (the default scenario answers 404)", async () => {
    mock.reset("default");
    const { container } = render(<NeedsYouInbox />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    expect(calls("/api/crew/approvals/held")).toHaveLength(0);
  });

  it("lists held commands, the break on main and a background run, ranked, with the one count over all of them", async () => {
    await open("crew-held-account");
    const items = within(await list()).getAllByRole("listitem");
    // High risk first (the break on main), then the items without a stated risk, oldest first (the run is from 7 Nov).
    expect(items).toHaveLength(5);
    expect(items[0]).toHaveTextContent("Main broke and the cause isn’t certain");
    // Only a review item draws chips (v37): the break on main keeps its risk for the ranking alone. No row is amber.
    for (const row of items) {
      expect(row.querySelector(".ny-tag")).toBeNull();
      expect(row).not.toHaveAttribute("data-hi");
    }
    expect(items[0]).toHaveTextContent("Watch on main · 9be4410");
    expect(items[0]).toHaveTextContent("Nothing has been reverted. Main stays as it is until you choose.");
    expect(items[1]).toHaveTextContent("Paused: budget reached");
    expect(items[1]).toHaveTextContent("The run stopped at its budget. The budget cannot be changed; turn the crew off, or create a new crew with a bigger budget.");
    expect(items[1]).toHaveTextContent("Background crew · Fix failing CI");
    expect(items[2]).toHaveTextContent("DELETE FROM sessions");
    expect(items[3]).toHaveTextContent("aws s3 rb");
    expect(items[4]).toHaveTextContent("DELETE FROM orders");
    const bar = screen.getByRole("group", { name: "Filter the inbox" });
    expect(within(bar).getAllByRole("button").map((one) => one.textContent)).toEqual(["Needs you 5", "Approvals 3", "Main 1", "Background 1"]);
    expect(bar).toHaveTextContent("Risk and reversibility only where the item’s data has them");
  });

  it("a filter shows one kind and says which is pressed", async () => {
    const { user } = await open("crew-held-account");
    await list();
    await user.click(screen.getByRole("button", { name: "Approvals 3" }));
    expect(screen.getByRole("button", { name: "Approvals 3" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Needs you 5" })).toHaveAttribute("aria-pressed", "false");
    expect(within(screen.getByRole("list", { name: "Needs you" })).getAllByRole("listitem")).toHaveLength(3);
  });

  it("lists a task a trip wire stopped, and its Open goes to the Approvals tab", async () => {
    const { user } = await open("crew-tripwire");
    const item = (await screen.findByText("Stopped · too many denials")).closest("li") as HTMLElement;
    expect(item).toHaveTextContent("Task task-client");
    await user.click(within(item).getByRole("button", { name: /^Open/ }));
    expect(window.location.hash).toBe("#/crew/approvals");
  });

  it("Dismiss hides a stop in this browser only: it is remembered locally, nothing is sent, and another read does not bring it back", async () => {
    const { user, view } = await open("crew-tripwire");
    const item = (await screen.findByText("Stopped · too many denials")).closest("li") as HTMLElement;
    expect(item).toHaveTextContent("3 denials in a row stopped the task.");
    expect(item).toHaveTextContent("Dismiss hides this stop on this device.");
    await user.click(within(item).getByRole("button", { name: /^Dismiss/ }));
    expect(screen.queryByText("Stopped · too many denials")).toBeNull();
    expect(writes()).toHaveLength(0);
    expect(JSON.parse(window.localStorage.getItem("orbyn.needsyou.dismissedStops") ?? "[]")).toHaveLength(1);
    view.unmount();
    render(<NeedsYouInbox />);
    await screen.findByRole("list", { name: "Needs you" });
    expect(screen.queryByText("Stopped · too many denials")).toBeNull();
  });

  it("a demoted area is one line with its record, ranked below what waits on you (needsyou-demoted)", async () => {
    oneGitProject();
    await open("crew-areas-tripped");
    const items = within(await list()).getAllByRole("listitem");
    const item = items.find((one) => one.textContent?.includes("moved from Full to On")) as HTMLElement;
    expect(item).toHaveTextContent("web/auth moved from Full to On: 2 reverts in 14 days. Review or override.");
    expect(item).toHaveTextContent("Autopilot by area · circuit breaker · 9 merges in the window");
    expect(item).toHaveTextContent("Nothing is blocked. A High risk diff in web/auth now waits for you at the merge gate; Low and Medium still merge by themselves. Dismiss is remembered on this browser.");
    // It blocks nothing, so it waits below every item that waits on the person, whatever its age.
    expect(items[items.length - 1]).toBe(item);
    const bar = screen.getByRole("group", { name: "Filter the inbox" });
    expect(within(bar).getByRole("button", { name: "Autonomy 1" })).toBeVisible();
  });

  it("a demotion's Dismiss is remembered on this browser: nothing is sent, and another read does not bring it back", async () => {
    oneGitProject();
    const { user, view } = await open("crew-areas-tripped");
    const item = (await screen.findByText("web/auth moved from Full to On: 2 reverts in 14 days. Review or override.")).closest("li") as HTMLElement;
    await user.click(within(item).getByRole("button", { name: /^Dismiss/ }));
    expect(screen.queryByText(/moved from Full to On/)).toBeNull();
    expect(writes()).toHaveLength(0);
    const stored = () => JSON.parse(window.localStorage.getItem("orbyn.needsyou.dismissedAutonomy") ?? "[]") as string[];
    expect(stored()).toHaveLength(1);
    view.unmount();
    render(<NeedsYouInbox />);
    await screen.findByRole("list", { name: "Needs you" });
    expect(screen.queryByText(/moved from Full to On/)).toBeNull();
    expect(stored()).toHaveLength(1);
  });

  it("with several repositories the inbox says Autonomy shows one repository only; with one it says nothing", async () => {
    twoGitProjects();
    const first = await open("crew-areas-tripped");
    expect(await screen.findByText("Autonomy shows orbyn-engineering only.")).toBeVisible();
    first.view.unmount();
    oneGitProject();
    await open("crew-areas-tripped");
    await screen.findByRole("list", { name: "Needs you" });
    expect(screen.queryByText(/Autonomy shows/)).toBeNull();
  });

  it("a dismissal is per repository, and pruning only touches the shown repository's own entries", async () => {
    oneGitProject();
    const origin = window.location.origin;
    const other = `${origin}|autonomy:p-other:web/auth:1`;
    const stale = `${origin}|autonomy:p-app:web/auth:9`;
    window.localStorage.setItem("orbyn.needsyou.dismissedAutonomy", JSON.stringify([other, stale]));
    await open("crew-areas-tripped");
    await screen.findByRole("list", { name: "Needs you" });
    const stored = () => JSON.parse(window.localStorage.getItem("orbyn.needsyou.dismissedAutonomy") ?? "[]") as string[];
    await waitFor(() => expect(stored()).not.toContain(stale));
    expect(stored()).toContain(other);
  });

  it("after an override only the areas are read again, not the whole inbox", async () => {
    oneGitProject();
    const { user } = await open("crew-areas-tripped");
    const item = (await screen.findByText("web/auth moved from Full to On: 2 reverts in 14 days. Review or override.")).closest("li") as HTMLElement;
    const before = { stops: calls("/api/crew/stops").length, review: calls("/api/crew/autopilot/review").length, areas: calls("/api/crew/autopilot/areas?").length };
    await user.click(within(item).getByRole("button", { name: /^Override/ }));
    const menu = await screen.findByRole("menu", { name: "web/auth override" });
    await user.click(within(menu).getByRole("menuitem", { name: "Off" }));
    const dialog = await screen.findByRole("dialog", { name: "Turn web/auth off?" });
    await user.click(within(dialog).getByRole("checkbox", { name: "I understand that everything in web/auth will wait for me." }));
    await user.click(within(dialog).getByRole("button", { name: "Turn it off" }));
    await waitFor(() => expect(calls("/api/crew/autopilot/areas?").length).toBeGreaterThan(before.areas));
    await waitFor(() => expect(screen.queryByText(/moved from Full to On/)).toBeNull());
    expect(calls("/api/crew/stops")).toHaveLength(before.stops);
    expect(calls("/api/crew/autopilot/review")).toHaveLength(before.review);
  });

  it("a demotion's Review opens Autopilot by area on the row's repository; it writes nothing", async () => {
    oneGitProject();
    const { user } = await open("crew-areas-tripped");
    const item = (await screen.findByText("web/auth moved from Full to On: 2 reverts in 14 days. Review or override.")).closest("li") as HTMLElement;
    await user.click(within(item).getByRole("button", { name: /^Review/ }));
    expect(window.location.hash).toBe("#/settings/crew/areas?repo=p-app");
    expect(writes()).toHaveLength(0);
  });

  it("a demotion's Override… opens the page's menu, and Off asks first with the same warning", async () => {
    oneGitProject();
    const { user } = await open("crew-areas-tripped");
    const item = (await screen.findByText("web/auth moved from Full to On: 2 reverts in 14 days. Review or override.")).closest("li") as HTMLElement;
    await user.click(within(item).getByRole("button", { name: /^Override/ }));
    const menu = await screen.findByRole("menu", { name: "web/auth override" });
    expect(menu).toHaveTextContent("web/auth is on On. What should it follow?");
    expect(within(menu).getAllByRole("menuitem").map((one) => one.getAttribute("aria-label"))).toEqual(["Follow the record", "Keep the ceiling", "Off"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Off" }));
    const dialog = await screen.findByRole("dialog", { name: "Turn web/auth off?" });
    const confirm = within(dialog).getByRole("button", { name: "Turn it off" });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox", { name: "I understand that everything in web/auth will wait for me." }));
    await user.click(confirm);
    await waitFor(() => expect(writes().map(([url, init]) => [String(url), init?.method])).toContainEqual(["/api/crew/autopilot/areas/override", "POST"]));
    // The area is no longer demoted, so the inbox's line is gone.
    await waitFor(() => expect(screen.queryByText(/moved from Full to On/)).toBeNull());
  });

  const KEY = "orbyn.needsyou.dismissedStops";
  const stored = () => JSON.parse(window.localStorage.getItem(KEY) ?? "[]") as string[];

  it("a dismissal is keyed on the service, the ledger id, the task and the time: the same id with another task or time stays visible", async () => {
    const origin = window.location.origin;
    window.localStorage.setItem(KEY, JSON.stringify([`${origin}|stop:90:task-other:2026-11-07T14:27:00`, `${origin}|stop:90:task-client:2026-11-07T14:28:00`, `http://elsewhere.example|stop:90:task-client:2026-11-07T14:27:00`]));
    await open("crew-tripwire");
    expect(await screen.findByText("Stopped · too many denials")).toBeVisible();
  });

  it("the exact stop on this service stays hidden", async () => {
    window.localStorage.setItem(KEY, JSON.stringify([`${window.location.origin}|stop:90:task-client:2026-11-07T14:27:00`]));
    await open("crew-tripwire");
    await screen.findByRole("list", { name: "Needs you" });
    expect(screen.queryByText("Stopped · too many denials")).toBeNull();
    expect(stored()).toHaveLength(1);
  });

  it("prunes dismissals whose stop is no longer in the service's list", async () => {
    const origin = window.location.origin;
    window.localStorage.setItem(KEY, JSON.stringify([`${origin}|stop:90:task-client:2026-11-07T14:27:00`, `${origin}|stop:1:gone:2026-01-01T00:00:00`]));
    await open("crew-tripwire");
    await screen.findByRole("list", { name: "Needs you" });
    await waitFor(() => expect(stored()).toEqual([`${origin}|stop:90:task-client:2026-11-07T14:27:00`]));
  });

  it("still works with storage unavailable: the stop is hidden for this window", async () => {
    const { user } = await open("crew-tripwire");
    const item = (await screen.findByText("Stopped · too many denials")).closest("li") as HTMLElement;
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    await user.click(within(item).getByRole("button", { name: /^Dismiss/ }));
    expect(screen.queryByText("Stopped · too many denials")).toBeNull();
    vi.restoreAllMocks();
  });

  it("a background run's Open goes to the Background tab; the break on main has no screen to open yet", async () => {
    const { user } = await open("crew");
    const run = (await screen.findByText("Paused: budget reached")).closest("li") as HTMLElement;
    await user.click(within(run).getByRole("button", { name: /^Open/ }));
    expect(window.location.hash).toBe("#/crew/background");
    const main = screen.getByText("Main broke and the cause isn’t certain").closest("li") as HTMLElement;
    const openMain = within(main).getByRole("button", { name: /^Open/ });
    expect(openMain).toBeDisabled();
    expect(openMain).toHaveAttribute("aria-description", "Crew is not connected to this app yet");
  });

  it("says nothing needs you when every source is empty", async () => {
    await open("crew-empty");
    expect(await screen.findByText("Nothing needs you")).toBeVisible();
    expect(screen.getByText("When something needs you, it appears here.")).toBeVisible();
    expect(screen.queryByRole("list", { name: "Needs you" })).toBeNull();
  });

  it("offers Open the Flow view when nothing needs you, and it only changes the tab", async () => {
    const { user } = await open("crew-empty");
    await user.click(await screen.findByRole("button", { name: "Open the Flow view" }));
    expect(window.location.hash).toBe("#/crew/flow");
    expect(writes()).toHaveLength(0);
  });

  it("says the service is not running when no source can be reached, with Retry", async () => {
    mock.reset("crew");
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("/api/crew/preview")) return real(input, init);
      throw new TypeError("Failed to fetch");
    });
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can’t read what needs you")).toBeVisible();
    expect(screen.getByText("Check that the Orbyn service is running.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
  });

  it("announces the wait while the sources are read", async () => {
    mock.reset("crew");
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("/api/crew/preview")) return real(input, init);
      return new Promise<Response>(() => undefined);
    });
    render(<NeedsYouInbox />);
    expect(await screen.findByRole("status")).toHaveTextContent("Reading what needs you");
  });

  it("names the source it could not read, keeps the rest, and Try again reads again", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/stops", 1, 503, "unavailable");
    const user = userEvent.setup();
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can’t read stops. The rest is shown.")).toBeVisible();
    expect(await list()).toBeVisible();
    await user.click(screen.getByRole("button", { name: /^Try again/ }));
    await waitFor(() => expect(screen.queryByText(/Can’t read/)).toBeNull());
    // Try again only reads: no request that changes anything is sent.
    expect(writes()).toHaveLength(0);
  });

  it("is an error when nothing could be read, and retrying reads again", async () => {
    mock.reset("crew");
    for (const path of ["/api/crew/stops", "/api/crew/autopilot/review", "/api/crew/background", "/api/crew/approvals/held", "/api/crew/approvals/second-engine", "/api/crew/flow/crews", "/api/crew/needs-you"]) mock.fail(path, 1, 503, "unavailable");
    const user = userEvent.setup();
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can’t read what needs you")).toBeVisible();
    await user.click(screen.getByRole("button", { name: /retry/i }));
    expect(await list()).toBeVisible();
  });
});

describe("answering from the inbox", () => {
  const row = async (text: RegExp) => (await screen.findByText(text)).closest("li") as HTMLElement;

  it("Deny sends the existing deny route and nothing else, and the item leaves the list", async () => {
    const { user } = await open("crew-held-account");
    const orders = await row(/DELETE FROM orders/);
    await user.click(within(orders).getByRole("button", { name: /^Deny/ }));
    await waitFor(() => expect(calls("/api/crew/approvals/hold-orders/deny")).toHaveLength(1));
    expect(writes().map(([url]) => String(url))).toEqual([expect.stringContaining("/api/crew/approvals/hold-orders/deny")]);
    await waitFor(() => expect(screen.queryByText(/DELETE FROM orders/)).toBeNull());
  });

  it("Approve opens the Approvals tab where the approve-once flow runs: no dialog, no request, no inline flow", async () => {
    const { user } = await open("crew-held-account");
    await user.click(within(await row(/DELETE FROM orders/)).getByRole("button", { name: /^Approve/ }));
    expect(window.location.hash).toBe("#/crew/approvals");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("approve-once")).toHaveLength(0);
    expect(calls("/api/account/confirm")).toHaveLength(0);
    expect(writes()).toHaveLength(0);
  });

  it("Approve stays on without an account, since the Approvals tab says what is needed; Deny still works", async () => {
    const { user } = await open("crew-held");
    const orders = await row(/DELETE FROM orders/);
    expect(within(orders).getByRole("button", { name: /^Approve/ })).toBeEnabled();
    await user.click(within(orders).getByRole("button", { name: /^Deny/ }));
    await waitFor(() => expect(calls("/api/crew/approvals/hold-orders/deny")).toHaveLength(1));
  });

  it("a row already answered elsewhere keeps its place with Deny off and the reason (409)", async () => {
    // The service answers 409 but its list still has the hold open, so the row stays on screen as the window reloads.
    const { user } = await open("crew-held-account");
    const orders = await row(/DELETE FROM orders/);
    mock.fail("/api/crew/approvals/hold-orders/deny", 1, 409, "conflict");
    await user.click(within(orders).getByRole("button", { name: /^Deny/ }));
    await waitFor(() => expect(calls("/api/crew/approvals/hold-orders/deny")).toHaveLength(1));
    const deny = await waitFor(() => {
      const button = within(screen.getByText(/DELETE FROM orders/).closest("li") as HTMLElement).getByRole("button", { name: /^Deny/ });
      expect(button).toBeDisabled();
      return button;
    });
    expect(deny).toHaveAttribute("title", "Already decided.");
    expect(deny).toHaveAttribute("aria-description", "Already decided.");
    // No second request can be sent from it.
    await user.click(deny);
    expect(calls("/api/crew/approvals/hold-orders/deny")).toHaveLength(1);
  });

  it("from another computer, Deny is off with the reason; Approve only opens a tab", async () => {
    useConnection.setState({ remote: true });
    await open("crew-held-account");
    const orders = await row(/DELETE FROM orders/);
    const deny = within(orders).getByRole("button", { name: /^Deny/ });
    expect(deny).toBeDisabled();
    expect(deny).toHaveAttribute("title", "Do this from the computer the app runs on.");
    expect(within(orders).getByRole("button", { name: /^Approve/ })).toBeEnabled();
  });

  it("offers no merge or revert: from an approval only Approve, Deny and Open exist (a demoted area's Override… is person-only)", async () => {
    await open("crew-held-account");
    await list();
    const labels = screen.getAllByRole("button").map((one) => (one.textContent ?? "").trim());
    for (const label of labels) expect(label).toMatch(/^(Open|Approve|Deny|Dismiss|Try again|Needs you \d+|Approvals \d+|Background \d+|Main \d+|Stops \d+)$/);
  });
});

describe("the watch rows (KC6)", () => {
  it("lists a failing watch and one whose checks could not run beside a background run, and Open asks the Background tab for that watch", async () => {
    oneGitProject();
    const { user } = await open("crew-background-watch-needsyou");
    const items = within(await list()).getAllByRole("listitem");
    expect(items.map((one) => one.textContent)).toEqual(
      expect.arrayContaining([
        expect.stringContaining("Watch could not run: Lockfile check"),
        expect.stringContaining("Watch failing: Redirect tests"),
        expect.stringContaining("Needs approval"),
      ]),
    );
    const failing = items.find((one) => one.textContent?.includes("Watch failing: Redirect tests"))!;
    expect(failing).toHaveTextContent("Watch · Redirect tests · turned failing");
    expect(failing).toHaveTextContent("This is raised once for this change and is meant not to repeat while the check keeps failing.");
    const couldNotRun = items.find((one) => one.textContent?.includes("Watch could not run: Lockfile check"))!;
    expect(couldNotRun).toHaveTextContent("3 checks in a row could not run: timed out.");
    // The service's row time is the latest check, not the streak's first error: it is a "one row since" claim, not "3 since".
    expect(couldNotRun).toHaveTextContent("3 checks in a row · one row since ");
    expect(couldNotRun).toHaveTextContent("One row for the whole stretch");
    const bar = screen.getByRole("group", { name: "Filter the inbox" });
    expect(within(bar).getAllByRole("button").map((one) => one.textContent)).toEqual(["Needs you 3", "Background 3"]);
    const { useWatchFocus } = await import("../autopilot/approvals/watchFocus");
    useWatchFocus.setState({ id: null });
    await user.click(within(failing).getByRole("button", { name: "Open: Watch failing: Redirect tests" }));
    expect(useWatchFocus.getState().id).toBe("bg-watch-redirect");
  });

  it("draws no watch row when the shared Needs you read is not served: a 404 is a service without watches, not an error", async () => {
    oneGitProject();
    mock.reset("crew-held-account");
    mock.fail("/api/crew/needs-you", 1, 404, "not_found", "Not found.");
    render(<NeedsYouInbox />);
    const items = within(await list()).getAllByRole("listitem");
    expect(items.some((one) => one.textContent?.includes("Watch failing:"))).toBe(false);
    expect(items.some((one) => one.textContent?.includes("Watch could not run:"))).toBe(false);
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("the live frame", () => {
  it("draws the inbox inside the Crew page on the app's own routes (needsyou-live)", async () => {
    const { renderPreviewFrame } = await import("../shared/test-support");
    mock.reset("crew-held-account");
    await renderPreviewFrame("needsyou-live");
    const items = within(await screen.findByRole("list", { name: "Needs you" })).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    expect(screen.getByText("Inbox")).toBeVisible();
  });
});

describe("the Inbox tab of the Crew page", () => {
  it("is a tab beside Approvals, Trust and Background while Crew Preview is on, and mounts the inbox", async () => {
    const { CrewSurface } = await import("../autopilot/approvals/CrewSurface");
    mock.reset("crew-held-account");
    const user = userEvent.setup();
    render(<CrewSurface tab="approvals" />);
    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((one) => one.textContent)).toEqual(["Approvals", "Trust", "Background", "Inbox", "Flow", "Board", "Map", "Lanes"]);
    await user.click(screen.getByRole("tab", { name: "Inbox" }));
    expect(await screen.findByRole("list", { name: "Needs you" })).toBeVisible();
  });

  it("is not drawn while Crew Preview is off: no tab list at all", async () => {
    const { CrewSurface } = await import("../autopilot/approvals/CrewSurface");
    mock.reset("default");
    const { container } = render(<CrewSurface tab="inbox" />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
  });

  it("keeps the list and shows no error when a periodic read fails after it was shown", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      await open("crew-held-account");
      const before = within(await list()).getAllByRole("listitem").length;
      expect(before).toBeGreaterThan(0);
      const real = vi.mocked(fetch).getMockImplementation()!;
      vi.mocked(fetch).mockImplementation(async (input, init) =>
        String(input).includes("/api/crew/") && !String(input).includes("/api/crew/preview")
          ? new Response(JSON.stringify({ code: "unavailable", message: "The service is busy." }), { status: 503 })
          : real(input, init),
      );
      const reads = calls("/api/crew/").length;
      await act(async () => { vi.advanceTimersByTime(NEEDS_REFRESH_MS); });
      await waitFor(() => expect(calls("/api/crew/").length).toBeGreaterThan(reads));
      // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
      await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
      expect(within(await list()).getAllByRole("listitem")).toHaveLength(before);
      expect(screen.queryByText("The service is busy.")).toBeNull();
      expect(screen.queryByRole("alert")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
