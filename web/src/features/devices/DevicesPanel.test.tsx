import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createDevicesMock, deviceFixtures } from "../../../mock/devices";
import { setViewportWidth } from "../../test-support";
import { DevicesPanel } from "./DevicesPanel";
import { setDevicesApi } from "./api";

afterEach(() => { setDevicesApi(null); vi.unstubAllGlobals(); });
const FOLD = "android:Pixel_Fold_API_35";

async function open(name: RegExp | string) {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name }));
  return user;
}
const toolbar = () => screen.findByRole("toolbar", { name: "Device controls" });

it("draws nothing, and no error, when the devices route answers 404", async () => {
  const fetching = vi.fn(async (_input: RequestInfo | URL) => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const { container } = render(<DevicesPanel />);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  expect(String(fetching.mock.calls[0]?.[0])).toMatch(/\/api\/devices/);
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("lists each device with its name, platform, version and state, and tags only the foldable ones", async () => {
  setDevicesApi(createDevicesMock());
  render(<DevicesPanel />);
  const list = await screen.findByRole("list", { name: "Devices" });
  const rows = within(list).getAllByRole("listitem");
  expect(rows).toHaveLength(4);
  expect(within(rows[0]!).getByText("Pixel Fold")).toBeVisible();
  expect(within(rows[0]!).getByText("Android 15")).toBeVisible();
  expect(within(rows[0]!).getByText("Running")).toBeVisible();
  expect(within(list).getAllByText("Foldable")).toHaveLength(2);
  expect(within(rows[2]!).getByText("Shut down")).toBeVisible();
  expect(within(rows[3]!).getByText("iOS 26.1")).toBeVisible();
});

it("shows the platform message when no device is listed and a platform is unavailable", async () => {
  setDevicesApi(createDevicesMock({ devices: [], platforms: [{ platform: "android", available: false, code: "tool_missing", message: "Install the Android platform tools." }, { platform: "ios", available: true }] }));
  render(<DevicesPanel />);
  expect(await screen.findByText("Install the Android platform tools.")).toBeVisible();
  expect(screen.getByText("No devices are listed.")).toBeVisible();
});

it("offers only screenshot and power off on iOS, marks Rotate and Home Planned, and has no fold or hinge control", async () => {
  setDevicesApi(createDevicesMock());
  render(<DevicesPanel />);
  await open(/iPhone 17 Pro/);
  const bar = await toolbar();
  expect(within(bar).getByRole("button", { name: "Power off" })).toBeEnabled();
  expect(within(bar).getByRole("button", { name: "Screenshot" })).toBeEnabled();
  expect(within(bar).getByRole("button", { name: "Rotate (Planned)" })).toBeDisabled();
  expect(within(bar).getByRole("button", { name: "Home (Planned)" })).toBeDisabled();
  for (const name of ["Back", "Recents", "Fold", "Unfold", "Hinge angle"]) expect(screen.queryByRole("button", { name })).toBeNull();
});

it("offers Back, Home, Recents and Rotate on Android, and no fold control on a device that does not fold", async () => {
  setDevicesApi(createDevicesMock());
  render(<DevicesPanel />);
  await open(/Pixel 8/);
  const bar = await toolbar();
  for (const name of ["Back", "Home", "Recents", "Rotate", "Power off", "Screenshot"]) expect(within(bar).getByRole("button", { name })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Fold" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Hinge angle" })).toBeNull();
});

it("shows the orientation the service reports and rotates to the next one in the contract's order", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  const bar = await toolbar();
  expect(await screen.findByLabelText("Orientation")).toHaveTextContent("Portrait");
  const steps = [
    ["landscape_left", "Landscape left"],
    ["portrait_upside_down", "Portrait upside down"],
    ["landscape_right", "Landscape right"],
    ["portrait", "Portrait"],
  ] as const;
  for (const [value, label] of steps) {
    await user.click(within(bar).getByRole("button", { name: "Rotate" }));
    await waitFor(() => expect(mock.writes.at(-1)).toBe(`android:Pixel_8_API_35:rotate:${value}`));
    await waitFor(() => expect(screen.getByLabelText("Orientation")).toHaveTextContent(label));
    await waitFor(() => expect(within(bar).getByRole("button", { name: "Rotate" })).toBeEnabled());
  }
});

it("sends the first next orientation when none is reported, and shows none it did not receive", async () => {
  const { orientation: _orientation, ...withoutOrientation } = deviceFixtures()[1]!;
  const mock = createDevicesMock({ devices: [withoutOrientation] });
  const act = mock.act.bind(mock);
  mock.act = async (id, action) => {
    const result = await act(id, action);
    const { orientation: _answer, ...device } = result.device;
    return { ...result, device };
  };
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  expect(await toolbar()).toBeVisible();
  expect(screen.queryByLabelText("Orientation")).toBeNull();
  const rotate = screen.getByRole("button", { name: "Rotate" });
  await user.click(rotate);
  await waitFor(() => expect(mock.writes).toEqual(["android:Pixel_8_API_35:rotate:landscape_left"]));
  expect(screen.queryByLabelText("Orientation")).toBeNull();
  await waitFor(() => expect(screen.getByRole("button", { name: "Rotate" })).toBeEnabled());
  await user.click(rotate);
  await waitFor(() => expect(mock.writes).toEqual(["android:Pixel_8_API_35:rotate:landscape_left", "android:Pixel_8_API_35:rotate:landscape_left"]));
  expect(screen.queryByLabelText("Orientation")).toBeNull();
});

it("offers nothing to do on a device that is shut down", async () => {
  setDevicesApi(createDevicesMock());
  render(<DevicesPanel />);
  await open(/Pixel 9 Pro Fold/);
  expect(await screen.findByText("Start this device in its own tools to use the controls.")).toBeVisible();
  expect(screen.queryByRole("toolbar", { name: "Device controls" })).toBeNull();
});

it("sends one write per click, and a second click while one runs sends nothing", async () => {
  const mock = createDevicesMock({ delayMs: 100 });
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  const bar = await toolbar();
  const home = within(bar).getByRole("button", { name: "Home" });
  await user.click(home);
  await user.click(home);
  await waitFor(() => expect(within(bar).getByRole("button", { name: "Home" })).toBeEnabled());
  expect(mock.writes).toEqual([`${FOLD}:button:home`]);
});

it("disables every control while a write runs", async () => {
  setDevicesApi(createDevicesMock({ delayMs: 40 }));
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  const bar = await toolbar();
  await user.click(within(bar).getByRole("button", { name: "Fold" }));
  for (const button of within(bar).getAllByRole("button")) expect(button).toBeDisabled();
  await waitFor(() => expect(within(bar).getByRole("button", { name: "Unfold" })).toBeEnabled());
});

it("folds and unfolds with one write each and shows the posture the service returned", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  expect(await screen.findByText("Open · 180°")).toBeVisible();
  await user.click(await screen.findByRole("button", { name: "Fold" }));
  expect(await screen.findByText("Closed · 0°")).toBeVisible();
  await user.click(await screen.findByRole("button", { name: "Unfold" }));
  expect(await screen.findByText("Open · 180°")).toBeVisible();
  expect(mock.writes).toEqual([`${FOLD}:fold`, `${FOLD}:unfold`]);
});

it("renders whatever posture the service answers, not the one it asked for", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  mock.answerPosture({ posture: "tent", hinge_angle: 61 });
  await user.click(await screen.findByRole("button", { name: "Fold" }));
  expect(await screen.findByText("Tent · 61°")).toBeVisible();
  mock.answerPosture({ posture: null, hinge_angle: null });
  await user.click(await screen.findByRole("button", { name: "Fold" }));
  expect(await screen.findByText("Not reported")).toBeVisible();
});

it("sets the hinge from a slider bounded 1 to 179 and sends one write on Set angle", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  await user.click(await screen.findByRole("button", { name: "Hinge angle" }));
  const dialog = await screen.findByRole("dialog", { name: "Hinge angle" });
  const slider = within(dialog).getByRole("slider", { name: "Angle in degrees" });
  expect(slider).toHaveAttribute("min", "1");
  expect(slider).toHaveAttribute("max", "179");
  fireEvent.change(slider, { target: { value: "100" } });
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Set angle" }));
  await waitFor(() => expect(mock.writes).toEqual([`${FOLD}:hinge:100`]));
  expect(await screen.findByText("Half open · 100°")).toBeVisible();
});

it("sends a named preset, never a raw number, and closes the popover with the answer", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  for (const [name, preset] of [["Closed", "closed"], ["Half open", "half_open"], ["Open", "open"]] as const) {
    await user.click(await screen.findByRole("button", { name: "Hinge angle" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Hinge angle" })).getByRole("button", { name }));
    await waitFor(() => expect(mock.writes.at(-1)).toBe(`${FOLD}:hinge_preset:${preset}`));
    await waitFor(() => expect(screen.getByRole("button", { name: "Hinge angle" })).toBeEnabled());
  }
});

it("shows a screenshot as an image and takes only a PNG", async () => {
  setDevicesApi(createDevicesMock());
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  await user.click(await screen.findByRole("button", { name: "Screenshot" }));
  const image = await screen.findByRole("img", { name: "Screenshot of Pixel 8" });
  expect(image.getAttribute("src")).toMatch(/^data:image\/png;base64,iVBOR/);
});

it("says so when the screenshot is not a PNG, and shows no image", async () => {
  const mock = createDevicesMock();
  const act = mock.act.bind(mock);
  mock.act = async (id, action) => ({ ...(await act(id, action)), image: "PHN2ZyBvbmxvYWQ9eD4=" });
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  await user.click(await screen.findByRole("button", { name: "Screenshot" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("not a usable screenshot");
  expect(screen.queryByRole("img", { name: /Screenshot of/ })).toBeNull();
});

it("shows the service's message on a failure, and Try again repeats the same write once", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  mock.failNext("tool_failed");
  await user.click(await screen.findByRole("button", { name: "Fold" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Couldn’t fold Pixel Fold. The device tool could not do that. Try again.");
  await user.click(within(alert).getByRole("button", { name: "Try again" }));
  expect(await screen.findByText("Closed · 0°")).toBeVisible();
  expect(mock.writes).toEqual([`${FOLD}:fold`, `${FOLD}:fold`]);
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each([
  ["invalid_action", "That action is not valid."],
  ["device_busy", "Another action is running on this device."],
  ["device_not_booted", "Start the device first."],
  ["not_foldable", "This device does not fold."],
  ["action_unsupported", "This device cannot do that."],
  ["tool_failed", "The device tool could not do that. Try again."],
  ["tool_timeout", "The device did not answer in time."],
  ["tool_output_too_large", "The device sent more than Orbyn can read."],
])("shows the service's own sentence for a failed action (%s)", async (code, sentence) => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel Fold/);
  mock.failNext(code);
  await user.click(await screen.findByRole("button", { name: "Fold" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent(`Couldn’t fold Pixel Fold. ${sentence}`);
  expect(screen.getByRole("toolbar", { name: "Device controls" })).toBeVisible();
});

it("shows the platform's own sentence when an action on its missing tools fails", async () => {
  const message = "Android tools were not found. Install Android platform-tools and the emulator.";
  const mock = createDevicesMock({
    devices: [deviceFixtures().find((one) => one.id === "android:Pixel_8_API_35")!],
    platforms: [{ platform: "android", available: false, code: "tool_missing", message }, { platform: "ios", available: true }],
  });
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  mock.failNext("tool_missing");
  await user.click(await screen.findByRole("button", { name: "Home" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent(`Couldn’t press home on Pixel 8. ${message}`);
});

it("drops a device the service no longer knows and reads the list again", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  mock.failNext("device_not_found");
  await user.click(await screen.findByRole("button", { name: "Home" }));
  expect(await screen.findByText("That device is no longer there.")).toBeVisible();
  expect(screen.queryByText("That device is gone. The list is up to date.")).toBeNull();
  await waitFor(() => expect(screen.queryByRole("tab", { name: "Pixel 8" })).toBeNull());
  const list = await screen.findByRole("list", { name: "Devices" });
  expect(within(list).queryByRole("button", { name: /Pixel 8/ })).toBeNull();
});

it("powers a device off and then offers nothing to do on it", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  const user = await open(/Pixel 8/);
  await user.click(await screen.findByRole("button", { name: "Power off" }));
  expect(await screen.findByText("Start this device in its own tools to use the controls.")).toBeVisible();
  expect(mock.writes).toEqual(["android:Pixel_8_API_35:power_off"]);
});

it("escapes and cuts a hostile device name and message", async () => {
  const hostile = deviceFixtures().slice(0, 1).map((device) => ({ ...device, name: `<img src=x onerror=alert(1)>${"n".repeat(300)}` }));
  setDevicesApi(createDevicesMock({ devices: hostile, platforms: [{ platform: "ios", available: false, message: `<script>1</script>${"m".repeat(500)}` }] }));
  const { container } = render(<DevicesPanel />);
  await screen.findByRole("list", { name: "Devices" });
  expect(container.querySelector("img[src='x']")).toBeNull();
  expect(container.querySelector("script")).toBeNull();
  expect(container.textContent).not.toContain("n".repeat(120));
  expect(container.textContent).not.toContain("m".repeat(260));
});

it("never types a device id: a thread-chosen id outside the list cannot be reached", async () => {
  const mock = createDevicesMock();
  setDevicesApi(mock);
  render(<DevicesPanel />);
  await screen.findByRole("list", { name: "Devices" });
  expect(screen.queryByRole("textbox")).toBeNull();
});

it("keeps the controls inside a phone-width panel", async () => {
  setViewportWidth(320);
  setDevicesApi(createDevicesMock());
  render(<DevicesPanel />);
  await open(/Pixel Fold/);
  expect(await toolbar()).toBeVisible();
});

it("guards against two clicks in the same tick: one write, even before the panel redraws", async () => {
  const mock = createDevicesMock({ delayMs: 20 });
  setDevicesApi(mock);
  render(<DevicesPanel />);
  await open(/Pixel 8/);
  const home = await screen.findByRole("button", { name: "Home" });
  act(() => { fireEvent.click(home); fireEvent.click(home); });
  await waitFor(() => expect(screen.getByRole("button", { name: "Home" })).toBeEnabled());
  expect(mock.writes).toEqual(["android:Pixel_8_API_35:button:home"]);
});
