/**
 * The mock's Needs you reads: the after-merge Review queue and the background crews with their runs, which `mock/crew.ts` does not
 * serve. They answer in the Crew scenarios (`crew`, `crew-<state>`) and nowhere else, so the default scenario keeps every Crew route at
 * 404 as the service does with Crew Preview off. `crew-empty` has nothing waiting. Reads only.
 */
import type { AutopilotReviewItem } from "../src/generated/AutopilotReviewItem.ts";
import type { AutopilotReviewQueue } from "../src/generated/AutopilotReviewQueue.ts";
import type { BackgroundCrew } from "../src/generated/BackgroundCrew.ts";
import type { BackgroundCrewList } from "../src/generated/BackgroundCrewList.ts";
import type { BackgroundRun } from "../src/generated/BackgroundRun.ts";
import type { BackgroundRunList } from "../src/generated/BackgroundRunList.ts";
import type { MockResponse } from "./api.ts";

const NOW = Date.parse("2026-11-07T15:00:00Z") / 1000;

const reviewItem = (): AutopilotReviewItem => ({
  task_id: "task-redirect",
  risk: "high",
  reasons: ["touches auth/"],
  merge_commit: "9be4410c0ffee0123456789abcdef0123456789a",
  before_commit: "f1a0c63c0ffee0123456789abcdef0123456789a",
  candidate: "crew/redirect-impl",
  files: 3,
  lines: 41,
  merged_unix: NOW - 3600,
  due_unix: null,
  overdue: false,
  proof: {},
  not_verified: [],
  cost: {},
  watch: "broken",
  needs_person: "Main broke after this merge and the cause is not certain.",
});

const crew = (): BackgroundCrew => ({
  id: "bg-ci",
  project_id: "p-1",
  name: "Fix failing CI",
  template: "fix_failing_ci",
  trigger: { kind: "ci_failure" },
  budget: { turns: 40, minutes: 30 },
  enabled: true,
  state: "needs_review",
  reviews_waiting: 1,
  created_at: "2026-11-01T10:00:00Z",
  updated_at: "2026-11-07T14:00:00Z",
});

const run = (): BackgroundRun => ({
  id: "run-1",
  crew_id: "bg-ci",
  state: "needs_review",
  stop_reason: "budget",
  turns_spent: 40,
  minutes_spent: 12,
  budget: { turns: 40, minutes: 30 },
  review_waiting: true,
  review_reason: "It used its whole turn budget before the checks passed",
  started_at: "2026-11-07T13:40:00Z",
  updated_at: "2026-11-07T14:00:00Z",
  ended_at: "2026-11-07T14:00:00Z",
});

/** The answer for a Needs you read, or null when the route is not one of these (or Crew is off) and the Crew mock answers. */
export function needsYouMock(scenario: string, method: string, path: string): MockResponse | null {
  if (method !== "GET" || !(scenario === "crew" || scenario.startsWith("crew-"))) return null;
  const empty = scenario === "crew-empty";
  if (path === "/api/crew/autopilot/review") {
    const body: AutopilotReviewQueue = { items: empty ? [] : [reviewItem()], total: empty ? 0 : 1, next_offset: null, now_unix: NOW };
    return { status: 200, body };
  }
  if (path === "/api/crew/background") {
    const body: BackgroundCrewList = { crews: empty ? [] : [crew()], total: empty ? 0 : 1, next_offset: null };
    return { status: 200, body };
  }
  if (path === "/api/crew/background/bg-ci/runs") {
    const body: BackgroundRunList = { runs: [run()], total: 1, next_offset: null };
    return { status: 200, body };
  }
  return null;
}
