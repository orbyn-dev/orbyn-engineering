import { render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { SidebarNav } from "./SidebarChrome";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
});

const labels = () => within(document.querySelector(".sidebar-nav") as HTMLElement).getAllByRole("link").map((link) => link.textContent);

describe("the Crew row of the left bar", () => {
  it("sits after Pull requests while Crew Preview is on", async () => {
    mock.reset("crew");
    render(<SidebarNav route={{ name: "home" }} ledger={false} />);
    await screen.findByRole("link", { name: /Crew/ });
    const names = labels();
    expect(names.indexOf("Crew" + "Preview")).toBe(names.indexOf("Pull requests") + 1);
  });

  it("is not there while Crew Preview is off", async () => {
    mock.reset("default");
    render(<SidebarNav route={{ name: "home" }} ledger={false} />);
    await screen.findByRole("link", { name: "Pull requests" });
    await waitFor(() => expect(labels()).not.toContain("CrewPreview"));
    expect(screen.queryByRole("link", { name: /^Crew/ })).toBeNull();
  });
});
