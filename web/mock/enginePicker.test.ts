import { expect, it } from "vitest";
import type { EngineStatus, PreferenceView, Thread } from "../src/api/types";
import type { CrewAcpTierView } from "../src/generated/CrewAcpTierView";
import { createMockApi } from "./api";

it("the engine picker scenario has Kiro unmanaged and reviewer only, chosen for new threads and listed in the Crew tiers", () => {
  const api = createMockApi();
  api.reset("crew-engine-picker");
  const engines = api.handle("GET", "/api/engines").body as EngineStatus[];
  expect(engines.find((engine) => engine.id === "kiro")).toMatchObject({
    name: "Kiro",
    installed: true,
    capabilities: { managed: false },
    crew_role: "reviewer_only",
  });
  const preferences = api.handle("GET", "/api/preferences").body as PreferenceView[];
  expect(preferences.find((row) => row.key === "default_engine")?.value).toBe("kiro");
  const tiers = api.handle("GET", "/api/crew/settings/engine-tiers").body as CrewAcpTierView;
  expect(tiers.engines.find((engine) => engine.id === "kiro")).toMatchObject({ name: "Kiro", native: false, available: true });
});

it("the unmanaged-thread scenario puts Kiro on a thread and answers its first send with Orbyn's notice", () => {
  const api = createMockApi();
  api.reset("unmanaged-thread");
  const kiro = (api.handle("GET", "/api/engines").body as EngineStatus[]).find((engine) => engine.id === "kiro");
  expect(kiro).toMatchObject({ name: "Kiro", capabilities: { managed: false, permission_modes: ["no_approvals"] } });
  const thread = (api.handle("GET", "/api/threads").body as Thread[]).find((entry) => entry.id === "t-empty");
  expect(thread).toMatchObject({ engine: "kiro", lifecycle: { permission_mode: "auto" } });
  const send = api.handle("POST", "/api/threads/t-empty/runs", { prompt: "Run the tests." });
  expect(send).toMatchObject({ status: 409, body: { code: "engine_notice_required", message: "Orbyn cannot ask before Kiro acts." } });
});
