import { render, screen, waitFor, within } from "@testing-library/react";
import { createElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { App } from "../../../app/App";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { SidebarNav } from "./SidebarChrome";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false, crewPreview: false });
  window.location.hash = "";
});

const labels = () => within(document.querySelector(".sidebar-nav") as HTMLElement).getAllByRole("link").map((link) => link.textContent);
const crewCalls = () => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes("/api/crew"));

describe("the Crew row of the left bar", () => {
  it("sits after Pull requests while the service says Crew Preview is on", () => {
    useConnection.setState({ crewPreview: true });
    render(<SidebarNav route={{ name: "home" }} ledger={false} />);
    const names = labels();
    expect(names.indexOf("CrewPreview")).toBe(names.indexOf("Pull requests") + 1);
  });

  it("is not there while Crew Preview is off, and the shell asks no Crew route", () => {
    render(<SidebarNav route={{ name: "home" }} ledger={false} />);
    expect(labels()).not.toContain("CrewPreview");
    expect(crewCalls()).toHaveLength(0);
  });

  it("is known from the info the app reads at start: on in a Crew scenario, off otherwise, with no Crew request either way", async () => {
    mock.reset("crew");
    const on = render(createElement(App));
    expect(await screen.findByRole("link", { name: /^Crew\s*Preview/ })).toBeVisible();
    expect(crewCalls().filter(([url]) => String(url).includes("/api/crew/preview"))).toHaveLength(0);
    on.unmount();
    useConnection.setState({ crewPreview: false });
    mock.reset("default");
    render(createElement(App));
    await screen.findByRole("link", { name: "Pull requests" });
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes("/api/info"))).toBe(true));
    expect(screen.queryByRole("link", { name: /^Crew/ })).toBeNull();
  });

  it("fails closed: a start whose info cannot be read shows no Crew row, and the flag stays off", async () => {
    mock.reset("crew");
    mock.fail("/api/info", 20);
    render(createElement(App));
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes("/api/info"))).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByRole("link", { name: /^Crew/ })).toBeNull();
    expect(useConnection.getState().crewPreview).toBe(false);
    expect(crewCalls()).toHaveLength(0);
  });
});
