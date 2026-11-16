import type { ComponentType } from "react";
import { BacktestSettingsUnset } from "./settings-unset";
import { BacktestSettings } from "./settings";
import { BacktestEstimate } from "./estimate";
import { BacktestOverBudget } from "./over-budget";
import { BacktestRunning } from "./running";
import { BacktestStopped } from "./stopped";
import { BacktestReport } from "./report";
import { BacktestVendors } from "./vendors";
import { BacktestEscapes } from "./escapes";
import { BacktestUnmatched } from "./unmatched";
import { BacktestLaneFailed } from "./lane-failed";
import { BacktestEmpty } from "./empty";
import { BacktestLowHistory } from "./low-history";
import { BacktestLanePicker } from "./lane-picker";
import { BacktestNarrowPanel } from "./narrow-panel";
import { BacktestNarrowWindow } from "./narrow-window";
import type { BacktestFrameId } from "./names";

/** Each frame of Crew Designs v23, drawn on sample data. Loaded only through the Preview route. */
export const BACKTEST_SCREENS: Record<BacktestFrameId, ComponentType> = {
  "backtest-settings-unset": BacktestSettingsUnset,
  "backtest-settings": BacktestSettings,
  "backtest-estimate": BacktestEstimate,
  "backtest-over-budget": BacktestOverBudget,
  "backtest-running": BacktestRunning,
  "backtest-stopped": BacktestStopped,
  "backtest-report": BacktestReport,
  "backtest-vendors": BacktestVendors,
  "backtest-escapes": BacktestEscapes,
  "backtest-unmatched": BacktestUnmatched,
  "backtest-lane-failed": BacktestLaneFailed,
  "backtest-empty": BacktestEmpty,
  "backtest-low-history": BacktestLowHistory,
  "backtest-lane-picker": BacktestLanePicker,
  "backtest-narrow-panel": BacktestNarrowPanel,
  "backtest-narrow-window": BacktestNarrowWindow,
};
