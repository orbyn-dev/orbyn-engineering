import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createDevicesMock, createDevicesPlannedMock, stateFixture } from "../../../../mock/devices";
import { useUi } from "../../../product/state/ui";
import { DevicesPanel } from "../DevicesPanel";
import { setDevicesApi } from "../api";
import { setDevicesPlannedApi } from "../planned";
import { DeviceToolsDrawer, DeviceToolsPlatforms } from "./DeviceTools";

afterEach(() => {
  setDevicesApi(null);
  setDevicesPlannedApi(null);
  vi.unstubAllGlobals();
});

const iphone = () => stateFixture("booted");
const pixel = () => stateFixture("booted", "android");

function openPanel(device = iphone()) {
  useUi.setState({ devices: { open: [device.id], active: device.id, names: {}, float: null } });
  setDevicesApi(createDevicesMock({ devices: [device] }));
  render(<DevicesPanel />);
  return device;
}

it("opens the tools drawer from the Device tools pill", async () => {
  const device = openPanel();
  const user = userEvent.setup();
  expect(await screen.findByRole("toolbar", { name: "Device controls" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: /Device tools/ }));
  const drawer = await screen.findByRole("region", { name: "Device tools" });
  expect(drawer).toBeVisible();
  expect(within(drawer).getByRole("tab", { name: "Appearance" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("button", { name: /Device tools/ })).toHaveAttribute("aria-expanded", "true");
  await user.click(within(drawer).getByRole("button", { name: "Close tools" }));
  expect(screen.queryByRole("region", { name: "Device tools" })).toBeNull();
  expect(device.state).toBe("booted");
});

it("shows the Planned note, and no controls, when the service has no tools route", async () => {
  const device = openPanel();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /Device tools/ }));
  const drawer = await screen.findByRole("region", { name: "Device tools" });
  expect(within(drawer).getByText(/Device tools are Planned/)).toBeVisible();
  expect(within(drawer).getByText(/The service has no tools route yet/)).toBeVisible();
  expect(within(drawer).queryByText("Not seen on a real device yet")).toBeNull();
  expect(within(drawer).queryByRole("switch")).toBeNull();
  expect(device.state).toBe("booted");
});

it("draws Appearance from the Planned client and sends one typed change per choice", async () => {
  const mock = createDevicesPlannedMock();
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} />);
  expect(await screen.findByText("Not seen on a real device yet")).toBeVisible();
  expect(screen.getByText("The device’s own setting")).toBeVisible();
  expect(screen.getByRole("button", { name: "Light" })).toHaveAttribute("aria-pressed", "true");
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Dark" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:appearance:dark"));
  await waitFor(() => expect(screen.getByRole("button", { name: "Dark" })).toHaveAttribute("aria-pressed", "true"));
  await user.click(screen.getByRole("switch", { name: "Increase contrast" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:increase_contrast:true"));
  await user.click(screen.getByRole("switch", { name: "Reduce motion" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:reduce_motion:true"));
});

it("names the current text step and steps it with −, Default and +", async () => {
  const mock = createDevicesPlannedMock();
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="text" />);
  expect(await screen.findByText("Large · step 4 of 12")).toBeVisible();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Larger" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:text_size:5"));
  await waitFor(() => expect(screen.getByText("Larger · step 5 of 12")).toBeVisible());
  await user.click(screen.getByRole("button", { name: "Smaller" }));
  await waitFor(() => expect(screen.getByText("Large · step 4 of 12")).toBeVisible());
  await user.click(screen.getByRole("button", { name: "Default" }));
  await waitFor(() => expect(screen.getByText("Default · step 3 of 12")).toBeVisible());
  await user.click(screen.getByRole("switch", { name: "Bold text" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:bold:true"));
});

it("keeps Show elements disabled with its reason and hides the elements when no screen is live", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="accessibility" />);
  const toggle = await screen.findByRole("switch", { name: "Show elements" });
  expect(toggle).toBeDisabled();
  expect(toggle).toHaveAttribute("title", "Outlines need a live screen.");
  expect(screen.getByText("Outlines need a live screen.")).toBeVisible();
  expect(screen.getByText("No screen is live.")).toBeVisible();
  expect(screen.queryByText("Outline every element the device exposes")).toBeNull();
  expect(screen.queryByText("Sign in · Button")).toBeNull();
  expect(screen.queryByText("Email · Text field")).toBeNull();
});

it("enables Show elements and lists the element rows when a screen is live", async () => {
  const mock = createDevicesPlannedMock({ live: {} });
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="accessibility" live />);
  const toggle = await screen.findByRole("switch", { name: "Show elements" });
  expect(toggle).toBeEnabled();
  expect(toggle).toHaveAttribute("aria-checked", "true");
  expect(toggle).not.toHaveAttribute("title");
  expect(screen.getByText("Outline every element the device exposes")).toBeVisible();
  expect(screen.queryByText("Outlines need a live screen.")).toBeNull();
  expect(screen.queryByText("No screen is live.")).toBeNull();
  expect(screen.getByText("Sign in · Button")).toBeVisible();
  expect(screen.getByText("Label “Sign in”, enabled, 192 × 32")).toBeVisible();
  expect(screen.getByText("Email · Text field")).toBeVisible();
  const row = screen.getByRole("button", { name: /Sign in · Button/ });
  const user = userEvent.setup();
  await user.click(row);
  expect(row).toHaveAttribute("aria-pressed", "true");
  await user.click(toggle);
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:show_elements:false"));
});

it("turns Show elements off once when the live screen goes away", async () => {
  const mock = createDevicesPlannedMock({ live: {} });
  setDevicesPlannedApi(mock);
  const view = render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="accessibility" live />);
  expect(await screen.findByRole("switch", { name: "Show elements" })).toHaveAttribute("aria-checked", "true");
  view.rerender(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="accessibility" live={false} />);
  await waitFor(() => expect(mock.toolWrites).toEqual(["ios:7C2D-0001:show_elements:false"]));
  expect(await screen.findByText("No screen is live.")).toBeVisible();
  expect(screen.getByRole("switch", { name: "Show elements" })).toBeDisabled();
  expect(screen.getByRole("switch", { name: "Show elements" })).toHaveAttribute("aria-checked", "false");
  view.rerender(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="accessibility" live={false} />);
  await waitFor(() => expect(screen.getByText("No screen is live.")).toBeVisible());
  expect(mock.toolWrites).toEqual(["ios:7C2D-0001:show_elements:false"]);
});

it("sets a simulated place and typed coordinates", async () => {
  const mock = createDevicesPlannedMock();
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="location" />);
  expect(await screen.findByText("Used by maps and location APIs on the device")).toBeVisible();
  const place = screen.getByRole("combobox", { name: "Place" });
  expect(place).toHaveValue("Berlin, Germany");
  const coordinates = screen.getByRole("textbox", { name: "Coordinates" });
  expect(coordinates).toHaveValue("52.5200, 13.4050");
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Set" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:location:52.5200, 13.4050"));
  await user.selectOptions(place, "None");
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:location:none"));
});

it("grants, denies and resets each app permission", async () => {
  const mock = createDevicesPlannedMock();
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="permissions" />);
  const camera = await screen.findByRole("group", { name: "Camera" });
  expect(within(camera).getByText("Acme · asked once")).toBeVisible();
  expect(within(camera).getByRole("button", { name: "Ask" })).toHaveAttribute("aria-pressed", "true");
  const notifications = screen.getByRole("group", { name: "Notifications" });
  expect(within(notifications).getByRole("button", { name: "Allow" })).toHaveAttribute("aria-pressed", "true");
  const user = userEvent.setup();
  await user.click(within(camera).getByRole("button", { name: "Allow" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:permission:camera:allow"));
  await user.click(within(notifications).getByRole("button", { name: "Deny" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:permission:notifications:deny"));
  await user.click(within(camera).getByRole("button", { name: "Ask" }));
  await waitFor(() => expect(mock.toolWrites).toContain("ios:7C2D-0001:permission:camera:ask"));
});

it("sends a test push on iOS and keeps the payload per device", async () => {
  const mock = createDevicesPlannedMock();
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="push" />);
  const payload = await screen.findByRole("textbox", { name: "Notification payload" });
  expect(payload).toHaveValue('{ "aps": { "alert": { "title": "Order shipped", "body": "Arrives Friday" } } }');
  const user = userEvent.setup();
  // A person's edit, with the editor read before Send: every keystroke is awaited and the value is asserted in the
  // field, so the write can only carry what the editor shows. The old form poked the textarea with one raw change event
  // and clicked at once, with no read of the value between the two.
  await user.clear(payload);
  await user.paste('{"aps":{"alert":"Hello"}}');
  expect(payload).toHaveValue('{"aps":{"alert":"Hello"}}');
  await user.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(mock.pushWrites).toEqual(["ios:7C2D-0001"]));
  expect(mock.toolState("ios:7C2D-0001")?.push.payload).toBe('{"aps":{"alert":"Hello"}}');
});

it("says push is not available on Android and offers nothing to send", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(<DeviceToolsDrawer device={pixel()} onClose={() => {}} initialTab="push" />);
  expect(await screen.findByText("Not available on Android")).toBeVisible();
  expect(screen.getByText("Sending a test notification to the app")).toBeVisible();
  expect(screen.queryByRole("textbox", { name: "Notification payload" })).toBeNull();
  expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "App" })).toBeDisabled();
});

it("shows the platform's error line under the push editor when a send fails", async () => {
  const mock = createDevicesPlannedMock();
  mock.failNextPush(true);
  setDevicesPlannedApi(mock);
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} initialTab="push" />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Send" }));
  expect(await screen.findByText("The device did not accept the notification.")).toBeVisible();
});

it("tables what each platform can do, including the controls it cannot", async () => {
  render(<DeviceToolsPlatforms />);
  expect(screen.getByRole("heading", { name: "Device tools by platform" })).toBeVisible();
  expect(screen.getByText("Not verified means Orbyn has not yet seen this work on a real device.")).toBeVisible();
  for (const name of ["Appearance", "Text size", "Accessibility", "Location", "Permissions", "Push", "Hardware keys"]) {
    expect(screen.getByText(name)).toBeVisible();
  }
  expect(screen.getByText("Light and Dark, content size")).toBeVisible();
  expect(screen.getByText("Dynamic Type or font scale")).toBeVisible();
  expect(screen.getByText("Outline elements and read the tree")).toBeVisible();
  expect(screen.getByText("A simulated place")).toBeVisible();
  expect(screen.getByText("Grant, deny and reset per app")).toBeVisible();
  expect(screen.getByText("A test notification")).toBeVisible();
  expect(screen.getByText("Back and Recents")).toBeVisible();
  expect(screen.getAllByText("iOS · Not verified")).toHaveLength(6);
  expect(screen.getAllByText("Android · Not verified")).toHaveLength(6);
  expect(screen.getByText("Not available on Android")).toBeVisible();
  expect(screen.getByText("Not available on iOS")).toBeVisible();
});

it("opens the platform table from the drawer", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(<DeviceToolsDrawer device={iphone()} onClose={() => {}} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "What each platform can do" }));
  expect(screen.getByRole("heading", { name: "Device tools by platform" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Back to tools" })).toBeVisible();
});
