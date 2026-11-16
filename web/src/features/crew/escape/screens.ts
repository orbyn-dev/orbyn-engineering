import type { ComponentType } from "react";
import { EscapeLog } from "./log";
import { EscapePossible } from "./possible";
import { EscapeCreateCheck } from "./create-check";
import { EscapeCreateCheckTooling } from "./create-check-tooling";
import { EscapeReport } from "./report";
import { EscapeDetail } from "./detail";
import { EscapeDetailTooling } from "./detail-tooling";
import { EscapeCheckInvalid } from "./check-invalid";
import { EscapeDashboard } from "./dashboard";
import { EscapeRouteCounts } from "./route-counts";
import { EscapeRouteShifted } from "./route-shifted";
import { EscapeSettings } from "./settings";
import { EscapeEmpty } from "./empty";
import { EscapeLowSample } from "./low-sample";
import { EscapeError } from "./error";
import { EscapeNarrowPanel } from "./narrow-panel";
import { EscapeNarrowWindow } from "./narrow-window";
import type { EscapeFrameId } from "./names";

/** Each frame of Crew Designs v20, drawn on sample data. Loaded only through the Preview route. */
export const ESCAPE_SCREENS: Record<EscapeFrameId, ComponentType> = {
  "escape-log": EscapeLog,
  "escape-possible": EscapePossible,
  "escape-create-check": EscapeCreateCheck,
  "escape-create-check-tooling": EscapeCreateCheckTooling,
  "escape-report": EscapeReport,
  "escape-detail": EscapeDetail,
  "escape-detail-tooling": EscapeDetailTooling,
  "escape-check-invalid": EscapeCheckInvalid,
  "escape-dashboard": EscapeDashboard,
  "escape-route-counts": EscapeRouteCounts,
  "escape-route-shifted": EscapeRouteShifted,
  "escape-settings": EscapeSettings,
  "escape-empty": EscapeEmpty,
  "escape-low-sample": EscapeLowSample,
  "escape-error": EscapeError,
  "escape-narrow-panel": EscapeNarrowPanel,
  "escape-narrow-window": EscapeNarrowWindow,
};
