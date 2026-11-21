import type { ComponentType } from "react";
import { LiveBacktest } from "./LiveBacktest";
import { backtestIdOfHash } from "./model";

/**
 * The backtest frames of Crew Designs v23 built on the real 4n routes. Each opens at '#/preview/crew/<id>'; a report frame
 * names its run in the hash's query ('?id=<run id>'), the way a detail frame names its escape. The estimate shows what a
 * backtest would replay and whether outcomes can be measured; the report and the stopped frames show a stored run, its
 * lanes' scores and every lane the escape log could not score as Not measured (never a zero).
 */
export const BACKTEST_LIVE_FRAMES = ["backtest-estimate", "backtest-report", "backtest-stopped"] as const;

export type BacktestLiveFrameId = (typeof BACKTEST_LIVE_FRAMES)[number];

export const BACKTEST_LIVE_SCREENS: Record<BacktestLiveFrameId, ComponentType> = {
  "backtest-estimate": () => <LiveBacktest view="estimate" />,
  "backtest-report": () => <LiveBacktest view="report" id={backtestIdOfHash()} />,
  "backtest-stopped": () => <LiveBacktest view="report" id={backtestIdOfHash()} />,
};
