import { afterEach, expect, it, vi } from "vitest";
import { createDevicesPlannedMock } from "../../../mock/devices";
import { devicesPlannedApi, setDevicesPlannedApi } from "./planned";

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

it("installs a typed client only when a test or the preview swaps one in", async () => {
  const mock = createDevicesPlannedMock({ access: true });
  setDevicesPlannedApi(mock);
  expect(await devicesPlannedApi().status()).toEqual({ supported: true });
  expect((await devicesPlannedApi().tools("ios"))?.access).toBe(true);
  setDevicesPlannedApi(null);
  expect(await devicesPlannedApi().status()).toBeNull();
});
