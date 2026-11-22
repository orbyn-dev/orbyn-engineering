import { expect, it } from "vitest";
import type { EngineStatus, PreferenceView } from "../src/api/types";
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
