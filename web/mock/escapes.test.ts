import { expect, it } from "vitest";
import { createMockApi } from "./api";

const write = (api: ReturnType<typeof createMockApi>, id: string, act: string, seen: { state: string; gap: string | null }, extra: object = {}) =>
  api.handle("POST", `/api/crew/escapes/${id}/${act}`, { project: "p-app", seen, ...extra });

it("answers the same confirm twice with 200, as the service answers an unchanged repeat before it checks stale", () => {
  const api = createMockApi();
  api.reset("crew-escapes");
  const seen = { state: "possible", gap: null };
  expect(write(api, "esc-cookie", "confirm", seen).status).toBe(200);
  expect(write(api, "esc-cookie", "confirm", seen).status).toBe(200);
});

it("answers the same dismiss twice with 200", () => {
  const api = createMockApi();
  api.reset("crew-escapes");
  const seen = { state: "possible", gap: null };
  expect(write(api, "esc-shortcut", "dismiss", seen).status).toBe(200);
  expect(write(api, "esc-shortcut", "dismiss", seen).status).toBe(200);
});

it("counts a same gap as unchanged only when a person set it; else a stale view is refused", () => {
  const api = createMockApi();
  api.reset("crew-escapes");
  const seen = { state: "confirmed", gap: null };
  expect(write(api, "esc-session", "gap", seen, { gap: "test_gap" }).status).toBe(200);
  expect(write(api, "esc-session", "gap", seen, { gap: "test_gap" }).status).toBe(200);
  expect(write(api, "esc-session", "gap", seen, { gap: "tooling_gap" }).status).toBe(409);
});
