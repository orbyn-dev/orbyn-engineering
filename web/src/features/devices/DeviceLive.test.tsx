import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createDevicesMock, createDevicesPlannedMock, stateFixture } from "../../../mock/devices";
import { IOS_LOGIN } from "../../../mock/deviceScreens";
import { DeviceView } from "./DeviceView";
import { setDevicesApi } from "./api";
import type { Device } from "./contract";
import { setDevicesPlannedApi } from "./planned";

afterEach(() => { setDevicesApi(null); setDevicesPlannedApi(null); vi.unstubAllGlobals(); });

function renderDevice(device: Device = stateFixture("booted")) {
  const api = createDevicesMock({ devices: [device] });
  setDevicesApi(api);
  const view = render(
    <DeviceView
      device={device}
      onChanged={() => {}}
      onGone={() => {}}
      onRestart={() => {}}
    />,
  );
  return { api, view };
}

it("keeps the live screen Planned in production while the built toolbar keeps working", async () => {
  const { api } = renderDevice();
  const toolbar = await screen.findByRole("toolbar", { name: "Device controls" });
  expect(await screen.findByText("No screenshot yet. The live screen is Planned.")).toBeVisible();
  expect(screen.queryByRole("img", { name: /Live screen of/ })).toBeNull();
  expect(screen.getByText("Live screen · Planned")).toBeVisible();
  expect(screen.getByText("Type text · Planned")).toBeVisible();

  await userEvent.setup().click(within(toolbar).getByRole("button", { name: "Screenshot" }));
  expect(await screen.findByRole("img", { name: "Screenshot of iPhone 16" })).toBeVisible();
  expect(api.writes).toEqual(["ios:7C2D-0001:screenshot"]);
});

it("draws the Planned stream and sends a tap in normalized screen coordinates", async () => {
  const { api } = renderDevice();
  const planned = createDevicesPlannedMock({ live: { screen: IOS_LOGIN } });
  setDevicesPlannedApi(planned);

  const image = await screen.findByRole("img", { name: "Live screen of iPhone 16" });
  const live = image.parentElement!;
  vi.spyOn(live, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 390, bottom: 844, width: 390, height: 844, toJSON: () => ({}) });

  fireEvent.pointerDown(live, { pointerId: 7, button: 0, clientX: 195, clientY: 422 });
  fireEvent.pointerUp(live, { pointerId: 7, button: 0, clientX: 195, clientY: 422 });

  await waitFor(() => expect(planned.pointers).toEqual([{ type: "tap", x: 0.5, y: 0.5 }]));
  expect(api.writes).toEqual([]);
});

it("opens a type field, sends the text and sends Enter on Return", async () => {
  const { api } = renderDevice();
  const planned = createDevicesPlannedMock({ live: { screen: IOS_LOGIN } });
  setDevicesPlannedApi(planned);
  const user = userEvent.setup();

  await user.click(await screen.findByRole("button", { name: "Type text" }));
  const field = screen.getByRole("textbox", { name: "Text to type" });
  await user.type(field, "user@example.com");
  await user.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(planned.typed).toEqual(["user@example.com"]));

  await user.type(field, "password");
  await user.keyboard("{Enter}");
  await waitFor(() => expect(planned.typed).toEqual(["user@example.com", "password\n"]));
  expect(api.writes).toEqual([]);
});

it("names the agent that is driving, dims the rail, takes over and hands back", async () => {
  renderDevice();
  const planned = createDevicesPlannedMock({ live: { screen: IOS_LOGIN, agent: "driving", agentName: "Claude Code" } });
  setDevicesPlannedApi(planned);
  const user = userEvent.setup();

  expect(await screen.findByText("Claude Code is using this device")).toBeVisible();
  const rail = await screen.findByRole("toolbar", { name: "Device controls" });
  expect(rail).toHaveAttribute("data-agent", "driving");

  await user.click(screen.getByRole("button", { name: "Take over" }));
  expect(await screen.findByText("Paused. You are in control.")).toBeVisible();
  expect(planned.agentChanges).toEqual(["take_over"]);

  await user.click(screen.getByRole("button", { name: "Hand back" }));
  expect(await screen.findByText("Claude Code is using this device")).toBeVisible();
  expect(planned.agentChanges).toEqual(["take_over", "hand_back"]);
});

it("pauses a driving agent when the person touches the screen", async () => {
  renderDevice();
  const planned = createDevicesPlannedMock({ live: { screen: IOS_LOGIN, agent: "driving", agentName: "Claude Code" } });
  setDevicesPlannedApi(planned);

  const image = await screen.findByRole("img", { name: "Live screen of iPhone 16" });
  const live = image.parentElement!;
  vi.spyOn(live, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 390, bottom: 844, width: 390, height: 844, toJSON: () => ({}) });
  fireEvent.pointerDown(live, { pointerId: 3, button: 0, clientX: 100, clientY: 200 });
  fireEvent.pointerUp(live, { pointerId: 3, button: 0, clientX: 100, clientY: 200 });

  expect(await screen.findByText("Paused. You are in control.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Hand back" })).toBeVisible();
});

it("passes the running stream to the tools drawer so outlines stay enabled", async () => {
  renderDevice();
  setDevicesPlannedApi(createDevicesPlannedMock({ live: { screen: IOS_LOGIN } }));
  await screen.findByRole("img", { name: "Live screen of iPhone 16" });
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /Device tools/ }));
  const drawer = await screen.findByRole("region", { name: "Device tools" });
  await user.click(within(drawer).getByRole("tab", { name: "Accessibility" }));
  const toggle = await within(drawer).findByRole("switch", { name: "Show elements" });
  expect(toggle).toBeEnabled();
  expect(within(drawer).getByText("Sign in · Button")).toBeVisible();
});

it("keeps outlines disabled when the production client has no live screen", async () => {
  renderDevice();
  const toolsOnly = { ...createDevicesPlannedMock() };
  delete (toolsOnly as { stream?: unknown }).stream;
  setDevicesPlannedApi(toolsOnly);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /Device tools/ }));
  const drawer = await screen.findByRole("region", { name: "Device tools" });
  await user.click(within(drawer).getByRole("tab", { name: "Accessibility" }));
  const toggle = await within(drawer).findByRole("switch", { name: "Show elements" });
  expect(toggle).toBeDisabled();
  expect(toggle).toHaveAttribute("title", "Outlines need a live screen.");
  expect(within(drawer).getByText("Outlines need a live screen.")).toBeVisible();
  expect(within(drawer).getByText("No screen is live.")).toBeVisible();
  expect(within(drawer).queryByText("Sign in · Button")).toBeNull();
});
