import { lazy } from "react";
import { REVIEW_FRAMES, type ReviewFrameId } from "./autopilot/review/names";

/**
 * Crew design screens that have no engine route behind them yet: the Autopilot review frames. Nothing in the
 * app links to them; they open by hash (`#/preview/crew/<frame>`) and only when the build-time flag
 * `__ORBYN_PREVIEWS__` is true. In a production build the flag is false, so the lazy import is dead code and
 * the chunk is never emitted: with Crew's Preview flag off the screens do not exist.
 */
export function crewPreviewRoute(hash: string): ReviewFrameId | null {
  if (!__ORBYN_PREVIEWS__) return null;
  const match = /^#\/preview\/crew\/([a-z-]+)$/.exec(hash);
  return REVIEW_FRAMES.find((name) => name === match?.[1]) ?? null;
}

export const CrewPreview = __ORBYN_PREVIEWS__ ? lazy(() => import("./autopilot/review/PreviewScreens")) : null;
