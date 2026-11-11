import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { SidebarFoot } from "./SidebarChrome";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

async function signedIn() {
  mock.reset("account");
  await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: "correct horse battery" }) });
}

describe("the gate off", () => {
  it("keeps the plain Settings row and no sync status", async () => {
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    expect(await screen.findByRole("link", { name: "Settings" })).toBeVisible();
    await waitFor(() => expect(mock.handle("GET", "/api/account").body).toMatchObject({ available: false }));
    expect(screen.queryByRole("button", { name: /Account and sync/ })).toBeNull();
  });

});

describe("the gate on", () => {
  it("signed out the footer is still the plain Settings row", async () => {
    mock.reset("account");
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    expect(await screen.findByRole("link", { name: "Settings" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Account and sync/ })).toBeNull();
  });
});

describe("signed in", () => {
  it("shows the avatar, the quiet Synced status and the gear", async () => {
    await signedIn();
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    const status = await screen.findByRole("button", { name: "Account and sync: Synced" });
    expect(status).toHaveTextContent("D");
    expect(status).toHaveTextContent("Synced");
    expect(screen.getByRole("button", { name: "Settings" })).toBeVisible();
  });

  it("opens the popover with only the zone that exists, and Pause then Resume change the line", async () => {
    await signedIn();
    const user = userEvent.setup();
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    await user.click(await screen.findByRole("button", { name: "Account and sync: Synced" }));
    const popover = await screen.findByRole("dialog", { name: "Sync" });
    expect(popover).toHaveTextContent("Up to date");
    expect(popover).toHaveTextContent("Your settings are the same on every signed-in computer.");
    expect(popover).toHaveTextContent("0 waiting");
    expect(popover.querySelectorAll(".ac-zone")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Pause sync" }));
    expect(await screen.findByRole("button", { name: "Account and sync: Paused" })).toBeVisible();
    expect(await screen.findByText("You paused sync. Changes are kept on this computer and don’t send or arrive until you resume.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Sync now" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Resume sync" }));
    expect(await screen.findByRole("button", { name: "Account and sync: Synced" })).toBeVisible();
  });

  it("says Syncing… while changes wait, and counts them in the popover", async () => {
    await signedIn();
    mock.setAccountWaiting(3);
    const user = userEvent.setup();
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    await user.click(await screen.findByRole("button", { name: "Account and sync: Syncing…" }));
    expect(await screen.findByRole("dialog", { name: "Sync" })).toHaveTextContent("3 waiting");
  });

  it("closes the popover with Escape", async () => {
    await signedIn();
    const user = userEvent.setup();
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    await user.click(await screen.findByRole("button", { name: "Account and sync: Synced" }));
    await screen.findByRole("dialog", { name: "Sync" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Sync" })).toBeNull());
  });

  it("offline: the popover says the changes are saved here and Sync now is off", async () => {
    await signedIn();
    mock.setAccountWaiting(3);
    mock.setAccount({ sync: ["offline"] });
    const user = userEvent.setup();
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    await user.click(await screen.findByRole("button", { name: "Account and sync: Offline · 3 waiting" }));
    const popover = await screen.findByRole("dialog", { name: "Sync" });
    expect(popover).toHaveTextContent("No connection. 3 changes are saved on this computer and will send when you’re back online.");
    expect(within(popover).getByRole("button", { name: "Sync now" })).toBeDisabled();
    expect(within(popover).getByRole("button", { name: "Pause sync" })).toBeEnabled();
  });

  it("couldn’t sync: the popover says what was kept and offers Try again", async () => {
    await signedIn();
    mock.setAccountWaiting(1);
    mock.setAccount({ sync: ["paused", "zone_failed"] });
    const user = userEvent.setup();
    render(<SidebarFoot route={{ name: "home" }} ledger={false} />);
    await user.click(await screen.findByRole("button", { name: "Account and sync: Couldn’t sync" }));
    const popover = await screen.findByRole("dialog", { name: "Sync" });
    expect(popover).toHaveTextContent("The server didn’t accept 1 change. Nothing was lost; it is kept here.");
    expect(within(popover).getByRole("button", { name: "Try again" })).toBeEnabled();
  });
});
