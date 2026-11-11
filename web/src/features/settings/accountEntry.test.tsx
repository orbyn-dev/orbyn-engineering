import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "./SettingsPage";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

describe("the Account category in Settings", () => {
  it("is not in the list while the gate is off", async () => {
    render(<SettingsPage section={null} onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toBeVisible();
    expect(screen.queryByRole("link", { name: "Account" })).toBeNull();
  });

  it("goes to General when the address names Account and the gate is off", async () => {
    render(<SettingsPage section="account" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
  });

  it("is listed after General while the gate is on", async () => {
    mock.reset("account");
    render(<SettingsPage section={null} onClose={() => undefined} />);
    await waitFor(() => expect(screen.getByRole("link", { name: "Account" })).toBeVisible());
    const names = screen.getAllByRole("link").map((link) => link.textContent);
    expect(names.indexOf("General")).toBeLessThan(names.indexOf("Account"));
  });
});
