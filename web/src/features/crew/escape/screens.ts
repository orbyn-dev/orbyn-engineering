import type { ComponentType } from "react";
import { EscapeCreateCheck } from "./create-check";
import { EscapeCreateCheckTooling } from "./create-check-tooling";
import { EscapeReport } from "./report";
import { EscapeDashboard } from "./dashboard";
import { EscapeRouteCounts } from "./route-counts";
import { EscapeRouteShifted } from "./route-shifted";
import { ESCAPE_LIVE_FRAMES } from "./live/screens";
import type { EscapeFrameId } from "./names";

/**
 * The escape frames still on drawn sample data: the create-check proposal form is Crew Designs v20, and the mark dialog,
 * the dashboard and the counts cards were redrawn to v79. They stay drawn because no read carries their data: the mark is
 * reached from a Review queue the beta does not have, and the report and per-area counts routes are not built. The log and
 * its states, the possible list, the escape pages, the check frames, the settings sheet and the log's narrow variants are
 * on the real routes instead (./live/screens).
 */
type EscapeDrawnFrameId = Exclude<EscapeFrameId, (typeof ESCAPE_LIVE_FRAMES)[number]>;

export const ESCAPE_SCREENS: Partial<Record<EscapeDrawnFrameId, ComponentType>> = {
  "escape-create-check": EscapeCreateCheck,
  "escape-create-check-tooling": EscapeCreateCheckTooling,
  "escape-report": EscapeReport,
  "escape-dashboard": EscapeDashboard,
  "escape-route-counts": EscapeRouteCounts,
  "escape-route-shifted": EscapeRouteShifted,
};
