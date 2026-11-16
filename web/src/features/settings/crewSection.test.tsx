import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { useConnection } from "../../product/state/connection";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "./SettingsPage";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false, crewPreview: false });
  window.location.hash = "";
});

const crewCalls = () => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes("/api/crew"));

describe("the Crew category of Settings", () => {
  it("is there while the service says Crew Preview is on", async () => {
    mock.reset("crew");
    useConnection.setState({ crewPreview: true });
    render(<SettingsPage section={null} onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "Crew" })).toBeVisible();
  });

  it("is not there while it is off or not known yet, and Settings asks no Crew route (a 404 there is logged by the browser)", async () => {
    mock.reset("crew");
    render(<SettingsPage section={null} onClose={() => undefined} />);
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThan(0));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(screen.queryByRole("link", { name: "Crew" })).toBeNull();
    expect(crewCalls().filter(([url]) => String(url).includes("/api/crew/preview"))).toHaveLength(0);
  });

  it("falls back to General when Crew is opened while it is off", async () => {
    mock.reset("default");
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toBeVisible();
    expect(screen.queryByRole("heading", { name: /Autopilot/ })).toBeNull();
  });
});
