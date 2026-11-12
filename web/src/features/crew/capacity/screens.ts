import type { ComponentType } from "react";
import { CapacitySettings } from "./settings";
import { CapacitySettingsUnset } from "./settings-unset";
import { CapacityOver } from "./over";
import { CapacityResumed } from "./resumed";
import { CapacityPausedBoard } from "./paused-board";
import { CapacityPausedCard } from "./paused-card";
import { CapacitySampling } from "./sampling";
import { CapacityDigest } from "./digest";
import { CapacityCoverage } from "./coverage";
import { CapacityCoverageArea } from "./coverage-area";
import { CapacityCoverageGaps } from "./coverage-gaps";
import { CapacityCoverageEmpty } from "./coverage-empty";
import { CapacityCoverageLow } from "./coverage-low";
import { CapacityCoverageError } from "./coverage-error";
import { CapacityNarrowPanel } from "./narrow-panel";
import { CapacityNarrowWindow } from "./narrow-window";
import { CapacityCoverageNarrowPanel } from "./coverage-narrow-panel";
import { CapacityCoverageNarrowWindow } from "./coverage-narrow-window";
import type { CapacityFrameId } from "./names";

/** Each frame of Crew Designs v21, drawn on sample data. Loaded only through the Preview route. */
export const CAPACITY_SCREENS: Record<CapacityFrameId, ComponentType> = {
  "capacity-settings": CapacitySettings,
  "capacity-settings-unset": CapacitySettingsUnset,
  "capacity-over": CapacityOver,
  "capacity-resumed": CapacityResumed,
  "capacity-paused-board": CapacityPausedBoard,
  "capacity-paused-card": CapacityPausedCard,
  "capacity-sampling": CapacitySampling,
  "capacity-digest": CapacityDigest,
  "capacity-coverage": CapacityCoverage,
  "capacity-coverage-area": CapacityCoverageArea,
  "capacity-coverage-gaps": CapacityCoverageGaps,
  "capacity-coverage-empty": CapacityCoverageEmpty,
  "capacity-coverage-low": CapacityCoverageLow,
  "capacity-coverage-error": CapacityCoverageError,
  "capacity-narrow-panel": CapacityNarrowPanel,
  "capacity-narrow-window": CapacityNarrowWindow,
  "capacity-coverage-narrow-panel": CapacityCoverageNarrowPanel,
  "capacity-coverage-narrow-window": CapacityCoverageNarrowWindow,
};
