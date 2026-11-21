import { useAsync } from "../../../../lib/useAsync";
import type { BacktestRun } from "../../../../generated/BacktestRun";
import { backtestApi } from "./api";

/** One project's backtest estimate. A read, never a write; a retry reads again. */
export const useBacktestEstimate = (project: string) =>
  useAsync(`crew-backtest-estimate:${project}`, (signal) => backtestApi.estimate(project, signal));

/** One backtest run; null when the frame names none, which reads nothing. A read, never a write. */
export const useBacktestReport = (id: string) =>
  useAsync<BacktestRun | null>(`crew-backtest:${id}`, (signal) => (id ? backtestApi.report(id, signal) : Promise.resolve(null)));
