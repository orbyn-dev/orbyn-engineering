import { render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { setDevicesApi } from "./api";
import { DevicesLivePreview } from "./DevicesLivePreview";
import { setDevicesPlannedApi } from "./planned";

afterEach(() => { setDevicesApi(null); setDevicesPlannedApi(null); });

it("draws the touch frame with its screen sketch, bar and Planned pieces", async () => {
  render(<DevicesLivePreview frame="device-input-touch" />);
  expect(await screen.findByRole("img", { name: "Live screen of iPhone 17 Pro" })).toBeVisible();
  expect(screen.getByText("Running")).toBeVisible();
  expect(screen.getByText("Live screen · Planned")).toBeVisible();
  expect(screen.getByText("Record · Planned")).toBeVisible();
  expect(screen.getByText("Type text · Planned")).toBeVisible();
  expect(screen.getByText("Device tools · Planned")).toBeVisible();
});

it("draws the typing frame with the field under the device and its Send action", async () => {
  render(<DevicesLivePreview frame="device-input-text" />);
  expect(await screen.findByRole("img", { name: "Live screen of iPhone 16" })).toBeVisible();
  expect(await screen.findByRole("textbox", { name: "Text to type" })).toHaveValue("user@example.com");
  expect(screen.getByRole("button", { name: "Send" })).toBeVisible();
});

it("draws the Android frame with its list, orientation and built toolbar", async () => {
  render(<DevicesLivePreview frame="device-input-android" />);
  const list = await screen.findByRole("list", { name: "Devices" });
  expect(within(list).getByText("Android 15")).toBeVisible();
  expect(within(list).getByText("iOS 26.0")).toBeVisible();
  expect(await screen.findByText("No screenshot yet. The live screen is Planned.")).toBeVisible();
  expect(screen.getByLabelText("Orientation")).toHaveTextContent("Portrait");
  const bar = await screen.findByRole("toolbar", { name: "Device controls" });
  for (const name of ["Back", "Home", "Recents", "Rotate", "Screenshot", "Power off"]) expect(within(bar).getByRole("button", { name })).toBeEnabled();
});

it("draws the agent driving frame with its cursor, chip and Take over", async () => {
  render(<DevicesLivePreview frame="device-agent-driving" />);
  expect(await screen.findByText("Claude Code is using this device")).toBeVisible();
  expect(screen.getByRole("button", { name: "Take over" })).toBeVisible();
  expect(screen.getByText("Tapping Email")).toBeVisible();
  expect(within(await screen.findByRole("toolbar", { name: "Device controls" })).getByRole("button", { name: "Screenshot" })).toBeEnabled();
});

it("draws the paused frame with the thread waiting and Hand back", async () => {
  render(<DevicesLivePreview frame="device-agent-paused" />);
  expect(await screen.findByText("Paused. You are in control.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Hand back" })).toBeVisible();
  expect(screen.getByText("Waiting for you")).toBeVisible();
  expect(screen.getByText("Tapped Email")).toBeVisible();
});

it("draws the agent device-tool rows and the screenshot in the thread", async () => {
  render(<DevicesLivePreview frame="device-agent-tools" />);
  expect(await screen.findByText("device_list")).toBeVisible();
  expect(screen.getByText("2 devices · iPhone 16 running, Pixel 8 shut down")).toBeVisible();
  expect(screen.getByText("device_open")).toBeVisible();
  expect(screen.getByText("iPhone 16 · iOS 18.2 · 3.1 s")).toBeVisible();
  expect(screen.getByText("device_screenshot")).toBeVisible();
  expect(screen.getByRole("button", { name: "Open" })).toBeVisible();
  expect(screen.getByText("device_close")).toBeVisible();
  expect(screen.getByText("Released the device. It keeps running.")).toBeVisible();
  expect(screen.getByText("The Sign in button sits below the fold on this screen size.")).toBeVisible();
});
