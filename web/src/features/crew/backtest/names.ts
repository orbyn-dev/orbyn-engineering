/** Frame ids of Crew Designs v23 built here. Each opens at `#/preview/crew/<id>`. */
export const BACKTEST_FRAMES = [
  "backtest-settings-unset",
  "backtest-settings",
  "backtest-estimate",
  "backtest-over-budget",
  "backtest-running",
  "backtest-stopped",
  "backtest-report",
  "backtest-vendors",
  "backtest-escapes",
  "backtest-unmatched",
  "backtest-lane-failed",
  "backtest-empty",
  "backtest-low-history",
  "backtest-lane-picker",
  "backtest-narrow-panel",
  "backtest-narrow-window",
] as const;

export type BacktestFrameId = (typeof BACKTEST_FRAMES)[number];
