import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "../settings/SettingsPage";
import { SidebarFoot } from "../workspace/sidebar/SidebarChrome";
import { SyncTitleButton, useSignedInAccount } from "./SyncLine";

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

  it("has no Account item in the Settings list", async () => {
    render(<SettingsPage section={null} onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toBeVisible();
    expect(screen.queryByRole("link", { name: "Account" })).toBeNull();
  });

  it("goes to General when the address names Account", async () => {
    render(<SettingsPage section="account" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
  });
});

describe("the gate on", () => {
  it("lists Account after General", async () => {
    mock.reset("account");
    render(<SettingsPage section={null} onClose={() => undefined} />);
    const links = await screen.findAllByRole("link");
    const names = links.map((link) => link.textContent);
    await waitFor(() => expect(screen.getByRole("link", { name: "Account" })).toBeVisible());
    expect(names.indexOf("General")).toBeLessThan(screen.getAllByRole("link").map((l) => l.textContent).indexOf("Account"));
  });

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

  it("in a narrow window the status is one title-bar button that opens the same popover", async () => {
    await signedIn();
    const user = userEvent.setup();
    function Narrow() {
      const account = useSignedInAccount();
      return account ? <SyncTitleButton view={account.view} reload={account.reload} /> : null;
    }
    render(<Narrow />);
    await user.click(await screen.findByRole("button", { name: "Sync: Synced" }));
    expect(await screen.findByRole("dialog", { name: "Sync" })).toHaveTextContent("Up to date");
  });
});
