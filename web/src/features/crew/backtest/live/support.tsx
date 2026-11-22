import { vi } from "vitest";
import type { Project } from "../../../../api/types";
import type { BacktestEstimate } from "../../../../generated/BacktestEstimate";
import type { BacktestLaneRun } from "../../../../generated/BacktestLaneRun";
import type { BacktestRun } from "../../../../generated/BacktestRun";
import type { BacktestStartLane } from "../../../../generated/BacktestStartLane";
import type { BacktestStartRun } from "../../../../generated/BacktestStartRun";
import type { BacktestStartView } from "../../../../generated/BacktestStartView";

/** The project every backtest read names, as the mock's first project. */
export const project = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;

/** Sat 7 Nov 2026, 16:02 UTC: the day the design's sample run lives on. */
export const T = 1_793_923_200 + 16 * 3_600 + 2 * 60;

export const estimate = (over: Partial<BacktestEstimate> = {}): BacktestEstimate => ({
  project: "p-app",
  history_days: 180,
  outcome_days: 30,
  eligible_prs: 64,
  lanes: [],
  outcomes_measured: true,
  running: null,
  ...over,
});

export const lane = (over: Partial<BacktestLaneRun> = {}): BacktestLaneRun => ({
  lane: "correctness",
  engine: "claude",
  model: "sonnet-5-5",
  kind: "review",
  state: "done",
  tokens_used: 1_280_000,
  outcome: "scored",
  findings_n: 21,
  unresolved_n: 2,
  score: { recall: { numerator: 6, denominator: 9, permille: 666 }, precision: { numerator: 6, denominator: 21, permille: 285 }, escapes_n: 9, findings_n: 21, unmatched_n: 15 },
  ...over,
});

/** A lane the escape log cannot score: finding counts only, never a zero rate. */
export const notMeasuredLane = (over: Partial<BacktestLaneRun> = {}): BacktestLaneRun =>
  lane({ lane: "security", engine: "opencode", model: "deepseek-v4", tokens_used: 396_000, outcome: "not_measured", findings_n: 9, unresolved_n: null, score: null, ...over });

export const run = (over: Partial<BacktestRun> = {}): BacktestRun => ({
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
  lanes: [lane()],
  ...over,
});

export const startLane = (over: Partial<BacktestStartLane> = {}): BacktestStartLane => ({
  lane: "correctness",
  engine: "claude",
  model: "sonnet-5-5",
  kind: "review",
  ...over,
});

/** The run `start-view` reads back: the design's three review lanes and a budget for every engine. */
export const startRun = (over: Partial<BacktestStartRun> = {}): BacktestStartRun => ({
  id: "run-4n",
  state: "finished",
  lanes: [startLane(), startLane({ engine: "codex", model: "gpt-5-5" }), startLane({ lane: "security", engine: "opencode", model: "deepseek-v4" })],
  budgets: [
    { engine: "claude", tokens: 1_500_000 },
    { engine: "codex", tokens: 1_000_000 },
    { engine: "opencode", tokens: 500_000 },
  ],
  ...over,
});

export const startView = (over: Partial<BacktestStartView> = {}): BacktestStartView => ({ project: "p-app", run: startRun(), ...over });

export type AnswerBody = { status: number; body?: unknown };
export type Answer = AnswerBody | "never" | Promise<AnswerBody>;

const json = async (answer: Answer): Promise<Response> => {
  const settled = await answer;
  if (settled === "never") return new Promise<Response>(() => undefined);
  return new Response(settled.body === undefined ? null : JSON.stringify(settled.body), { status: settled.status });
};

/** The calls the screens made, with the request init, so a test can say which write was sent. */
export type Call = { url: string; init?: RequestInit };

/** One fetch stub for the backtest screens: answer each route from `answers` and record every call. */
export function stubFetch(
  answers: {
    preview: Answer;
    projects: Answer;
    estimate?: Answer;
    startView?: Answer;
    startViewById?: Record<string, Answer>;
    report?: Answer;
    reportById?: Record<string, Answer>;
  },
  calls: Call[],
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.startsWith("/api/projects")) return json(answers.projects);
      if (url.includes("/api/crew/preview")) return json(answers.preview);
      const view = /\/api\/crew\/backtest\/start-view\?project=([^&]+)/.exec(url);
      if (view) {
        const byProject = answers.startViewById?.[decodeURIComponent(view[1] ?? "")];
        return json(byProject ?? answers.startView ?? { status: 404, body: { code: "not_found", message: "No such route." } });
      }
      const report = /\/api\/crew\/backtest\/([^/?]+)\/report/.exec(url);
      if (report) {
        const byId = answers.reportById?.[decodeURIComponent(report[1] ?? "")];
        return json(byId ?? answers.report ?? { status: 404, body: { code: "not_found", message: "No such backtest." } });
      }
      if (url.includes("/api/crew/backtest/estimate")) return json(answers.estimate ?? { status: 404, body: { code: "not_found", message: "No such route." } });
      return json({ status: 404, body: { code: "not_found", message: "No such route." } });
    }),
  );
}

export const posts = (calls: readonly Call[]) => calls.filter(({ init }) => (init?.method ?? "GET") === "POST");
