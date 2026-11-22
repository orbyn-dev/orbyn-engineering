import type { BacktestEstimate } from "../src/generated/BacktestEstimate.ts";
import type { BacktestRun } from "../src/generated/BacktestRun.ts";
import type { BacktestStartLane } from "../src/generated/BacktestStartLane.ts";
import type { BacktestStartRun } from "../src/generated/BacktestStartRun.ts";
import type { BacktestStartView } from "../src/generated/BacktestStartView.ts";
import type { MockResponse, Scenario } from "./api.ts";

/**
 * The reviewer backtest's mock (slice 4n): the estimate, the project's start input and the report reads with the wire
 * shapes, over the Crew Designs v23 sample run. Scenarios: "crew-backtest" (the estimate: 64 merged pull requests 30 days
 * old or more, outcomes measured, nothing running, and a finished last run), "crew-backtest-report" (the finished run
 * `run-4n`, two lanes scored and one Not measured), "crew-backtest-stopped" (the stopped run `run-stopped`, Partial),
 * "crew-backtest-kind-none" (a lane recorded before kinds were kept), "crew-backtest-budget-none" (three lanes, a budget
 * for two engines), "crew-backtest-none" (the project never ran a backtest), "crew-backtest-failed" (a failed last run)
 * and "crew-backtest-running" (a stored run in progress, with no live runner behind it). The mock sends no start or stop,
 * because no screen composes one.
 */

/** Sat 7 Nov 2026, 16:02 UTC: the day the design's sample run lives on. */
const T = 1_793_923_200 + 16 * 3_600 + 2 * 60;

const estimate = (running: string | null = null): BacktestEstimate => ({
  project: "p-app",
  history_days: 180,
  outcome_days: 30,
  eligible_prs: 64,
  lanes: [],
  outcomes_measured: true,
  running,
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

const startLanes = (): BacktestStartLane[] => [
  { lane: "correctness", engine: "claude", model: "sonnet-5-5", kind: "review" },
  { lane: "correctness", engine: "codex", model: "gpt-5-5", kind: "review" },
  { lane: "security", engine: "opencode", model: "deepseek-v4", kind: "review" },
];

const startBudgets = () => [
  { engine: "claude", tokens: 1_500_000 },
  { engine: "codex", tokens: 1_000_000 },
  { engine: "opencode", tokens: 500_000 },
];

const startRun = (over: Partial<BacktestStartRun> = {}): BacktestStartRun => ({
  id: "run-4n",
  state: "finished",
  lanes: startLanes(),
  budgets: startBudgets(),
  ...over,
});

const startView = (run: BacktestStartRun | null): BacktestStartView => ({ project: "p-app", run });

/** The start input each scenario serves: the last run's lanes, kinds and per-engine budgets, or no run at all. */
function startViewFor(scenario: Scenario): BacktestStartView {
  if (scenario === "crew-backtest-none") return startView(null);
  if (scenario === "crew-backtest-kind-none") return startView(startRun({ lanes: startLanes().map((lane, index) => (index === 2 ? { ...lane, kind: null } : lane)) }));
  if (scenario === "crew-backtest-budget-none") return startView(startRun({ budgets: startBudgets().slice(0, 2) }));
  if (scenario === "crew-backtest-failed") return startView(startRun({ state: "failed" }));
  if (scenario === "crew-backtest-stopped") return startView(startRun({ id: "run-stopped", state: "stopped" }));
  if (scenario === "crew-backtest-running") return startView(startRun({ id: "run-live", state: "running" }));
  return startView(startRun());
}

export function createBacktestMock() {
  let scenario: Scenario = "default";
  return {
    reset(next: Scenario) {
      scenario = next;
    },
    handle(method: string, pathname: string): MockResponse | null {
      if (method !== "GET" || !scenario.startsWith("crew-backtest")) return null;
      if (pathname === "/api/crew/backtest/estimate") return { status: 200, body: estimate(scenario === "crew-backtest-running" ? "run-live" : null) };
      if (pathname === "/api/crew/backtest/start-view") return { status: 200, body: startViewFor(scenario) };
      const report = /^\/api\/crew\/backtest\/([^/]+)\/report$/.exec(pathname);
      if (!report) return null;
      const id = decodeURIComponent(report[1] ?? "");
      if (scenario === "crew-backtest-stopped" && id === "run-stopped") return { status: 200, body: stopped() };
      if (scenario === "crew-backtest-report" && id === "run-4n") return { status: 200, body: finished() };
      return { status: 404, body: { code: "not_found", message: "No such backtest." } };
    },
  };
}
