import { render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DevicesToolsPreview } from "./DevicesToolsPreview";
import { setDevicesApi } from "./api";
import { setDevicesPlannedApi } from "./planned";

afterEach(() => {
  setDevicesApi(null);
  setDevicesPlannedApi(null);
  vi.unstubAllGlobals();
});

it("draws the appearance drawer over the built device panel", async () => {
  render(<DevicesToolsPreview frame="device-tools-appearance" />);
  const drawer = await screen.findByRole("region", { name: "Device tools" });
  expect(within(drawer).getByRole("tab", { name: "Appearance" })).toHaveAttribute("aria-selected", "true");
  expect(await within(drawer).findByText("The device’s own setting")).toBeVisible();
  expect(screen.getAllByText("iPhone 16").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Running").length).toBeGreaterThan(0);
});

it("draws the push frame on Android with Send disabled", async () => {
  render(<DevicesToolsPreview frame="device-tools-push-android" />);
  const drawer = await screen.findByRole("region", { name: "Device tools" });
  expect(await within(drawer).findByText("Not available on Android")).toBeVisible();
  expect(within(drawer).getByRole("button", { name: "Send" })).toBeDisabled();
  expect(screen.getAllByText("Pixel 8").length).toBeGreaterThan(0);
});

it("draws the platform table as its own panel", async () => {
  render(<DevicesToolsPreview frame="device-tools-platforms" />);
  expect(await screen.findByRole("heading", { name: "Device tools by platform" })).toBeVisible();
  expect(screen.getByText("Hardware keys")).toBeVisible();
  expect(screen.queryByRole("region", { name: "Device tools" })).toBeNull();
});

it("draws the waiting permission card with the thread header saying Waiting for you", async () => {
  render(<DevicesToolsPreview frame="device-agent-permission" />);
  expect(await screen.findByText("Waiting for you")).toBeVisible();
  expect(await screen.findByText("Claude Code")).toBeVisible();
  expect(screen.getByText(/use the/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Deny" })).toBeVisible();
  expect(screen.getByPlaceholderText("Type a reply to decline with feedback")).toBeVisible();
});

it("draws the lead's answer, the granted chip and the revoked chip state", async () => {
  const lead = render(<DevicesToolsPreview frame="device-agent-permission-lead" />);
  expect(await screen.findByText(/Lead allowed/)).toBeVisible();
  expect(screen.getByText("Autopilot Full")).toBeVisible();
  expect(screen.getByText("Agent access is on")).toBeVisible();
  lead.unmount();

  render(<DevicesToolsPreview frame="device-agent-permission-granted" />);
  const chip = await screen.findByRole("button", { name: "Device" });
  expect(chip).toHaveAttribute("title", "Claude Code · this thread");
  expect(screen.queryByText("Waiting for you")).toBeNull();
});

it("draws the revoked state without the chip and with the agent's plain message", async () => {
  render(<DevicesToolsPreview frame="device-agent-permission-revoked" />);
  expect(await screen.findByText("Access to devices was turned off")).toBeVisible();
  expect(screen.getByText("I lost access to the device, so I cannot take a screenshot. Turn access on in the Device tab if you want me to continue.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Device" })).toBeNull();
});

it("draws agent access off with the failed call and no ask", async () => {
  render(<DevicesToolsPreview frame="device-agent-off" />);
  expect(await screen.findByText("Agent access is off")).toBeVisible();
  expect(screen.getByText("Devices are off for agents, so I cannot open the simulator. Turn on Agent access in the Device tab, or open the device yourself and I will read screenshots you attach.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Allow once" })).toBeNull();
  expect(screen.queryByText("Waiting for you")).toBeNull();
});

it("answers a waiting request only when the person chooses", async () => {
  render(<DevicesToolsPreview frame="device-agent-permission" />);
  const button = await screen.findByRole("button", { name: "Allow once" });
  button.click();
  await waitFor(() => expect(screen.queryByText("Waiting for you")).toBeNull());
  expect(screen.getByRole("button", { name: "Device" })).toBeVisible();
});
