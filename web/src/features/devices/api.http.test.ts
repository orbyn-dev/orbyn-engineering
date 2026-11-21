import { afterEach, expect, it, vi } from "vitest";
import { sendsPersonKey } from "../../api/personKey";
import { devicesApi, setDevicesApi } from "./api";

afterEach(() => { setDevicesApi(null); vi.unstubAllGlobals(); });
const answer = (status: number, body: unknown) => vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));

it("answers a 404 from the list as no feature, with no error", async () => {
  vi.stubGlobal("fetch", answer(404, { code: "not_found", message: "No such route." }));
  await expect(devicesApi().list()).resolves.toBeNull();
});

it("answers a body that is not a device list as no feature", async () => {
  vi.stubGlobal("fetch", answer(200, { hello: "world" }));
  await expect(devicesApi().list()).resolves.toBeNull();
});

it("reads the list and tolerates a missing platforms array", async () => {
  vi.stubGlobal("fetch", answer(200, { devices: [{ id: "android:a", name: "A", platform: "android", state: "booted", version: "15", foldable: false }] }));
  const list = await devicesApi().list();
  expect(list?.devices).toHaveLength(1);
  expect(list?.platforms).toEqual([]);
});

it("lets any other failure through, so it is not mistaken for a missing feature", async () => {
  vi.stubGlobal("fetch", answer(500, { code: "tool_failed", message: "x" }));
  await expect(devicesApi().list()).rejects.toMatchObject({ code: "tool_failed" });
});

it("posts an action to the encoded device path with the typed body", async () => {
  const fetching = answer(200, { ok: true, device: {} });
  vi.stubGlobal("fetch", fetching);
  await devicesApi().act("android:Pixel Fold/1", { type: "hinge", angle: 100 });
  const [url, init] = fetching.mock.calls[0]!;
  expect(String(url)).toContain("/api/devices/android%3APixel%20Fold%2F1/actions");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(String(init?.body))).toEqual({ type: "hinge", angle: 100 });
});

it("treats every action write as Person-class and the reads as not", () => {
  expect(sendsPersonKey("POST", "/api/devices/android%3Aa/actions")).toBe(true);
  expect(sendsPersonKey("POST", "/api/devices/ios%3Au1/actions")).toBe(true);
  expect(sendsPersonKey("GET", "/api/devices")).toBe(false);
  expect(sendsPersonKey("GET", "/api/devices/android%3Aa")).toBe(false);
});
