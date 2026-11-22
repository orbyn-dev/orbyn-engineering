/** Synthetic data for the two approved controls; no provider or forge effects. */
import type { CrewAcpTierView } from "../src/generated/CrewAcpTierView.ts";
import type { CrewLaneRemovalReceipt } from "../src/generated/CrewLaneRemovalReceipt.ts";
import type { CrewAcpTierSettings } from "../src/generated/CrewAcpTierSettings.ts";
import type { MockResponse } from "./api.ts";
import { flowMock } from "./flow.ts";
import type { FlowTaskDetail } from "../src/generated/FlowTaskDetail.ts";

export const exampleTierView = (): CrewAcpTierView => ({ writable: true, settings: { revision: 1, assignments: { "example-acp": { tier: "worker", model: "example-model-id" } } }, engines: [
  { id: "claude", name: "Claude Code", native: true, available: true, unavailable_reason: null, defaults: [] },
  { id: "codex", name: "Codex", native: true, available: true, unavailable_reason: null, defaults: [] },
  { id: "example-acp", name: "Example ACP", native: false, available: true, unavailable_reason: null, defaults: ["example-model-id"] },
] });
export function createReviewControlsMock() {
  let view = exampleTierView();
  let receipts: Record<string, CrewLaneRemovalReceipt> = {};
  // `crew-worker-provider-any` starts on Any provider; `crew-worker-provider-fail` refuses the write.
  let providers: Record<string, "same_as_lead" | "any"> = {};
  return {
    reset(next = "default") {
      view = exampleTierView();
      // The engine picker scenario adds the reviewer-only ACP engine its Crew tiers list shows.
      if (next === "crew-engine-picker") {
        view.engines.push({ id: "kiro", name: "Kiro", native: false, available: true, unavailable_reason: null, defaults: ["kiro-default"] });
      }
      receipts = {}; providers = {};
    },
    handle(scenario: string, method: string, path: string, body: unknown): MockResponse | null {
      if (!scenario.startsWith("crew")) return null;
      const fail = (message: string, status = 400) => ({ status, body: { code: "invalid", message } });
      const task = /^\/api\/crew\/review\/([^/]+)\/controls$/.exec(path)?.[1];
      if (task && method === "GET") {
        const found = flowMock(scenario, "GET", `/api/crew/flow/tasks/${task}`, new URLSearchParams());
        if (!found || found.status !== 200) return { status: 404, body: { code: "not_found", message: "Task" } };
        const passed = (found.body as FlowTaskDetail).row.state === "passed";
        const rule = scenario.startsWith("crew-review-remove");
        return { status: 200, body: { task: decodeURIComponent(task), error: null, lanes: [
          { lane: rule ? "licence" : "correctness", source: rule ? "Repository rule · .orbyn/crew.json" : "Default review lane", locked: false, can_remove: passed && !receipts[`${task}|${rule ? "licence" : "correctness"}`], removal: receipts[`${task}|${rule ? "licence" : "correctness"}`] ?? null },
          { lane: "security", source: rule ? "Repository rule · .orbyn/crew.json" : "Default review lane", locked: rule, can_remove: passed && !rule && !receipts[`${task}|security`], removal: receipts[`${task}|security`] ?? null },
        ] } };
      }
      const remove = /^\/api\/crew\/review\/([^/]+)\/remove-lane$/.exec(path)?.[1];
      if (remove && method === "POST") {
        const input = body as {lane?: string; reason?: string};
        const found = this.handle(scenario, "GET", `/api/crew/review/${remove}/controls`, null);
        const row = (found?.body as { lanes?: { lane: string; can_remove: boolean; removal: unknown }[] })?.lanes?.find((l) => l.lane === input.lane);
        if (!input.lane || !row || (!row.can_remove && !row.removal)) return fail("This lane cannot be removed.");
        const reason = input.reason?.trim() ?? ""; if (!reason || [...reason].length > 1000) return fail("Give a reason of 1 to 1000 characters.");
        const key = `${remove}|${input.lane}`;
        receipts[key] ??= { id: `synthetic-removal-${remove}`, lane: input.lane, reason };
        return { status: 200, body: { task: decodeURIComponent(remove), receipt: receipts[key] } };
      }
      const provider = /^\/api\/crew\/projects\/([^/]+)\/worker-provider(\/set)?$/.exec(path);
      if (provider) {
        const project = decodeURIComponent(provider[1]!);
        if (!["p-app", "p-site", "p-new"].includes(project)) return { status: 404, body: { code: "not_found", message: "Project" } };
        const current = () => providers[project] ?? (scenario === "crew-worker-provider-any" ? "any" : "same_as_lead");
        if (!provider[2] && method === "GET") return { status: 200, body: { project, worker_provider: current() } };
        if (provider[2] && method === "POST") {
          const choice = (body as { worker_provider?: string } | null)?.worker_provider;
          if (choice !== "same_as_lead" && choice !== "any") return fail("Worker provider must be same_as_lead or any.");
          if (scenario === "crew-worker-provider-fail") return { status: 500, body: { code: "internal", message: "Could not save the worker provider." } };
          providers[project] = choice;
          return { status: 200, body: { project, worker_provider: choice } };
        }
      }
      if (path === "/api/crew/settings/engine-tiers" && method === "GET") return { status: 200, body: structuredClone(view) };
      if (path === "/api/crew/settings/engine-tiers/save" && method === "POST") {
        const input = body as {seen?: number; assignments?: {engine: string; tier: string; expected_default_model: string}[]};
        if (input.seen !== view.settings.revision) return fail("Mappings are stale.", 409);
        const assignments: CrewAcpTierSettings["assignments"] = {};
        for (const row of input.assignments ?? []) {
          const engine = view.engines.find((e) => e.id === row.engine);
          if (!engine || engine.native || !engine.available || engine.defaults.length !== 1 || row.expected_default_model !== engine.defaults[0] || Object.hasOwn(assignments, row.engine) || !["lead", "worker", "scout"].includes(row.tier)) return fail("Review the configured default before saving.");
          assignments[row.engine] = { tier: row.tier as "lead" | "worker" | "scout", model: engine.defaults[0]! };
        }
        view.settings = { revision: view.settings.revision + 1, assignments };
        return { status: 200, body: structuredClone(view.settings) };
      }
      return null;
    },
  };
}
