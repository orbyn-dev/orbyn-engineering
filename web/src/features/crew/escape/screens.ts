import type { ComponentType } from "react";
import { EscapeCreateCheck } from "./create-check";
import { EscapeCreateCheckTooling } from "./create-check-tooling";
import { EscapeReport } from "./report";
import { EscapeDashboard } from "./dashboard";
import { EscapeRouteCounts } from "./route-counts";
import { EscapeRouteShifted } from "./route-shifted";
import { EscapeSettings } from "./settings";
import { EscapeNarrowPanel } from "./narrow-panel";
import { EscapeNarrowWindow } from "./narrow-window";
import { ESCAPE_LIVE_FRAMES } from "./live/screens";
import type { EscapeFrameId } from "./names";

/**
 * The escape frames still on drawn sample data (Crew Designs v20): the proposal form, report and settings slices. The
 * log and its states, the possible list, the escape pages and the check frames are on the real routes instead
 * (./live/screens).
 */
type EscapeDrawnFrameId = Exclude<EscapeFrameId, (typeof ESCAPE_LIVE_FRAMES)[number]>;

export const ESCAPE_SCREENS: Partial<Record<EscapeDrawnFrameId, ComponentType>> = {
  "escape-create-check": EscapeCreateCheck,
  "escape-create-check-tooling": EscapeCreateCheckTooling,
  "escape-report": EscapeReport,
  "escape-dashboard": EscapeDashboard,
  "escape-route-counts": EscapeRouteCounts,
  "escape-route-shifted": EscapeRouteShifted,
  "escape-settings": EscapeSettings,
  "escape-narrow-panel": EscapeNarrowPanel,
  "escape-narrow-window": EscapeNarrowWindow,
};
