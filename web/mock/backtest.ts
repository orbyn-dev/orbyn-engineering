import type { BacktestEstimate } from "../src/generated/BacktestEstimate.ts";
import type { BacktestRun } from "../src/generated/BacktestRun.ts";
import type { MockResponse, Scenario } from "./api.ts";

/**
 * The reviewer backtest's mock (slice 4n): the estimate and the report reads with the wire shapes, over the Crew Designs
 * v23 sample run. Scenarios: "crew-backtest" (the estimate: 64 merged pull requests 30 days old or more, outcomes measured,
 * nothing running), "crew-backtest-report" (the finished run `run-4n`, two lanes scored and one Not measured) and
 * "crew-backtest-stopped" (the stopped run `run-stopped`, Partial). The mock sends no start or stop, because no screen
 * composes one: the app has no read of the lanes, the lane kinds or the per-engine budgets.
 */

/** Sat 7 Nov 2026, 16:02 UTC: the day the design's sample run lives on. */
const T = 1_793_923_200 + 16 * 3_600 + 2 * 60;

const estimate = (): BacktestEstimate => ({
  project: "p-app",
  history_days: 180,
  outcome_days: 30,
  eligible_prs: 64,
  lanes: [],
  outcomes_measured: true,
  running: null,
});

const finished = (): BacktestRun => ({
  id: "run-4n",
  project: "p-app",
  state: "finished",
  budgets: [
    { engine: "claude", tokens: 1_500_000 },
    { engine: "codex", tokens: 1_000_000 },
    { engine: "opencode", tokens: 500_000 },
  ],
  stop_reason: null,
  created_unix: T,
  finished_unix: T + 29 * 60,
  lanes: [
    {
      lane: "correctness", engine: "claude", model: "sonnet-5-5", kind: "review", state: "done", tokens_used: 1_280_000,
      outcome: "scored", findings_n: 21, unresolved_n: 2,
      score: { recall: { numerator: 6, denominator: 9, permille: 666 }, precision: { numerator: 6, denominator: 21, permille: 285 }, escapes_n: 9, findings_n: 21, unmatched_n: 15 },
    },
    {
      lane: "correctness", engine: "codex", model: "gpt-5-5", kind: "review", state: "done", tokens_used: 896_000,
      outcome: "scored", findings_n: 17, unresolved_n: 0,
      score: { recall: { numerator: 5, denominator: 9, permille: 555 }, precision: { numerator: 5, denominator: 17, permille: 294 }, escapes_n: 9, findings_n: 17, unmatched_n: 12 },
    },
    {
      lane: "security", engine: "opencode", model: "deepseek-v4", kind: "review", state: "done", tokens_used: 396_000,
      outcome: "not_measured", findings_n: 9, unresolved_n: null, score: null,
    },
  ],
});

const stopped = (): BacktestRun => ({
  ...finished(),
  id: "run-stopped",
  state: "stopped",
  stop_reason: "stopped by the person",
  finished_unix: T + 18 * 60,
  lanes: finished().lanes.map((lane, index) => (index === 0 ? { ...lane, state: "stopped", tokens_used: 800_000, score: null, outcome: "unscored", unresolved_n: null } : lane)),
});

export function createBacktestMock() {
  let scenario: Scenario = "default";
  return {
    reset(next: Scenario) {
      scenario = next;
    },
    handle(method: string, pathname: string): MockResponse | null {
      if (method !== "GET" || !scenario.startsWith("crew-backtest")) return null;
      if (pathname === "/api/crew/backtest/estimate") return { status: 200, body: estimate() };
      const report = /^\/api\/crew\/backtest\/([^/]+)\/report$/.exec(pathname);
      if (!report) return null;
      const id = decodeURIComponent(report[1] ?? "");
      if (scenario === "crew-backtest-stopped" && id === "run-stopped") return { status: 200, body: stopped() };
      if (scenario === "crew-backtest-report" && id === "run-4n") return { status: 200, body: finished() };
      return { status: 404, body: { code: "not_found", message: "No such backtest." } };
    },
  };
}
