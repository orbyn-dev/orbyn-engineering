import { afterEach, expect, it, vi } from "vitest";
import { devicesPlannedApi, setDevicesPlannedApi } from "../planned";

afterEach(() => { setDevicesPlannedApi(null); vi.unstubAllGlobals(); });

it("sends nothing for the device tools or agent permission: every read answers null", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  const api = devicesPlannedApi();
  expect(await api.deviceTools("ios:7C2D-0001")).toBeNull();
  expect(await api.setDeviceTool("ios:7C2D-0001", { type: "appearance", mode: "dark" })).toBeNull();
  expect(await api.sendDevicePush("ios:7C2D-0001", "{}")).toBeNull();
  expect(await api.agentAccess()).toBeNull();
  expect(await api.setAgentAccess(true)).toBeNull();
  expect(await api.answerAgentRequest("req-1", "once")).toBeNull();
  expect(await api.revokeAgentAccess()).toBeNull();
  expect(fetching).not.toHaveBeenCalled();
});
