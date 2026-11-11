import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { AccountSection } from "./AccountSection";
import { announceAccountChange } from "./changes";
import { DevicesNote } from "./DeviceRows";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

const PASSWORD = "correct horse battery";

async function signedInPage(knobs: () => void = () => undefined) {
  mock.reset("account");
  await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: PASSWORD }) });
  knobs();
  const user = userEvent.setup();
  render(<AccountSection />);
  await screen.findByText("Signed in on this device · This computer");
  return user;
}

async function openDevices(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Manage devices" }));
  return screen.findByRole("heading", { name: "Devices" });
}

describe("the devices", () => {
  it("lists them on the Account page and on their own page, with this device marked", async () => {
    const user = await signedInPage();
    expect(await screen.findByRole("heading", { name: "Devices · 3" })).toBeVisible();
    await openDevices(user);
    const rows = screen.getAllByRole("listitem");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual(["This computer", "iPhone", "Mac mini"]);
    expect(within(rows[0] as HTMLElement).getByText("This device")).toBeVisible();
    expect(within(rows[0] as HTMLElement).queryByRole("button", { name: /Remove/ })).toBeNull();
    expect(within(rows[1] as HTMLElement).getByText("Orbyn Planner")).toBeVisible();
    expect(within(rows[2] as HTMLElement).getByText("Orbyn Engineering")).toBeVisible();
    expect(within(rows[1] as HTMLElement).getByText(/^Trusted since .* · Seen /)).toBeVisible();
    expect(screen.getByText(/Every trusted device can open all of your Orbyn data/)).toBeVisible();
    expect(screen.queryByText(/Too new/)).toBeNull();
    expect(screen.queryByRole("button", { name: /Rename/ })).toBeNull();
  });

  it("removes one device only after the password is typed for that device", async () => {
    const user = await signedInPage();
    await openDevices(user);
    await user.click(screen.getByRole("button", { name: "Remove Mac mini" }));
    expect(await screen.findByRole("dialog", { name: "Remove Mac mini?" })).toHaveTextContent("It will be signed out of your account and stop syncing.");
    // Cancel is the default: the device stays.
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("listitem", { name: "Mac mini" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Remove Mac mini" }));
    await user.click(await screen.findByRole("button", { name: "Remove device" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    expect(within(confirm).getByText("To remove Mac mini")).toBeVisible();
    expect(within(confirm).getByText("This confirms removing this device.")).toBeVisible();
    await user.type(within(confirm).getByLabelText("Password"), "not it");
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    expect(await within(confirm).findByText("That isn’t your password. Check it and try again.")).toBeVisible();
    expect(screen.getByRole("listitem", { name: "Mac mini" })).toBeVisible();
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    expect(await screen.findByText("Mac mini removed.")).toBeVisible();
    expect(screen.getByText(/It was signed out and stopped syncing\./)).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "Mac mini" })).toBeNull());
    expect(screen.getByRole("listitem", { name: "iPhone" })).toBeVisible();
  });

  it("says how to add a device when this one is the only one", async () => {
    const user = await signedInPage(() => mock.setAccount({ devices: { onlyThis: true } }));
    await openDevices(user);
    expect(await screen.findByText("This is the only device signed in.")).toBeVisible();
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });

  it("when the list cannot be read says so, keeps the last list dimmed and Try again reads it again", async () => {
    const user = await signedInPage();
    await openDevices(user);
    await screen.findByRole("listitem", { name: "iPhone" });
    mock.setAccount({ devices: { down: true } });
    act(() => announceAccountChange());
    expect(await screen.findByText("Couldn’t load your devices.")).toBeVisible();
    expect(screen.getByRole("listitem", { name: "iPhone" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Remove iPhone" })).toBeNull();
    mock.setAccount({ devices: { down: false } });
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByText("Couldn’t load your devices.")).toBeNull());
    expect(screen.getByRole("button", { name: "Remove iPhone" })).toBeVisible();
  });

  it("adds the Windows note only on Windows", () => {
    const { rerender } = render(<DevicesNote platform="macos" />);
    expect(screen.queryByText(/any program you run can act as this device/)).toBeNull();
    rerender(<DevicesNote platform="windows" />);
    expect(screen.getByText(/any program you run can act as this device/)).toBeVisible();
  });
});

describe("sign out", () => {
  it("signs out of this computer only and says what stayed", async () => {
    const user = await signedInPage();
    await user.click(screen.getByRole("button", { name: "Sign out" }));
    const dialog = await screen.findByRole("dialog", { name: "Sign out of this computer?" });
    expect(dialog).toHaveTextContent("Other devices stay signed in.");
    await user.click(within(dialog).getByRole("button", { name: "Sign out" }));
    expect(await screen.findByText("Signed out. Your threads, projects and settings are still on this computer.")).toBeVisible();
    expect(screen.getByText("Use an account to sync your settings")).toBeVisible();
  });

  it("with changes that have not synced, keeps them unless the person discards them", async () => {
    const user = await signedInPage(() => mock.setAccountWaiting(3));
    await user.click(screen.getByRole("button", { name: "Sign out" }));
    const dialog = await screen.findByRole("dialog", { name: "Sign out with 3 changes not synced?" });
    expect(within(dialog).getByLabelText(/Keep unsynced changes on this device/)).toBeChecked();
    expect(within(dialog).getByRole("button", { name: "Sync now, then sign out" })).toBeVisible();
    await user.click(within(dialog).getByLabelText(/Discard them/));
    await user.click(within(dialog).getByRole("button", { name: "Sign out" }));
    await screen.findByText(/^Signed out\./);
    const call = vi.mocked(fetch).mock.calls.find(([url]) => String(url) === "/api/account/sign-out");
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ discard_unsynced: true });
  });

  it("offline needs no connection, says the device list keeps this computer and offers no Sync now", async () => {
    const user = await signedInPage(() => {
      mock.setAccountWaiting(2);
      mock.setAccount({ sync: ["offline"] });
    });
    await user.click(screen.getByRole("button", { name: "Sign out" }));
    const dialog = await screen.findByRole("dialog", { name: "Sign out while offline?" });
    expect(dialog).toHaveTextContent("its entry in your device list stays until you remove it from another device");
    expect(within(dialog).queryByRole("button", { name: "Sync now, then sign out" })).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Sign out" }));
    expect(await screen.findByText(/^Signed out\./)).toBeVisible();
  });

  it("syncs first when asked to, then signs out keeping nothing back", async () => {
    const user = await signedInPage(() => mock.setAccountWaiting(1));
    await user.click(screen.getByRole("button", { name: "Sign out" }));
    await user.click(await screen.findByRole("button", { name: "Sync now, then sign out" }));
    await screen.findByText(/^Signed out\./);
    const urls = vi.mocked(fetch).mock.calls.map(([url]) => String(url));
    expect(urls.indexOf("/api/account/sync/now")).toBeGreaterThan(-1);
    expect(urls.indexOf("/api/account/sync/now")).toBeLessThan(urls.indexOf("/api/account/sign-out"));
  });

  it("signs out everywhere only after the password for that action", async () => {
    const user = await signedInPage();
    await user.click(screen.getByRole("button", { name: "Sign out everywhere…" }));
    const dialog = await screen.findByRole("dialog", { name: "Sign out everywhere?" });
    expect(dialog).toHaveTextContent("Settings sync on all 3 devices");
    await user.click(within(dialog).getByRole("button", { name: "Sign out everywhere" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    expect(within(confirm).getByText("This confirms one action.")).toBeVisible();
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    expect(await screen.findByText(/^Signed out everywhere\./)).toBeVisible();
    expect(screen.getByText(/This device stays in your device list until someone removes it from another device\./)).toBeVisible();
  });

  it("while deletion is scheduled explains instead of asking, and this computer can still sign out", async () => {
    const user = await signedInPage(() => mock.setAccount({ devices: { deletionScheduled: true } }));
    expect(screen.getByText("Off while your account is waiting to be deleted.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Sign out everywhere…" }));
    const dialog = await screen.findByRole("dialog", { name: "Deletion is scheduled" });
    expect(dialog).toHaveTextContent("Orbyn can’t sign out every session right now.");
    expect(screen.queryByRole("dialog", { name: "Confirm it’s you" })).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Sign out of this computer" }));
    expect(await screen.findByRole("dialog", { name: "Sign out of this computer?" })).toBeVisible();
  });
});

describe("a pause that is not the person’s choice", () => {
  it("deletion scheduled: the title stays Paused and Resume is off with the reason", async () => {
    await signedInPage(() => {
      mock.setAccountWaiting(2);
      mock.setAccount({ sync: ["paused", "deletion_scheduled"] });
    });
    expect(screen.getByText(/^Paused · 2 changes waiting/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Resume sync" })).toBeDisabled();
    expect(screen.getByText("Resume sync is not available while deletion is scheduled.")).toBeVisible();
  });

  it("protection changing: nothing to press", async () => {
    await signedInPage(() => mock.setAccount({ sync: ["paused", "protection_advanced"] }));
    expect(screen.getByText(/^Paused/)).toBeVisible();
    expect(screen.queryByRole("button", { name: /Resume sync|Try again|Open keybindings|Pause$/ })).toBeNull();
  });

  it("a bad keybindings file: Open keybindings goes to the file’s page", async () => {
    const user = await signedInPage(() => mock.setAccount({ sync: ["paused", "invalid_keyboard_file"] }));
    await user.click(screen.getByRole("button", { name: "Open keybindings" }));
    expect(window.location.hash).toBe("#/settings/keybindings");
  });

  it("an unknown reason: says so plainly and offers Try again", async () => {
    const user = await signedInPage(() => mock.setAccount({ sync: ["paused", "session_ended"] }));
    const again = screen.getByRole("button", { name: "Try again" });
    await user.click(again);
    expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url) === "/api/account/sync/now")).toBe(true);
  });
});
