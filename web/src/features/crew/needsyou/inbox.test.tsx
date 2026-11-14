import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { NeedsYouInbox } from "./NeedsYouInbox";

let mock: MockApi;
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

const list = () => screen.findByRole("list", { name: "Needs you" });

describe("the Needs you inbox on live data", () => {
  it("draws nothing while Crew Preview is off (the default scenario answers 404)", async () => {
    mock.reset("default");
    const { container } = render(<NeedsYouInbox />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
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
    expect(items[0]).toHaveTextContent("Risk · High");
    // The chip is amber because the row is High risk; no row is amber for its place, and rows with no stated risk have no chip.
    expect(items[0]?.querySelector(".ny-tag")).toHaveAttribute("data-tone", "amber");
    expect(items[0]).not.toHaveAttribute("data-hi");
    for (const row of items.slice(1)) expect(row.querySelector(".ny-tag")).toBeNull();
    expect(items[0]).toHaveTextContent("Watch on main · 9be4410");
    expect(items[0]).toHaveTextContent("Nothing has been reverted. Main stays as it is until you choose.");
    expect(items[1]).toHaveTextContent("Paused: budget reached");
    expect(items[1]).toHaveTextContent("It used its whole turn budget before the checks passed");
    expect(items[1]).toHaveTextContent("Background · Fix failing CI");
    expect(items[2]).toHaveTextContent("DELETE FROM sessions");
    expect(items[3]).toHaveTextContent("aws s3 rb");
    expect(items[4]).toHaveTextContent("DELETE FROM orders");
    const bar = screen.getByRole("group", { name: "Filter the inbox" });
    expect(within(bar).getAllByRole("button").map((one) => one.textContent)).toEqual(["Needs you 5", "Approvals 3", "Reviews 1", "Main 1"]);
    expect(bar).toHaveTextContent("Ranked by risk, then by how hard it is to undo, then by what waiting costs");
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
    expect(item).toHaveTextContent("It was denied 3 times in a row, so Crew stopped it.");
    await user.click(within(item).getByRole("button", { name: /^Dismiss/ }));
    expect(screen.queryByText("Stopped · too many denials")).toBeNull();
    expect(writes()).toHaveLength(0);
    expect(JSON.parse(window.localStorage.getItem("orbyn.needsyou.dismissedStops") ?? "[]")).toHaveLength(1);
    view.unmount();
    render(<NeedsYouInbox />);
    await screen.findByRole("list", { name: "Needs you" });
    expect(screen.queryByText("Stopped · too many denials")).toBeNull();
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
    expect(screen.getByText("When something needs you, it appears here, ranked, with what waiting costs.")).toBeVisible();
    expect(screen.queryByRole("list", { name: "Needs you" })).toBeNull();
  });

  it("names the source it could not read, keeps the rest, and Try again reads again", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/stops", 1, 503, "unavailable");
    const user = userEvent.setup();
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can’t read stops. The rest is shown.")).toBeVisible();
    expect(await list()).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByText(/Can’t read/)).toBeNull());
  });

  it("is an error when nothing could be read, and retrying reads again", async () => {
    mock.reset("crew");
    for (const path of ["/api/crew/stops", "/api/crew/autopilot/review", "/api/crew/background", "/api/crew/approvals/held"]) mock.fail(path, 1, 503, "unavailable");
    const user = userEvent.setup();
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can't read what needs you")).toBeVisible();
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

  it("a row already answered elsewhere has Deny off with the reason", async () => {
    const { user } = await open("crew-held-account-race");
    const orders = await row(/DELETE FROM orders/);
    await user.click(within(orders).getByRole("button", { name: /^Deny/ }));
    await waitFor(() => expect(calls("/api/crew/approvals/hold-orders/deny")).toHaveLength(1));
    await waitFor(() => {
      const again = screen.queryByText(/DELETE FROM orders/)?.closest("li");
      if (again) expect(within(again as HTMLElement).getByRole("button", { name: /^Deny/ })).toBeDisabled();
    });
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

  it("offers no merge, revert or other write: only Approve, Deny and Open exist", async () => {
    await open("crew-held-account");
    await list();
    const labels = screen.getAllByRole("button").map((one) => (one.textContent ?? "").trim());
    for (const label of labels) expect(label).toMatch(/^(Open|Approve|Deny|Dismiss|Try again|Needs you \d+|Approvals \d+|Reviews \d+|Main \d+|Stopped \d+)$/);
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
    expect(tabs.map((one) => one.textContent)).toEqual(["Approvals", "Trust", "Background", "Inbox"]);
    await user.click(screen.getByRole("tab", { name: "Inbox" }));
    expect(await screen.findByRole("list", { name: "Needs you" })).toBeVisible();
  });

  it("is not drawn while Crew Preview is off: no tab list at all", async () => {
    const { CrewSurface } = await import("../autopilot/approvals/CrewSurface");
    mock.reset("default");
    const { container } = render(<CrewSurface tab="inbox" />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
  });
});
