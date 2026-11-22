import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createDevicesPlannedMock } from "../../../mock/devices";
import type { PlatformStatus } from "./contract";
import { setDevicesPlannedApi } from "./planned";
import { SetupHub } from "./setup/SetupHub";
import { ToolVersions } from "./setup/ToolVersions";

afterEach(() => { setDevicesPlannedApi(null); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const available: PlatformStatus[] = [{ platform: "android", available: true }, { platform: "ios", available: true }];
const page = (step: 1 | 2 | 3 | "done" = 1, platform: "android" | "ios" = "android") => (
  <SetupHub platforms={available} deviceToOpen={{ id: "ios:one", name: "iPhone 17 Pro", platform: "ios", state: "booted", version: "26.1", foldable: false }} initialStep={step} initialPlatform={platform} onOpen={() => {}} onClose={() => {}} />
);

it("starts on the three-step hub and opens the tools step", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock());
  const user = userEvent.setup();
  render(page());
  expect(screen.getByRole("list", { name: "Device setup steps" })).toBeVisible();
  expect(screen.getByRole("heading", { name: "Choose a platform" })).toBeVisible();
  expect(screen.getByRole("radio", { name: /Android Emulator/ })).toHaveAttribute("aria-checked", "true");
  await user.click(screen.getByRole("button", { name: "Continue" }));
  expect(await screen.findByRole("heading", { name: "Tools for the Android Emulator" })).toBeVisible();
  expect(await screen.findByRole("list", { name: "Tools" })).toBeVisible();
});

it("dims the iOS platform with the reason on Windows and preselects Android", async () => {
  vi.spyOn(window.navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(page());
  const ios = screen.getByRole("radio", { name: /iOS Simulator/ });
  expect(ios).toBeDisabled();
  expect(ios).toHaveTextContent("Needs a Mac with Xcode. This computer runs Windows.");
  expect(screen.getByRole("radio", { name: /Android Emulator/ })).toHaveAttribute("aria-checked", "true");
});

it("shows the vendor's steps and a required tool blocks Continue", async () => {
  const planned = createDevicesPlannedMock();
  setDevicesPlannedApi(planned);
  const user = userEvent.setup();
  render(page(2, "android"));
  expect(await screen.findByText("Platform tools (adb)")).toBeVisible();
  expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  await user.click(screen.getAllByRole("button", { name: "Show steps" })[0]!);
  expect(await screen.findByText("Steps from the Android developers")).toBeVisible();
  expect(screen.getByText(/sdkmanager "platform-tools"/)).toBeVisible();
  expect(screen.getByRole("link", { name: /developer.android.com/ })).toHaveAttribute("href", "https://developer.android.com/studio");
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(planned.checks).toBe(1));
});

it("shows the service's platform sentence when a required tool is missing", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock({ iosMissing: true }));
  render(
    <SetupHub
      platforms={[{ platform: "ios", available: false, code: "tool_missing", message: "Xcode was not found. Install Xcode, then try again." }]}
      deviceToOpen={null}
      initialStep={2}
      initialPlatform="ios"
      onOpen={() => {}}
      onClose={() => {}}
    />,
  );
  expect(await screen.findByText("Xcode was not found. Install Xcode, then try again.")).toBeVisible();
  expect(await screen.findByText("Xcode command line tools")).toBeVisible();
  expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
});

it("installs agent-device, fails in plain words, and Try again keeps the optional install out of Continue's way", async () => {
  const planned = createDevicesPlannedMock();
  planned.failInstall(true);
  setDevicesPlannedApi(planned);
  const user = userEvent.setup();
  render(page(2, "android"));
  await screen.findByText("agent-device");
  await user.click(screen.getByRole("button", { name: "Install" }));
  expect(await screen.findByText("Could not install agent-device")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("Connection reset (ECONNRESET)");
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(planned.installs).toBe(2));
  expect(await screen.findByText("agent-device installed")).toBeVisible();
  expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
});

it("turns agent access on through the Planned toggle and finishes with the device summary", async () => {
  const planned = createDevicesPlannedMock();
  setDevicesPlannedApi(planned);
  const user = userEvent.setup();
  render(page(3, "android"));
  const toggle = await screen.findByRole("switch", { name: "Let agents use devices" });
  expect(toggle).toHaveAttribute("aria-checked", "false");
  await user.click(toggle);
  await waitFor(() => expect(planned.accessWrites).toEqual([true]));
  expect(toggle).toHaveAttribute("aria-checked", "true");
  await user.click(screen.getByRole("button", { name: "Finish" }));
  expect(await screen.findByRole("heading", { name: "Devices are ready" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Open iPhone 17 Pro" })).toBeVisible();
});

it("says the setup is Planned and keeps Continue shut when the production client answers nothing", async () => {
  render(page(2, "android"));
  expect(await screen.findByText(/Tool checks and setup are not available on this service yet/)).toBeVisible();
  expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
});

it("checks tool versions, shows Not checked offline, and opens the vendor's steps", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock({ offline: true }));
  const user = userEvent.setup();
  render(<ToolVersions onBack={() => {}} initialOpenSteps={["ios-runtime"]} />);
  expect(await screen.findByText("Could not check for updates. You are offline, so these are the versions found on this computer.")).toBeVisible();
  expect(screen.getAllByText("Not checked").length).toBeGreaterThan(0);
  expect(await screen.findByText("How to update the iOS Simulator runtime (example)")).toBeVisible();
  expect(screen.getByRole("link", { name: /developer.apple.com/ })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Check again" }));
  expect(await screen.findByText("Checked 14:02")).toBeVisible();
});

it("keeps the version labels while a check runs", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock({ delayMs: 40 }));
  render(<ToolVersions onBack={() => {}} />);
  expect(screen.getAllByText("Checking…").length).toBeGreaterThan(0);
  expect(await screen.findByRole("list", { name: "Tool versions" })).toBeVisible();
});

it("says tool versions are Planned without drawing rows in production", async () => {
  render(<ToolVersions onBack={() => {}} />);
  expect(await screen.findByText(/Tool versions are not available on this service yet/)).toBeVisible();
  expect(screen.queryByRole("list", { name: "Tool versions" })).toBeNull();
  expect(screen.getByRole("button", { name: "Check now" })).toBeDisabled();
});

it("keeps a hostile tool name as text", async () => {
  const planned = createDevicesPlannedMock();
  planned.setTools("android", [{ id: "x", name: "<img src=x onerror=alert(1)>", required: true, state: "missing", version: null, latest: null, steps: { title: "<script>1</script>", text: "x" } }]);
  setDevicesPlannedApi(planned);
  const { container } = render(page(2, "android"));
  expect(await screen.findByText("<img src=x onerror=alert(1)>")).toBeVisible();
  expect(container.querySelector("img, script")).toBeNull();
});

it("has a Copy button for a vendor command whose action does not run an install", async () => {
  const planned = createDevicesPlannedMock();
  setDevicesPlannedApi(planned);
  const user = userEvent.setup();
  render(page(2, "android"));
  await screen.findByText("Platform tools (adb)");
  await user.click(screen.getAllByRole("button", { name: "Show steps" })[0]!);
  expect(screen.getByRole("button", { name: "Copy command" })).toBeVisible();
  expect(planned.installs).toBe(0);
});

it("keeps the tool rows readable inside a narrow panel", async () => {
  setDevicesPlannedApi(createDevicesPlannedMock());
  render(<div style={{ width: 320 }}>{page(2, "android")}</div>);
  const list = await screen.findByRole("list", { name: "Tools" });
  expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0);
});
