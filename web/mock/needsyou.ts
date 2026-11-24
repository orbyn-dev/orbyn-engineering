/**
 * The mock's Needs you reads: the after-merge Review queue, the background crews with their runs, and the watch rows of the
 * shared Needs you read (KC6), which `mock/crew.ts` does not serve. They answer in the Crew scenarios (`crew`,
 * `crew-<state>`) and nowhere else, so the default scenario keeps every Crew route at 404 as the service does with Crew
 * Preview off. `crew-empty` has nothing waiting. Reads only.
 */
import type { AutopilotReviewItem } from "../src/generated/AutopilotReviewItem.ts";
import type { AutopilotReviewQueue } from "../src/generated/AutopilotReviewQueue.ts";
import type { BackgroundCrew } from "../src/generated/BackgroundCrew.ts";
import type { BackgroundCrewList } from "../src/generated/BackgroundCrewList.ts";
import type { BackgroundRun } from "../src/generated/BackgroundRun.ts";
import type { BackgroundRunList } from "../src/generated/BackgroundRunList.ts";
import type { MockResponse } from "./api.ts";
import { MOCK_CREW } from "./flow.ts";

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
  daily: { runs: 2, turns: 80 },
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

/**
 * The two watch rows the Needs you read carries in `crew-background-watch-needsyou` (watch-needsyou). `since` is the
 * latest check of each watch in `mock/background.ts` (Redirect tests 14:02, Lockfile check 12:02), which is what the
 * service's item time means; the could-not-run row is not claiming that stretch began then, only that it is one row since.
 */
const watchRows = (project: string) => [
  {
    id: "bg-watch-redirect",
    kind: "watch_failing" as const,
    project_id: project,
    crew_id: "bg-watch-redirect",
    since: "2026-11-07T14:02:00Z",
    dismissible: false,
    reason: "The watch’s command is failing.",
  },
  {
    id: "bg-watch-lock",
    kind: "watch_could_not_run" as const,
    project_id: project,
    crew_id: "bg-watch-lock",
    since: "2026-11-07T12:02:00Z",
    dismissible: false,
    reason: "3 checks in a row could not run: timed out.",
  },
];

/**
 * The one item of `crew-could-not-stop` (kc2-close-lead-could-not-stop): the verifying task of the board's sample, whose worker the
 * close of its lead thread could not stop. The service's own line, the task's id and its crew; the name comes from the board.
 */
const couldNotStopRows = () => [
  {
    id: "b-verifying",
    kind: "could_not_stop" as const,
    project_id: "p-app",
    crew_id: MOCK_CREW,
    since: "2026-11-07T14:40:00Z",
    dismissible: false,
    reason: "The worker could not be stopped. Stop it again.",
  },
];

/** The answer for a Needs you read, or null when the route is not one of these (or Crew is off) and the Crew mock answers. */
export function needsYouMock(scenario: string, method: string, path: string): MockResponse | null {
  if (method !== "GET" || !(scenario === "crew" || scenario.startsWith("crew-"))) return null;
  const empty = scenario === "crew-empty" || scenario.startsWith("crew-background-watch") || scenario === "crew-could-not-stop";
  if (path === "/api/crew/needs-you") {
    const items = scenario === "crew-background-watch-needsyou" ? watchRows("p-app") : scenario === "crew-background-watch-needsyou-p-two" ? watchRows("p-two") : scenario === "crew-could-not-stop" ? couldNotStopRows() : [];
    return { status: 200, body: { items, count: items.length } };
  }
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
