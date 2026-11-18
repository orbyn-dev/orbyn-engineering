import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { installMockFetch } from "../../../../test-support";
import { FirstRunBanner } from "./FirstRunBanner";
import { TrustTab } from "./Trust";

/**
 * The first-run banner's two edges and Trust before it has read. The banner's copy and its one Keep, and Trust's figures and young
 * sample, are pages.test.tsx.
 */
let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

const urls = () => vi.mocked(fetch).mock.calls.map(([url]) => String(url));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");
const confirms = () => writes().filter(([url]) => String(url).includes("/api/crew/autopilot/confirm"));

describe("the first-run banner", () => {
  it("sends Change to Settings' Crew section, and writes nothing", async () => {
    mock.reset("crew-first-run");
    const user = userEvent.setup();
    render(<FirstRunBanner />);
    const banner = await screen.findByRole("region", { name: "Autopilot" });
    await user.click(within(banner).getByRole("button", { name: "Change" }));
    expect(window.location.hash).toBe("#/settings/crew");
    expect(writes()).toHaveLength(0);
  });

  it("says a Keep that could not be saved failed, keeps the banner, and saves on the next try", async () => {
    mock.reset("crew-first-run");
    mock.fail("/api/crew/autopilot/confirm", 1);
    const user = userEvent.setup();
    render(<FirstRunBanner />);
    const banner = await screen.findByRole("region", { name: "Autopilot" });
    await user.click(within(banner).getByRole("button", { name: "Keep Full" }));
    expect(await within(banner).findByRole("alert")).toHaveTextContent("Couldn’t save that. Try again.");
    expect(screen.getByRole("region", { name: "Autopilot" })).toBeVisible();
    expect(within(banner).getByRole("button", { name: "Keep Full" })).toBeEnabled();
    expect(confirms()).toHaveLength(1);
    // The next try saves and takes the banner away.
    await user.click(within(banner).getByRole("button", { name: "Keep Full" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Autopilot" })).toBeNull());
    expect(confirms()).toHaveLength(2);
  });
});

describe("Trust before it has read", () => {
  it("says it is reading trust until the service answers, and no table before it", async () => {
    mock.reset("crew");
    const real = vi.mocked(fetch).getMockImplementation()!;
    let answer = () => {};
    const answered = new Promise<void>((resolve) => (answer = resolve));
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("/api/crew/trust")) await answered;
      return real(input, init);
    });
    render(<TrustTab />);
    expect(await screen.findByRole("status")).toHaveTextContent("Reading trust");
    expect(screen.queryByRole("table", { name: "By engine" })).toBeNull();
    await act(async () => answer());
    expect(await screen.findByRole("table", { name: "By engine" })).toBeVisible();
    expect(urls().filter((url) => url.includes("/api/crew/trust"))).toHaveLength(1);
  });

  it("names an unreadable trust page, and reads it again on Retry", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/trust", 1);
    const user = userEvent.setup();
    render(<TrustTab />);
    expect(await screen.findByText("Can't read trust")).toBeVisible();
    expect(screen.queryByRole("table", { name: "By engine" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("table", { name: "By engine" })).toBeVisible();
    expect(urls().filter((url) => url.includes("/api/crew/trust"))).toHaveLength(2);
    expect(writes()).toHaveLength(0);
  });
});
