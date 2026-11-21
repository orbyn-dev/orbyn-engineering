// The reviewer backtest client (slice 4n): the estimate and the report are Local reads, so a load, a refresh and a retry
// may all send them. The start and the stop are Person-only and are never sent by a load, a refresh or a retry of a read:
// only a person's own click may call them, and no screen sends one yet (a start needs the lane list and the per-engine
// budgets, and no route serves them before a run exists; a start is also refused until a live replay engine is connected).
import { ApiFailure, request } from "../../../../api/client";
import type { BacktestEstimate } from "../../../../generated/BacktestEstimate";
import type { BacktestRun } from "../../../../generated/BacktestRun";
import type { BacktestStartInput } from "../../../../generated/BacktestStartInput";

/** The same shape the service's every crew id check accepts. */
const validRunId = (id: string): boolean => id.length <= 96 && /^[A-Za-z0-9:_-][A-Za-z0-9:_.-]*$/.test(id);

const query = (project: string) => `project=${encodeURIComponent(project)}`;
const path = (id: string): string => {
  if (!validRunId(id)) throw new ApiFailure("invalid", "That backtest’s id is not valid.", 0);
  return `/api/crew/backtest/${encodeURIComponent(id)}`;
};

export const backtestApi = {
  /** How big a backtest would be: the merges it may replay, the window, whether outcomes can be measured, and any run in
   *  progress. Reads only; nothing runs and nothing is stored to make it. */
  estimate: (project: string, signal?: AbortSignal) =>
    request<BacktestEstimate>("GET", `/api/crew/backtest/estimate?${query(project)}`, undefined, signal),
  /** One run with each lane's state, findings, tokens and score (or why it has none). Reads only. */
  report: (id: string, signal?: AbortSignal) => request<BacktestRun>("GET", `${path(id)}/report`, undefined, signal),
  /** A person starts a backtest. Their own click sends it; no screen can compose the body yet. */
  start: (input: BacktestStartInput) => request<BacktestRun>("POST", "/api/crew/backtest/start", input),
  /** A person asks a running backtest to stop. Their own click sends it; no screen draws it yet. */
  stop: (id: string) => request<BacktestRun>("POST", `${path(id)}/stop`),
};
