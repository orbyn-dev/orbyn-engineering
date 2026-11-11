import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { AccountSection } from "./AccountSection";
import { SyncTitleButton, useSignedInAccount } from "./SyncLine";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

function Narrow() {
  const account = useSignedInAccount();
  return account ? <SyncTitleButton view={account.view} reload={account.reload} /> : <span>no status</span>;
}

describe("the sync popover on a computer whose settings are not connected", () => {
  it("says so neutrally and Connect settings opens the connect dialog on the Account page", async () => {
    mock.reset("account");
    mock.setAccount({ connectNeeded: true });
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: "correct horse battery" }) });
    await fetch("/api/account/connect/later", { method: "POST" });
    const user = userEvent.setup();
    render(<><Narrow /><AccountSection /></>);
    await user.click(await screen.findByRole("button", { name: "Sync: Not connected" }));
    const popover = await screen.findByRole("dialog", { name: "Sync" });
    expect(popover).toHaveTextContent("Not connected");
    expect(popover).toHaveTextContent("This computer’s settings stay here. Connect them to your account to sync appearance, keybindings and chat and diff preferences.");
    expect(popover).not.toHaveTextContent("waiting");
    expect(within(popover).queryByRole("button", { name: "Sync now" })).toBeNull();
    await user.click(within(popover).getByRole("button", { name: "Connect settings" }));
    expect(await screen.findByRole("dialog", { name: "Sync your settings?" })).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Sync" })).toBeNull();
  });
});

describe("the sync icon in the title bar", () => {
  it("is not there while the gate is off", async () => {
    render(<Narrow />);
    await waitFor(() => expect(mock.handle("GET", "/api/account").body).toMatchObject({ available: false }));
    expect(await screen.findByText("no status")).toBeVisible();
    expect(screen.queryByRole("button", { name: /^Sync:/ })).toBeNull();
  });

  it("is not there while signed out", async () => {
    mock.reset("account");
    render(<Narrow />);
    expect(await screen.findByText("no status")).toBeVisible();
    expect(screen.queryByRole("button", { name: /^Sync:/ })).toBeNull();
  });

  it("in a narrow window is one button that opens the same popover", async () => {
    mock.reset("account");
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: "correct horse battery" }) });
    const user = userEvent.setup();
    render(<Narrow />);
    await user.click(await screen.findByRole("button", { name: "Sync: Synced" }));
    expect(await screen.findByRole("dialog", { name: "Sync" })).toHaveTextContent("Up to date");
  });
});
