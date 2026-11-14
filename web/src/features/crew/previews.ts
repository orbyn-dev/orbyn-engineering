import { lazy } from "react";
import { REVIEW_FRAMES } from "./autopilot/review/names";
import { BACKTEST_FRAMES } from "./backtest/names";
import { CAPACITY_FRAMES } from "./capacity/names";
import { ESCAPE_FRAMES } from "./escape/names";
import { FLOW_FRAMES } from "./flow/names";
import { NEEDSYOU_FRAMES } from "./needsyou/names";
import { ORACLE_FRAMES } from "./oracle/names";
import { SETTINGS_FRAMES } from "./settings/names";

/**
 * Crew design screens that have no engine route behind them yet: the Autopilot review frames and the escape,
 * review capacity, oracle, backtest and settings frames. Nothing in the app links to them; they open by hash
 * (`#/preview/crew/<frame>`) and only when the build-time flag `__ORBYN_PREVIEWS__` is true. In a production build
 * the flag is false, so the lazy import is dead code and the chunk is never emitted: with Crew's Preview flag off
 * the screens do not exist.
 */
export function crewPreviewRoute(hash: string): string | null {
  if (!__ORBYN_PREVIEWS__) return null;
  const match = /^#\/preview\/crew\/([a-z-]+)$/.exec(hash);
  // Built here, not at module level, so a production build drops every frame id with this branch.
  const frames: readonly string[] = [...REVIEW_FRAMES, ...ESCAPE_FRAMES, ...CAPACITY_FRAMES, ...ORACLE_FRAMES, ...BACKTEST_FRAMES, ...SETTINGS_FRAMES, ...FLOW_FRAMES];
  return [...frames, ...NEEDSYOU_FRAMES].find((name) => name === match?.[1]) ?? null;
}

export const CrewPreview = __ORBYN_PREVIEWS__ ? lazy(() => import("./PreviewScreens")) : null;
