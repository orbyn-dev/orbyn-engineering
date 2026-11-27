/**
 * The mock's Needs you reads and the KC10 notify write: the after-merge Review queue, the background crews with their runs,
 * the watch rows of the shared Needs you read (KC6) and its could-not-stop row (KC2), which `mock/crew.ts` does not serve.
 * They answer in the Crew scenarios (`crew`, `crew-<state>`) and nowhere else, so the default scenario keeps every Crew
 * route at 404 as the service does with Crew Preview off. `crew-empty` has nothing waiting. Reads only, except the notify
 * route, which marks what it answers as told for this mock's lifetime so a later window is given nothing.
 *
 * The KC10 scenarios carry the items a notification test needs: `crew-notify-one` (one question), `crew-notify-two`
 * (two different kinds), `crew-notify-five` (five mixed kinds, one of them a watch), `crew-notify-claimed` (two questions
 * but the service says another window already has the second) and `crew-notify-missing` (Crew is on, the notify routes
 * answer 404).
 */
import type { AutopilotReviewItem } from "../src/generated/AutopilotReviewItem.ts";
import type { AutopilotReviewQueue } from "../src/generated/AutopilotReviewQueue.ts";
import type { BackgroundCrew } from "../src/generated/BackgroundCrew.ts";
import type { BackgroundCrewList } from "../src/generated/BackgroundCrewList.ts";
import type { BackgroundRun } from "../src/generated/BackgroundRun.ts";
import type { BackgroundRunList } from "../src/generated/BackgroundRunList.ts";
import type { NeedsYouItem } from "../src/generated/NeedsYouItem.ts";
import type { NeedsYouRef } from "../src/generated/NeedsYouRef.ts";
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
const watchRows = (project: string): NeedsYouItem[] => [
  {
    id: "bg-watch-redirect",
    kind: "watch_failing",
    project_id: project,
    crew_id: "bg-watch-redirect",
    since: "2026-11-07T14:02:00Z",
    dismissible: false,
    reason: "The watch’s command is failing.",
    question_hidden: false,
  },
  {
    id: "bg-watch-lock",
    kind: "watch_could_not_run",
    project_id: project,
    crew_id: "bg-watch-lock",
    since: "2026-11-07T12:02:00Z",
    dismissible: false,
    reason: "3 checks in a row could not run: timed out.",
    question_hidden: false,
  },
];

/**
 * The one item of `crew-could-not-stop` (kc2-close-lead-could-not-stop): the verifying task of the board's sample, whose worker the
 * close of its lead thread could not stop. It carries the service's own line and the task's id, crew, name, engine, model and kind,
 * so the row needs no board read. `crew-could-not-stop-no-name` is the same item without a name, so the row falls back to the id.
 */
const couldNotStopRows = (name: string | null = "Add tests for the redirect, including //evil.com"): NeedsYouItem[] => [
  {
    id: "b-verifying",
    kind: "could_not_stop",
    project_id: "p-app",
    crew_id: MOCK_CREW,
    since: "2026-11-07T14:40:00Z",
    dismissible: false,
    reason: "The worker could not be stopped. Stop it again.",
    ...(name === null ? {} : { name }),
    engine: "codex",
    model: "GPT-5.5",
    task_kind: "test",
    question_hidden: false,
  },
];

/**
 * The one item of `crew-worker-stopped-twice` (kc8-resumed-inbox): a task whose worker died again after its one resume. It carries the
 * service's own line; `crew-worker-stopped-twice-no-reason` is the same item with that line empty, so the row falls back to the design's
 * sentence.
 */
const workerStoppedTwiceRows = (reason: string | null = "The worker’s engine stopped with an error. It had already resumed once, so it did not resume again."): NeedsYouItem[] => [
  {
    id: "7a3e91c",
    kind: "worker_stopped_twice",
    project_id: "p-app",
    crew_id: MOCK_CREW,
    since: "2026-11-07T14:40:00Z",
    dismissible: true,
    ...(reason === null ? {} : { reason }),
    question_hidden: false,
  },
];

/** One KC10 item, with every field the card reads. */
const notifyItem = (over: Partial<NeedsYouItem> & Pick<NeedsYouItem, "id" | "kind" | "since">): NeedsYouItem => ({
  project_id: "p-app",
  crew_id: "crew-app",
  dismissible: false,
  question_hidden: false,
  ...over,
});

const question = (id: string, text: string, since: string): NeedsYouItem =>
  notifyItem({ id, kind: "task_needs_person", since, reason: text, question: text });

/** The items of each KC10 scenario, before the notify write marks any of them told. */
function notifyRows(scenario: string): NeedsYouItem[] {
  if (scenario === "crew-notify-one") {
    return [question("task-q1", "Which target wins when both next and returnTo are set?", "2026-11-07T14:02:00Z")];
  }
  if (scenario === "crew-notify-two") {
    return [
      notifyItem({ id: "task-refused", kind: "could_not_start", since: "2026-11-07T14:03:00Z", reason: "Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again." }),
      notifyItem({ id: "task-stopped", kind: "tripwire_stop", since: "2026-11-07T14:04:00Z", reason: "A trip wire stopped the task." }),
    ];
  }
  if (scenario === "crew-notify-five") {
    return [
      question("task-q1", "Which target wins when both next and returnTo are set?", "2026-11-07T14:02:00Z"),
      question("task-q2", "Should the redirect keep the query string?", "2026-11-07T14:03:00Z"),
      notifyItem({ id: "task-merge", kind: "merge_waiting", since: "2026-11-07T14:04:00Z", reason: "The merge waits for you." }),
      notifyItem({ id: "run-review", kind: "background_review", since: "2026-11-07T14:05:00Z", reason: "It used its whole turn budget before the checks passed" }),
      notifyItem({ id: "bg-watch-redirect", kind: "watch_failing", since: "2026-11-07T14:06:00Z", reason: "The watch’s command is failing." }),
    ];
  }
  if (scenario === "crew-notify-claimed") {
    return [
      question("task-q1", "Which target wins when both next and returnTo are set?", "2026-11-07T14:02:00Z"),
      question("task-q2", "Should the redirect keep the query string?", "2026-11-07T14:03:00Z"),
    ];
  }
  return [];
}

/** The Needs you mock of one API instance: the tellings it has already answered live no longer than the instance. */
export function createNeedsYouMock() {
  const announced = new Set<string>();
  const keyOf = (ref: NeedsYouRef): string => `${ref.kind}:${ref.id}`;

  /** The answer for a Needs you read, or null when the route is not one of these (or Crew is off) and the Crew mock answers. */
  function handle(scenario: string, method: string, path: string, body?: unknown): MockResponse | null {
    if (!(scenario === "crew" || scenario.startsWith("crew-"))) return null;

    if (path === "/api/crew/needs-you/notify") {
      if (method !== "POST" || !scenario.startsWith("crew-notify-")) return null;
      if (scenario === "crew-notify-missing") return { status: 404, body: { code: "not_found", message: "route" } };
      const refs = (body as { items?: NeedsYouRef[] } | undefined)?.items ?? [];
      const current = notifyRows(scenario);
      const claimed = refs.filter((ref) => {
        const known = current.find((item) => item.kind === ref.kind && item.id === ref.id);
        return known !== undefined && !announced.has(keyOf(ref));
      });
      // `crew-notify-claimed`: another window already has the second item, so only the first is answered.
      const answer = scenario === "crew-notify-claimed" ? claimed.slice(0, 1) : claimed;
      for (const ref of answer) announced.add(keyOf(ref));
      return { status: 200, body: { claimed: answer } };
    }

    if (method !== "GET") return null;
    if (path === "/api/crew/needs-you") {
      if (scenario === "crew-notify-missing") return { status: 404, body: { code: "not_found", message: "route" } };
      if (scenario.startsWith("crew-notify-")) {
        const items = notifyRows(scenario);
        return { status: 200, body: { items, count: items.length } };
      }
      const items = scenario === "crew-background-watch-needsyou" ? watchRows("p-app") : scenario === "crew-background-watch-needsyou-p-two" ? watchRows("p-two") : scenario === "crew-could-not-stop" ? couldNotStopRows() : scenario === "crew-could-not-stop-no-name" ? couldNotStopRows(null) : scenario === "crew-could-not-stop-and-watch" ? [...couldNotStopRows(), ...watchRows("p-app")] : scenario === "crew-worker-stopped-twice" ? workerStoppedTwiceRows() : scenario === "crew-worker-stopped-twice-no-reason" ? workerStoppedTwiceRows("") : [];
      return { status: 200, body: { items, count: items.length } };
    }
    const empty = scenario === "crew-empty" || scenario.startsWith("crew-background-watch") || scenario.startsWith("crew-could-not-stop") || scenario.startsWith("crew-worker-stopped-twice");
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

  return { handle, reset: () => announced.clear() };
}
