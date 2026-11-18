import { afterEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => {
  // The test setup loads the workspace store and API client before this file.
  vi.resetModules();
  return { origin: "http://127.0.0.1:1", headers: vi.fn<() => Promise<Record<string, string>>>() };
});
vi.mock("../../../../api/origin", () => ({ serviceUrl: (path: string) => state.origin + path }));
vi.mock("../../../../api/personKey", () => ({ personKeyHeaders: state.headers, forgetPersonKey: vi.fn() }));

import { crewApi } from "./api";

afterEach(() => {
  vi.unstubAllGlobals();
  state.origin = "http://127.0.0.1:1";
});

it.each(["confirm", "reset"] as const)("does not send a %s write or person key after the service changes", async (action) => {
  let release!: (value: Record<string, string>) => void;
  const headers = new Promise<Record<string, string>>((resolve) => { release = resolve; });
  state.headers.mockReset();
  state.headers.mockReturnValue(headers);
  const fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);

  const result = action === "confirm" ? crewApi.confirmPinnedRef("project", "commit") : crewApi.resetPinnedRef("project");
  const outcome = result.then(() => null, (error: unknown) => error);
  try {
    expect(state.headers).toHaveBeenCalledTimes(1);
    state.origin = "http://127.0.0.1:2";
  } finally {
    release({ "x-orbyn-person": "test-key" });
  }

  expect(await outcome).toMatchObject({ code: "service_changed", status: 0 });
  expect(fetchMock).not.toHaveBeenCalled();
});
