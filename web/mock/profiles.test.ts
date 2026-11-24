import { expect, it } from "vitest";
import { createProfilesMock } from "./profiles.ts";

const refusal = async (run: () => Promise<unknown>) => run().then(() => null, (error: { code: string; status: number; detail?: unknown }) => error);

it("names in confirm_required only the grants the person has not acknowledged, as the service does", async () => {
  const mock = createProfilesMock();
  const none = await refusal(() => mock.applyToThread("t1", { profile_id: "01j0000000000000000000a003" }));
  expect(none).toMatchObject({ code: "confirm_required", status: 409, detail: { grants: ["full_access", "autopilot_full"] } });
  const partial = await refusal(() => mock.applyToThread("t1", { profile_id: "01j0000000000000000000a003", ack_grants: ["full_access"] }));
  expect(partial).toMatchObject({ code: "confirm_required", detail: { grants: ["autopilot_full"] } });
});

it("answers run_active and desktop_required with their real statuses", async () => {
  const mock = createProfilesMock();
  mock.failNext("run_active");
  expect(await refusal(() => mock.applyToThread("t1", { profile_id: "builtin:careful" }))).toMatchObject({ code: "run_active", status: 409 });
  mock.failNext("desktop_required");
  expect(await refusal(() => mock.applyToThread("t1", { profile_id: "builtin:careful" }))).toMatchObject({ code: "desktop_required", status: 403 });
});

it("answers a crew apply with applied.autopilot_mode and a thread apply with it ignored", async () => {
  const mock = createProfilesMock();
  const crew = await mock.applyToCrew("c1", { profile_id: "builtin:fast" });
  expect(crew.applied.autopilot_mode).toBe("on");
  expect(crew.ignored).toEqual([]);
  const thread = await mock.applyToThread("t1", { profile_id: "builtin:fast" });
  expect(thread.applied).not.toHaveProperty("autopilot_mode");
  expect(thread.ignored).toEqual(["autopilot_mode"]);
  expect(await mock.applyToThread("t1", { profile_id: null })).toEqual({ applied: {}, ignored: [], application: null });
});
