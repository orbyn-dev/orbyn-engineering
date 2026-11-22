import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createDevicesMock, createDevicesPlannedMock, deviceFixtures, stateFixture } from "../../../mock/devices";
import { useUi } from "../../product/state/ui";
import { DevicesFloat } from "./FloatingDevice";
import { DevicesPanel } from "./DevicesPanel";
import { setDevicesApi } from "./api";
import { setDevicesPlannedApi } from "./planned";

afterEach(() => { setDevicesApi(null); setDevicesPlannedApi(null); vi.unstubAllGlobals(); });

const prefs = (open: string[], active: string | null = open[0] ?? null) =>
  useUi.setState({ devices: { open, active, names: {}, float: null } });

it("shows the no-device state, without a Create a device button", async () => {
  setDevicesApi(createDevicesMock({ devices: [], platforms: [{ platform: "android", available: true }, { platform: "ios", available: true }] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(<DevicesPanel />);
  expect(await screen.findByText("No devices are listed.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /Create a device/i })).toBeNull();
});

it("shows a platform read failure beside the other platform's devices", async () => {
  setDevicesApi(createDevicesMock({
    devices: deviceFixtures().filter((one) => one.platform === "ios"),
    platforms: [
      { platform: "android", available: false, code: "tool_failed", message: "Couldn’t read the devices. Try again." },
      { platform: "ios", available: true },
    ],
  }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(<DevicesPanel />);
  expect(await screen.findByText("Couldn’t read the devices. Try again.")).toBeVisible();
  expect(screen.getByRole("list", { name: "Devices" })).toBeVisible();
});

it("shows the iOS sentence while Android devices stay listed", async () => {
  setDevicesApi(createDevicesMock({
    devices: deviceFixtures().filter((one) => one.platform === "android"),
    platforms: [
      { platform: "android", available: true },
      { platform: "ios", available: false, code: "tool_missing", message: "Xcode was not found. Install Xcode, then try again." },
    ],
  }));
  render(<DevicesPanel />);
  const panel = await screen.findByRole("region", { name: "Devices" });
  await waitFor(() => expect(panel).toHaveTextContent("Xcode was not found. Install Xcode, then try again."));
  expect(screen.getByRole("list", { name: "Devices" })).toBeVisible();
});

it("keeps the list and both platform sentences when no platform is available and setup is Planned", async () => {
  setDevicesApi(createDevicesMock({
    devices: [],
    platforms: [
      { platform: "android", available: false, code: "tool_missing", message: "Android tools were not found. Install Android platform-tools and the emulator." },
      { platform: "ios", available: false, code: "tool_missing", message: "Xcode was not found. Install Xcode, then try again." },
    ],
  }));
  render(<DevicesPanel />);
  const panel = await screen.findByRole("region", { name: "Devices" });
  await waitFor(() => expect(panel).toHaveTextContent("Android tools were not found. Install Android platform-tools and the emulator."));
  expect(panel).toHaveTextContent("Xcode was not found. Install Xcode, then try again.");
  expect(panel).toHaveTextContent("No devices are listed.");
  expect(screen.queryByRole("heading", { name: "Choose a platform" })).toBeNull();
});

it("lists every device and opens the chosen one as a tab; nothing is selected until a row is chosen", async () => {
  setDevicesApi(createDevicesMock());
  setDevicesPlannedApi(createDevicesPlannedMock());
  const user = userEvent.setup();
  render(<DevicesPanel />);
  expect(await screen.findByText("Nothing is selected until a row is chosen.")).toBeVisible();
  const list = await screen.findByRole("list", { name: "Devices" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(4);
  await user.click(within(list).getByRole("button", { name: /Pixel 8/ }));
  expect(await screen.findByRole("tab", { name: "Pixel 8" })).toBeVisible();
  expect(await screen.findByRole("toolbar", { name: "Device controls" })).toBeVisible();
});

it("draws a running device with the toolbar the service lists", async () => {
  const device = stateFixture("booted");
  setDevicesApi(createDevicesMock({ devices: [device] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  render(<DevicesPanel />);
  expect(await screen.findByRole("toolbar", { name: "Device controls" })).toBeVisible();
  expect(screen.getAllByText("Running").length).toBeGreaterThan(0);
});

it("replaces a frozen device's screen with Not responding and restarts through the Planned client", async () => {
  const device = stateFixture("frozen");
  const mock = createDevicesMock({ devices: [device] });
  const planned = createDevicesPlannedMock();
  setDevicesApi(mock);
  setDevicesPlannedApi(planned);
  prefs([device.id]);
  const user = userEvent.setup();
  render(<DevicesPanel />);
  const overlay = await screen.findByRole("alert");
  expect(overlay).toHaveTextContent("Not responding");
  await user.click(within(overlay).getByRole("button", { name: "Restart device" }));
  await waitFor(() => expect(planned.restarts).toEqual([device.id]));
});

it("draws the crashed overlay in red with Start it again", async () => {
  const device = stateFixture("crashed");
  setDevicesApi(createDevicesMock({ devices: [device] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  render(<DevicesPanel />);
  const overlay = await screen.findByRole("alert");
  expect(overlay).toHaveTextContent("The simulator stopped");
  expect(within(overlay).getByRole("button", { name: "Start it again" })).toBeVisible();
  expect(overlay).toHaveAttribute("data-tone", "danger");
});

it("gives a shut-down device no controls and says where to start it", async () => {
  const device = stateFixture("shutdown");
  setDevicesApi(createDevicesMock({ devices: [device] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  render(<DevicesPanel />);
  expect(await screen.findByText("Start this device in its own tools to use the controls.")).toBeVisible();
  expect(screen.queryByRole("toolbar", { name: "Device controls" })).toBeNull();
});

it("shows the orientation the detail read reports when the list did not carry it", async () => {
  const { orientation: _unchanged, ...listed } = deviceFixtures()[1]!;
  const mock = createDevicesMock({ devices: [listed] });
  const detail = mock.detail.bind(mock);
  mock.detail = async (id, signal) => ({ ...(await detail(id, signal)), orientation: "landscape_left" });
  setDevicesApi(mock);
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([listed.id]);
  render(<DevicesPanel />);
  expect(await screen.findByLabelText("Orientation")).toHaveTextContent("Landscape left");
});

it("says it is reading a device's actions and draws no toolbar while the read is open", async () => {
  const device = deviceFixtures()[1]!;
  const mock = createDevicesMock({ devices: [device] });
  mock.detail = () => new Promise(() => {});
  setDevicesApi(mock);
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  render(<DevicesPanel />);
  expect(await screen.findByText("Reading what this device can do…")).toBeVisible();
  expect(screen.queryByRole("toolbar", { name: "Device controls" })).toBeNull();
});

it("says so and draws no toolbar when reading a device's actions fails", async () => {
  const device = deviceFixtures()[1]!;
  const mock = createDevicesMock({ devices: [device] });
  mock.failNextDetail("tool_failed");
  setDevicesApi(mock);
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  render(<DevicesPanel />);
  expect(await screen.findByText("Could not read what this device can do.")).toBeVisible();
  expect(screen.queryByText("The device tool could not do that. Try again.")).toBeNull();
  expect(screen.queryByRole("toolbar", { name: "Device controls" })).toBeNull();
});

it("uses the app's own sentence for every failed actions read, whatever the service code", async () => {
  const device = deviceFixtures()[1]!;
  const mock = createDevicesMock({ devices: [device] });
  mock.failNextDetail("tool_timeout");
  setDevicesApi(mock);
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  render(<DevicesPanel />);
  expect(await screen.findByText("Could not read what this device can do.")).toBeVisible();
  expect(screen.queryByText("The device did not answer in time.")).toBeNull();
  expect(screen.queryByRole("toolbar", { name: "Device controls" })).toBeNull();
});

it("removes a device the detail read no longer knows and reads the list again", async () => {
  const [device] = deviceFixtures();
  const mock = createDevicesMock({ devices: [device!, deviceFixtures()[1]!] });
  mock.failNextDetail("device_not_found");
  setDevicesApi(mock);
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device!.id]);
  render(<DevicesPanel />);
  expect(await screen.findByText("That device is no longer there.")).toBeVisible();
  expect(screen.queryByText("That device is gone. The list is up to date.")).toBeNull();
  await waitFor(() => expect(screen.queryByRole("tab", { name: device!.name })).toBeNull());
  const list = await screen.findByRole("list", { name: "Devices" });
  expect(within(list).queryByRole("button", { name: new RegExp(device!.name) })).toBeNull();
});

it("opens two devices as tabs and switches between them with +", async () => {
  setDevicesApi(createDevicesMock());
  setDevicesPlannedApi(createDevicesPlannedMock());
  const user = userEvent.setup();
  render(<DevicesPanel />);
  const list = await screen.findByRole("list", { name: "Devices" });
  await user.click(within(list).getByRole("button", { name: /Pixel 8/ }));
  await user.click(await screen.findByRole("button", { name: "Open another device" }));
  await user.click(await screen.findByRole("menuitem", { name: /iPhone 17 Pro/ }));
  expect(await screen.findByRole("tab", { name: "iPhone 17 Pro" })).toBeVisible();
  expect(screen.getByRole("tab", { name: "Pixel 8" })).toBeVisible();
  await user.click(screen.getByRole("tab", { name: "Pixel 8" }));
  expect(screen.getByRole("tab", { name: "Pixel 8" })).toHaveAttribute("aria-selected", "true");
});

it("renames a tab inline, saves on Return and goes back to the device name when empty", async () => {
  const device = deviceFixtures()[1]!;
  setDevicesApi(createDevicesMock({ devices: [device] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  const user = userEvent.setup();
  render(<DevicesPanel />);
  const tab = await screen.findByRole("tab", { name: "Pixel 8" });
  await user.dblClick(tab);
  const input = screen.getByRole("textbox", { name: "Rename Pixel 8" });
  await user.clear(input);
  await user.type(input, "Checkout flow");
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("tab", { name: "Checkout flow" })).toBeVisible();
  expect(useUi.getState().devices.names[device.id]).toBe("Checkout flow");
  await user.dblClick(screen.getByRole("tab", { name: "Checkout flow" }));
  await user.clear(screen.getByRole("textbox", { name: "Rename Checkout flow" }));
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("tab", { name: "Pixel 8" })).toBeVisible();
  expect(useUi.getState().devices.names[device.id]).toBeUndefined();
});

it("cancels a rename on Escape", async () => {
  const device = deviceFixtures()[1]!;
  setDevicesApi(createDevicesMock({ devices: [device] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  const user = userEvent.setup();
  render(<DevicesPanel />);
  await user.dblClick(await screen.findByRole("tab", { name: "Pixel 8" }));
  const input = screen.getByRole("textbox", { name: "Rename Pixel 8" });
  await user.clear(input);
  await user.type(input, "Not this");
  await user.keyboard("{Escape}");
  expect(await screen.findByRole("tab", { name: "Pixel 8" })).toBeVisible();
  expect(useUi.getState().devices.names[device.id]).toBeUndefined();
});

it("floats the active device over the chat and docks it back in the panel", async () => {
  const device = deviceFixtures()[1]!;
  setDevicesApi(createDevicesMock({ devices: [device] }));
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs([device.id]);
  const user = userEvent.setup();
  const panel = render(<DevicesPanel />);
  await user.click(await screen.findByRole("button", { name: "Device tab menu" }));
  await user.click(await screen.findByRole("menuitem", { name: "Float over chat" }));
  expect(useUi.getState().devices.float).toMatchObject({ id: device.id, width: 360 });
  expect(useUi.getState().panelOpen).toBe(false);
  panel.unmount();
  render(<DevicesFloat threadId="thread-1" />);
  const float = await screen.findByRole("complementary", { name: "Floating device" });
  expect(await within(float).findByText("Pixel 8")).toBeVisible();
  await user.click(within(float).getByRole("button", { name: "Dock" }));
  expect(useUi.getState().devices.float).toBeNull();
  expect(useUi.getState().panelOpen).toBe(true);
  expect(useUi.getState().panelTab).toBe("devices");
});

it("closes a tab without shutting the device down and keeps the other tab open", async () => {
  const devices = deviceFixtures().slice(0, 2);
  const mock = createDevicesMock({ devices });
  setDevicesApi(mock);
  setDevicesPlannedApi(createDevicesPlannedMock());
  prefs(devices.map((one) => one.id));
  const user = userEvent.setup();
  render(<DevicesPanel />);
  await user.click(await screen.findByRole("button", { name: "Close Pixel Fold tab" }));
  expect(screen.queryByRole("tab", { name: "Pixel Fold" })).toBeNull();
  expect(screen.getByRole("tab", { name: "Pixel 8" })).toBeVisible();
  expect(mock.writes).toEqual([]);
});
