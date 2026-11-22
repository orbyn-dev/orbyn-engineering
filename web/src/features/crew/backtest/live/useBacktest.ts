import { useAsync } from "../../../../lib/useAsync";
import type { BacktestRun } from "../../../../generated/BacktestRun";
import { backtestApi } from "./api";

/** One project's backtest estimate. A read, never a write; a retry reads again. */
export const useBacktestEstimate = (project: string) =>
  useAsync(`crew-backtest-estimate:${project}`, (signal) => backtestApi.estimate(project, signal));

/** One project's start input: the most recent run's lanes, kinds and per-engine budgets, or a null run. A read, and a
 *  retry reads again; the hook's key changes with the project, so a slow older answer never lands on a newer project. */
export const useBacktestStartView = (project: string) =>
  useAsync(`crew-backtest-start-view:${project}`, (signal) => backtestApi.startView(project, signal));

/** One backtest run; null when the frame names none, which reads nothing. A read, never a write. */
export const useBacktestReport = (id: string) =>
  useAsync<BacktestRun | null>(`crew-backtest:${id}`, (signal) => (id ? backtestApi.report(id, signal) : Promise.resolve(null)));
