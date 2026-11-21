import type { CapacityState } from "../src/generated/CapacityState.ts";
import type { ReviewCoverage } from "../src/generated/ReviewCoverage.ts";
import type { MockResponse, Scenario } from "./api.ts";

/**
 * The Review capacity and coverage routes (Crew Designs v77), on the sample the designs draw: a limit of 8 saved, pausing
 * off and every batch reviewed. Like the designs' own sample, the project is 24 measured merges and 14 not measured, and
 * 3,660 agent-written lines of which 2,280 (62%) no person has read. Scenarios turn Crew Preview on as every `crew-*`
 * scenario does: "crew-capacity" answers the two reads and takes a person's own settings save as the service does, refusing
 * a limit under 1 in the service's own words; "crew-capacity-save-fails" refuses every save with 403 `person_required`
 * ("Only a person can change this."), as a request without the launch's person key is refused.
 */
const T = 1_793_923_200;

const savedState = (over: Partial<CapacityState> = {}): CapacityState => ({
  project: "p-app",
  waiting: 5,
  limit: 8,
  sampling: "off",
  pause_enabled: false,
  settings_saved: true,
  plan: [
    { from: 1, to: 4, sampled: 1 },
    { from: 5, to: 8, sampled: 2 },
    { from: 9, to: 12, sampled: 3 },
    { from: 13, to: 16, sampled: 4 },
  ],
  held: [],
  picked: [],
  ...over,
});

const coverageMap = (): ReviewCoverage => ({
  project: "p-app",
  began_unix: T - 26 * 24 * 3600,
  as_of_unix: T + 17 * 3600,
  merges: 24,
  not_measured_merges: 14,
  agent_lines: 3660,
  reviewed_lines: 1380,
  in_queue_lines: 310,
  areas: [
    { area: "web/auth", merges: 9, agent_lines: 1420, reviewed_lines: 910, in_queue_lines: 200, not_measured_merges: 0 },
    { area: "web/app", merges: 6, agent_lines: 1060, reviewed_lines: 120, in_queue_lines: 60, not_measured_merges: 0 },
    { area: "docs/guides", merges: 5, agent_lines: 940, reviewed_lines: 350, in_queue_lines: 50, not_measured_merges: 0 },
    { area: "tests", merges: 4, agent_lines: 240, reviewed_lines: 0, in_queue_lines: 0, not_measured_merges: 0 },
  ],
  engines: [
    { engine: "claude", model: "Sonnet 5.5", agent_lines: 2100, reviewed_lines: 900 },
    { engine: "codex", model: "GPT-5.5", agent_lines: 1560, reviewed_lines: 480 },
  ],
});

export function createCapacityMock() {
  let limit = 8;
  let sampling: "off" | "one_in_four" = "off";
  let pause = false;
  let refuseSave = false;

  const state = (): CapacityState => savedState({ limit, sampling, pause_enabled: pause });

  function reset(next: Scenario) {
    limit = 8;
    sampling = "off";
    pause = false;
    refuseSave = next === "crew-capacity-save-fails";
  }

  /** The two reads and the one settings write; `null` for every other route, and while Crew Preview is off. */
  function handle(scenario: Scenario, method: string, path: string, body: unknown): MockResponse | null {
    if (!(scenario === "crew" || scenario.startsWith("crew-")) || !path.startsWith("/api/crew/capacity")) return null;
    if (path === "/api/crew/capacity" && method === "GET") return { status: 200, body: state() };
    if (path === "/api/crew/capacity/coverage" && method === "GET") return { status: 200, body: coverageMap() };
    if (path === "/api/crew/capacity/settings" && method === "POST") {
      if (refuseSave) return { status: 403, body: { code: "person_required", message: "Only a person can change this." } };
      const input = body as { limit?: number; sampling?: "off" | "one_in_four"; pause_enabled?: boolean } | null;
      if (!input || typeof input.limit !== "number") return { status: 400, body: { code: "invalid", message: "the request body is not a capacity settings write" } };
      if (input.limit < 1) return { status: 400, body: { code: "invalid", message: "the capacity limit must be at least 1" } };
      limit = input.limit;
      sampling = input.sampling === "one_in_four" ? "one_in_four" : "off";
      pause = input.pause_enabled === true;
      return { status: 200, body: state() };
    }
    return null;
  }

  return { reset, handle };
}
