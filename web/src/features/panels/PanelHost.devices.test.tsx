import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createDevicesMock, createDevicesPlannedMock } from "../../../mock/devices";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { installMockFetch, setViewportWidth } from "../../test-support";
import { useConnection } from "../../product/state/connection";
import { useUi } from "../../product/state/ui";
import { setDevicesApi } from "../devices";
import { setDevicesPlannedApi } from "../devices/planned";
import { PanelHost } from "./PanelHost";

const thread = { id: "thread-1" } as Thread;
const tabs = () => screen.getAllByRole("tab").map((tab) => tab.getAttribute("aria-label"));

beforeEach(() => {
  // The panel host draws the real panels; the service is the only fake.
  installMockFetch();
  vi.spyOn(api, "diff").mockResolvedValue([]);
  vi.spyOn(api, "keybindings").mockResolvedValue({ rules: [], issues: [] } as never);
  useConnection.setState({ offline: false, serviceDown: false });
  useUi.setState({ panelOpen: true, panelTab: null, terminalOpen: false, panelTabOrder: ["diff", "browser", "activity", "side", "tools", "linked", "review", "devices"], panelTabsClosed: ["tools", "linked", "review"] });
});
afterEach(() => { setDevicesApi(null); setDevicesPlannedApi(null); vi.unstubAllGlobals(); });

const host = () => render(<PanelHost thread={thread} mode="docked" closing={false} moves={false} content={{}} maximized={false} onToggleMaximize={() => {}} onReturn={() => {}} />);
const openLauncher = async () => {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Open a tab" }));
  return user;
};

it("draws no Devices tab, and a disabled launcher entry with the reason, when the service answers 404", async () => {
  const fetching = vi.fn(async () => new Response("{}", { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  useUi.setState({ panelTabsClosed: ["tools", "linked", "review"] });
  host();
  await openLauncher();
  const item = await screen.findByRole("menuitem", { name: "Devices" });
  expect(item).toBeDisabled();
  expect(item).toHaveTextContent("Devices are not available on this service.");
  expect(tabs()).not.toContain("Devices");
});

it("leaves the launcher entry out when the list read fails", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 500 })));
  host();
  await openLauncher();
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByRole("menuitem", { name: "Devices" })).toBeNull();
});

it("shows a Devices tab and its list once the service reports devices, and the launcher can open it", async () => {
  setDevicesApi(createDevicesMock());
  setDevicesPlannedApi(createDevicesPlannedMock());
  useUi.setState({ panelTabsClosed: ["tools", "linked", "review", "devices"] });
  host();
  const user = await openLauncher();
  await user.click(await screen.findByRole("menuitem", { name: "Devices" }));
  expect(await screen.findByRole("list", { name: "Devices" })).toBeVisible();
  await waitFor(() => expect(tabs()).toContain("Devices"));
});

it("disables the entry with the platform's sentence when no tool is found and no setup service answers", async () => {
  setDevicesApi(createDevicesMock({
    devices: [],
    platforms: [
      { platform: "android", available: false, code: "tool_missing", message: "Install the Android platform-tools." },
      { platform: "ios", available: false, code: "tool_missing", message: "Xcode was not found. Install Xcode, then try again." },
    ],
  }));
  host();
  await openLauncher();
  const item = await screen.findByRole("menuitem", { name: "Devices" });
  expect(item).toBeDisabled();
  expect(item).toHaveTextContent("Install the Android platform-tools.");
});

it("opens the setup hub when no platform is available and the Planned setup client answers", async () => {
  setDevicesApi(createDevicesMock({
    devices: [],
    platforms: [
      { platform: "android", available: false, code: "tool_missing", message: "Install the Android platform-tools." },
      { platform: "ios", available: false, code: "tool_missing", message: "Xcode was not found. Install Xcode, then try again." },
    ],
  }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  host();
  const user = await openLauncher();
  await user.click(await screen.findByRole("menuitem", { name: "Devices" }));
  expect(await screen.findByRole("heading", { name: "Choose a platform" })).toBeVisible();
  expect(screen.queryByRole("list", { name: "Devices" })).toBeNull();
});

it("does not show the Devices entry on a phone", async () => {
  setViewportWidth(390);
  setDevicesApi(createDevicesMock());
  setDevicesPlannedApi(createDevicesPlannedMock());
  host();
  const user = await openLauncher();
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByRole("menuitem", { name: "Devices" })).toBeNull();
  expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).not.toContain("DevicesBeta");
  await user.keyboard("{Escape}");
});
