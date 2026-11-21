import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { useConnection } from "../../../../product/state/connection";
import { SettingsPage } from "../../../settings/SettingsPage";
import { installMockFetch } from "../../../../test-support";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false, crewPreview: false });
  window.location.hash = "";
});
afterEach(() => {
  window.location.hash = "";
});

const crewReads = () => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes("/api/crew/capacity"));

describe("Settings › Crew › Review capacity, reached from the app (Crew Designs v23, the Review capacity row)", () => {
  it("with Crew Preview on, the Crew page has a Review capacity row that opens the live screen", async () => {
    mock.reset("crew-capacity");
    useConnection.setState({ crewPreview: true });
    const user = userEvent.setup();
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    const row = await screen.findByRole("region", { name: "Review capacity" });
    expect(within(row).getByText(/How many High and Medium changes you can have waiting before Crew pauses new Low-priority starts/)).toBeInTheDocument();
    // The row only offers the page: nothing of capacity is read before it is opened.
    expect(crewReads()).toHaveLength(0);
    await user.click(within(row).getByRole("button", { name: "Open Review capacity" }));
    expect(window.location.hash).toBe("#/settings/crew/capacity");
  });

  it("the address opens the live screen on the service's own read, and Settings › Crew is the way back", async () => {
    mock.reset("crew-capacity");
    useConnection.setState({ crewPreview: true });
    window.location.hash = "#/settings/crew/capacity";
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    expect(await screen.findByText("Crew › Review capacity")).toBeVisible();
    expect(await screen.findByText("8 changes")).toBeVisible();
    expect(screen.queryByRole("region", { name: "Review capacity" })).toBeNull();
    // Nothing of Crew was written by opening it.
    expect(vi.mocked(fetch).mock.calls.filter(([url, init]) => init?.method === "POST" && String(url).includes("/api/crew")).map(([url]) => String(url))).toEqual([]);
  });

  it("with Crew Preview off, nothing shows and no capacity route is read", async () => {
    mock.reset("crew-capacity");
    window.location.hash = "#/settings/crew/capacity";
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThan(0));
    expect(screen.queryByRole("link", { name: "Crew" })).toBeNull();
    expect(screen.queryByText("Crew › Review capacity")).toBeNull();
    expect(crewReads()).toHaveLength(0);
  });
});
