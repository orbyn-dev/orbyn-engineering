import { expect, it } from "vitest";
import { createMockApi } from "./api";
it("the mock keeps download consent distinct from install and rejects stale generations", () => {
  const api = createMockApi();
  api.reset("empty");
  const before = api.updates.app.current;
  const request = { release_id: api.updates.app.release_id, operation: api.updates.app.operation };
  expect(api.handle("POST", "/api/updates/app/apply", request).status).toBe(422);
  expect(api.handle("POST", "/api/updates/app/download", request).status).toBe(202);
  expect(api.updates.app.current).toBe(before);
  expect(api.updates.app.phase).toBe("ready");
  expect(api.handle("POST", "/api/updates/app/apply", request).status).toBe(422);
  expect(api.handle("POST", "/api/updates/app/apply", { ...request, operation: api.updates.app.operation }).status).toBe(202);
  expect(api.updates.app.current).toBe(api.updates.app.latest);
});
