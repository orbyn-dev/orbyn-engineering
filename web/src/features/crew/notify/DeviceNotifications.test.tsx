// Settings › Crew › Device notifications (Crew Designs v140, frames `kc10-notify-settings*`). Black-box through the
// rendered screen: the network is the mock and the browser's Notification global is the only other stand-in.
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { routeHref } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { CrewSettingsSection } from "../autopilot/approvals/CrewSettings";
import { DeviceNotifications } from "./DeviceNotifications";

const OFF = "Off. Nothing is raised on this computer. Items still show in Needs you, on the board and as toasts in the app. The digest notification has its own switch below.";
const THREADS_OFF = "Thread notifications in Settings › General is Off, so no system notification is raised. Choose Notifications or Both there.";
const DENIED = "Your computer is blocking notifications from Orbyn. Allow them in System Settings › Notifications (on Windows: Settings › System › Notifications), then come back. Until then Crew items show in Needs you and as toasts in the app.";
const NOT_ASKED = "Not asked yet. Choose Notifications or Both in Settings › General to allow it.";
const ALLOWED = "Allowed by your computer. Thread notifications in Settings › General is on.";
const SENTENCE = "A notification on this computer the first time something needs you: a question, a command to approve, a merge or a review that waits, a watch that turned failing, a start that was refused, a task that stopped. One per item, never repeated; items that arrive together are one. Items still appear in Needs you whatever this says. It uses Thread notifications in Settings › General.";

/** The browser's Notification global: only the static permission the screen reads. */
class ComputerNotification {
  static permission: NotificationPermission = "granted";
}

let mock: MockApi;

const toggle = () => screen.getByRole("switch", { name: "Device notifications" });
const setPrefs = (crewNotifications: boolean, threadNotifications: boolean) =>
  useUi.setState({ crewNotifications, threadNotifications });

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  // Every test states both prefs, so no test leans on another one's leftovers.
  setPrefs(true, true);
  ComputerNotification.permission = "granted";
  vi.stubGlobal("Notification", ComputerNotification);
});

describe("Settings › Crew › Device notifications", () => {
  it("starts on", () => {
    expect(useUi.getState().crewNotifications).toBe(true);
    render(<DeviceNotifications />);
    expect(toggle()).toHaveAttribute("aria-checked", "true");
  });

  it("draws the design's label, sentence and switch", () => {
    render(<DeviceNotifications />);
    expect(screen.getByText("Device notifications")).toBeVisible();
    expect(screen.getByText(SENTENCE)).toBeVisible();
    expect(toggle()).toHaveTextContent("On");
  });

  it("says nothing is raised and what still shows when the switch is off", () => {
    setPrefs(false, false);
    render(<DeviceNotifications />);
    expect(screen.getByText(OFF)).toBeVisible();
    expect(toggle()).toHaveAttribute("aria-checked", "false");
  });

  it("points at General when Thread notifications is off, and leaves its own switch on", () => {
    setPrefs(true, false);
    render(<DeviceNotifications />);
    expect(screen.getByText(THREADS_OFF)).toBeVisible();
    expect(screen.getByRole("button", { name: "Open General" })).toBeVisible();
    expect(toggle()).toHaveAttribute("aria-checked", "true");
  });

  it("opens General from that status line", () => {
    setPrefs(true, false);
    render(<DeviceNotifications />);
    fireEvent.click(screen.getByRole("button", { name: "Open General" }));
    expect(window.location.hash).toBe(routeHref({ name: "settings", section: "general" }));
  });

  it("says how to allow notifications when the computer blocks them, and keeps the switch on", () => {
    ComputerNotification.permission = "denied";
    render(<DeviceNotifications />);
    expect(screen.getByText(DENIED)).toBeVisible();
    expect(toggle()).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByRole("button", { name: "Open General" })).toBeNull();
  });

  it("says notifications are allowed when the computer grants them and General is on", () => {
    render(<DeviceNotifications />);
    expect(screen.getByText(ALLOWED)).toBeVisible();
  });

  it("says where to allow notifications while the permission is not asked yet, with no button and no prompt", () => {
    ComputerNotification.permission = "default";
    const ask = vi.fn();
    Object.assign(ComputerNotification, { requestPermission: ask });
    render(<DeviceNotifications />);
    expect(screen.getByText(NOT_ASKED)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Open General" })).toBeNull();
    expect(ask).not.toHaveBeenCalled();
  });

  it("shows no status line when the browser has no Notification global", () => {
    vi.unstubAllGlobals();
    Reflect.deleteProperty(window, "Notification");
    render(<DeviceNotifications />);
    expect(screen.queryByRole("status")).toBeNull();
    expect(toggle()).toBeVisible();
  });

  it("turns the preference off with the switch, and back on", () => {
    render(<DeviceNotifications />);
    fireEvent.click(toggle());
    expect(useUi.getState().crewNotifications).toBe(false);
    expect(toggle()).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(OFF)).toBeVisible();

    fireEvent.click(toggle());
    expect(useUi.getState().crewNotifications).toBe(true);
    expect(toggle()).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText(ALLOWED)).toBeVisible();
  });

  it("reads the permission again when the window gains focus", () => {
    ComputerNotification.permission = "denied";
    render(<DeviceNotifications />);
    expect(screen.getByText(DENIED)).toBeVisible();

    ComputerNotification.permission = "granted";
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(screen.getByText(ALLOWED)).toBeVisible();
    expect(screen.queryByText(DENIED)).toBeNull();
  });

  it("sits on Settings › Crew above the Daily digest row", () => {
    mock.reset("crew");
    render(<CrewSettingsSection />);
    const row = toggle().closest(".dn-row");
    const digest = screen.getByText("Daily digest").closest(".dg-row");
    expect(row).not.toBeNull();
    expect(digest).not.toBeNull();
    expect(row!.compareDocumentPosition(digest!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
