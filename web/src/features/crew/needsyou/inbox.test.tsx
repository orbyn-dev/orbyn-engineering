import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { NeedsYouInbox } from "./NeedsYouInbox";

const PASSWORD = "correct horse battery";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
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
    expect(items[0]).toHaveTextContent("Watch on main · 9be4410");
    expect(items[0]).toHaveTextContent("Nothing has been reverted. Main stays as it is until you choose.");
    expect(items[1]).toHaveTextContent("Review: It used its whole turn budget before the checks passed");
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

  it("a background run's Open goes to the Background tab; the break on main has no screen to open yet", async () => {
    const { user } = await open("crew");
    const run = (await screen.findByText(/^Review: It used its whole turn budget/)).closest("li") as HTMLElement;
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

  it("names the source it could not read and shows the rest", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/stops", 1, 503, "unavailable");
    render(<NeedsYouInbox />);
    expect(await screen.findByText("Can’t read stops. The rest is shown.")).toBeVisible();
    expect(await list()).toBeVisible();
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

  it("Approve asks Confirm it’s you first; the nonce goes on the existing route, once, and the password goes nowhere else", async () => {
    const { user } = await open("crew-held-account");
    await user.click(within(await row(/DELETE FROM orders/)).getByRole("button", { name: /^Approve/ }));
    const dialog = screen.getByRole("dialog", { name: "Confirm it’s you" });
    expect(calls("approve-once")).toHaveLength(0);
    await user.type(within(dialog).getByLabelText("Password"), PASSWORD);
    await user.click(within(dialog).getByRole("button", { name: "Approve once" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const sent = calls("/api/crew/approvals/hold-orders/approve-once");
    expect(sent).toHaveLength(1);
    expect(new Headers(sent[0]?.[1]?.headers).get("x-orbyn-confirm")).toMatch(/^nonce-/);
    expect(writes().map(([url]) => String(url)).filter((url) => url.includes("/api/crew"))).toEqual([expect.stringContaining("approve-once")]);
  });

  it("Approve is off without a signed-in account, with the reason; Deny still works", async () => {
    const { user } = await open("crew-held");
    const orders = await row(/DELETE FROM orders/);
    const approve = within(orders).getByRole("button", { name: /^Approve/ });
    expect(approve).toBeDisabled();
    expect(approve).toHaveAttribute("title", "Approving needs a signed-in account.");
    expect(within(orders).getByRole("button", { name: /^Deny/ })).toBeEnabled();
    await user.click(within(orders).getByRole("button", { name: /^Deny/ }));
    await waitFor(() => expect(calls("/api/crew/approvals/hold-orders/deny")).toHaveLength(1));
  });

  it("from another computer, both answers are off with the reason", async () => {
    useConnection.setState({ remote: true });
    await open("crew-held-account");
    const orders = await row(/DELETE FROM orders/);
    for (const name of [/^Approve/, /^Deny/]) {
      const button = within(orders).getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute("title", "Do this from the computer the app runs on.");
    }
  });

  it("offers no merge, revert or other write: only Approve, Deny and Open exist", async () => {
    await open("crew-held-account");
    await list();
    const labels = screen.getAllByRole("button").map((one) => (one.textContent ?? "").trim());
    for (const label of labels) expect(label).toMatch(/^(Open|Approve|Deny|Needs you \d+|Approvals \d+|Reviews \d+|Main \d+|Stopped \d+)$/);
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
