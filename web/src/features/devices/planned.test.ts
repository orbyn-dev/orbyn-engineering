import { afterEach, expect, it, vi } from "vitest";
import { createDevicesPlannedMock } from "../../../mock/devices";
import { devicesLivePlannedApi, devicesPlannedApi, setDevicesPlannedApi } from "./planned";

afterEach(() => { setDevicesPlannedApi(null); vi.unstubAllGlobals(); });

it("sends nothing for any Planned piece: every read answers null and no request leaves the app", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  const api = devicesPlannedApi();
  const progress = vi.fn();
  expect(await api.status()).toBeNull();
  expect(await api.tools("android")).toBeNull();
  expect(await api.check("ios")).toBeNull();
  expect(await api.install("android", progress)).toBeNull();
  expect(await api.setAccess(true)).toBeNull();
  expect(await api.restart("android:Pixel_8_API_35")).toBeNull();
  expect(await api.versions()).toBeNull();
  expect(progress).not.toHaveBeenCalled();
  expect(fetching).not.toHaveBeenCalled();
});

it("sends nothing for the Planned live screen, pointer, typing or agent state either", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  const live = devicesLivePlannedApi();
  const frame = vi.fn();
  expect(await live.stream("ios:7C2D-0001", frame)).toBeNull();
  expect(await live.pointer("ios:7C2D-0001", { type: "tap", x: 0.5, y: 0.5 })).toBeNull();
  expect(await live.typeText("ios:7C2D-0001", "user@example.com")).toBeNull();
  expect(await live.agentState("ios:7C2D-0001")).toBeNull();
  expect(await live.agentState("ios:7C2D-0001", "take_over")).toBeNull();
  expect(frame).not.toHaveBeenCalled();
  expect(fetching).not.toHaveBeenCalled();
});

it("installs a typed client only when a test or the preview swaps one in", async () => {
  const mock = createDevicesPlannedMock({ access: true });
  setDevicesPlannedApi(mock);
  expect(await devicesPlannedApi().status()).toEqual({ supported: true });
  expect((await devicesPlannedApi().tools("ios"))?.access).toBe(true);
  setDevicesPlannedApi(null);
  expect(await devicesPlannedApi().status()).toBeNull();
});

it("serves the live members from the same Planned client and stops when it is removed", async () => {
  const mock = createDevicesPlannedMock({
    live: { screen: "iVBORw0KGgo", agent: "driving", agentName: "Claude Code" },
  });
  setDevicesPlannedApi(mock);
  const got: string[] = [];
  expect(await devicesLivePlannedApi().stream("ios:7C2D-0001", (frame) => got.push(frame.image))).toBe(true);
  expect(got).toHaveLength(1);
  expect(await devicesLivePlannedApi().agentState("ios:7C2D-0001")).toEqual({ status: "driving", name: "Claude Code" });
  setDevicesPlannedApi(null);
  expect(await devicesLivePlannedApi().stream("ios:7C2D-0001", () => {})).toBeNull();
});
