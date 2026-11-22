import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createDevicesPlannedMock } from "../../../../mock/devices";
import { setDevicesPlannedApi, type AgentDeviceRequest } from "../planned";
import { AgentAccessSetting, AgentDeviceChip, AgentPermissionCard } from "./AgentDevice";

afterEach(() => { setDevicesPlannedApi(null); vi.unstubAllGlobals(); });

const request: AgentDeviceRequest = { id: "req-1", agent: "Claude Code", device: "iPhone 16", thread: "preview-thread" };

it("keeps agent access off until the person turns it on", async () => {
  const mock = createDevicesPlannedMock();
  setDevicesPlannedApi(mock);
  const user = userEvent.setup();
  render(<AgentAccessSetting />);
  const toggle = await screen.findByRole("switch", { name: "Let agents use devices" });
  expect(toggle).toHaveAttribute("aria-checked", "false");
  expect(mock.accessWrites).toEqual([]);
  expect(screen.getByText(/Agents cannot use devices/i)).toBeVisible();
  await user.click(toggle);
  await waitFor(() => expect(mock.accessWrites).toEqual([true]));
  await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
});

it("never auto-grants: rendering a request does not answer it", () => {
  const onAnswer = vi.fn();
  render(<AgentPermissionCard request={request} onAnswer={onAnswer} />);
  expect(screen.getByText(/Needs you/)).toBeVisible();
  expect(onAnswer).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Allow for this thread" })).toBeVisible();
});

it("offers Deny, Allow once and Allow for this thread, and only a choice answers", async () => {
  const onAnswer = vi.fn();
  const user = userEvent.setup();
  render(<AgentPermissionCard request={request} onAnswer={onAnswer} />);
  expect(screen.getByText("Claude Code")).toBeVisible();
  expect(screen.getByText("iPhone 16")).toBeVisible();
  expect(screen.getByText("It can open the device, tap, type and take screenshots. It cannot install apps without asking again.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Deny" }));
  expect(onAnswer).toHaveBeenLastCalledWith("deny");
  await user.click(screen.getByRole("button", { name: "Allow once" }));
  expect(onAnswer).toHaveBeenLastCalledWith("once");
  await user.click(screen.getByRole("button", { name: "Allow for this thread" }));
  expect(onAnswer).toHaveBeenLastCalledWith("thread");
});

it("shows who holds the device and the scope in the chip's tooltip", async () => {
  const onOpen = vi.fn();
  const user = userEvent.setup();
  render(<AgentDeviceChip grant={{ agent: "Claude Code", device: "iPhone 16", scope: "thread", thread: "preview-thread" }} onOpen={onOpen} />);
  const chip = screen.getByRole("button", { name: "Device" });
  expect(chip).toHaveAttribute("title", "Claude Code · this thread");
  await user.click(chip);
  expect(onOpen).toHaveBeenCalledTimes(1);
});

it("takes no choice while the Planned client answers nothing", async () => {
  const user = userEvent.setup();
  const onAnswer = vi.fn();
  render(<AgentPermissionCard request={request} onAnswer={onAnswer} planned />);
  expect(screen.getByText(/Planned/)).toBeVisible();
  expect(screen.getByRole("button", { name: "Allow once" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Allow once" }));
  expect(onAnswer).not.toHaveBeenCalled();
});

it("shows the plain reason and stays off where agent access cannot be turned on", async () => {
  render(<AgentAccessSetting />);
  expect(await screen.findByText(/not available on this service yet/i)).toBeVisible();
  expect(screen.getByRole("switch", { name: "Let agents use devices" })).toBeDisabled();
  expect(screen.getByRole("switch", { name: "Let agents use devices" })).toHaveAttribute("aria-checked", "false");
});

it("reads the grant state without changing it", async () => {
  const grant = { agent: "Claude Code", device: "iPhone 16", scope: "thread" as const, thread: "preview-thread" };
  const mock = createDevicesPlannedMock({ access: true, agentGrant: grant });
  setDevicesPlannedApi(mock);
  render(<AgentAccessSetting />);
  const toggle = await screen.findByRole("switch", { name: "Let agents use devices" });
  expect(toggle).toHaveAttribute("aria-checked", "true");
  expect(mock.accessWrites).toEqual([]);
  expect(screen.getByText("On. An agent can use a device when a run asks.")).toBeVisible();
});
