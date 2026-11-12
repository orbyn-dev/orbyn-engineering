import type { ComponentType } from "react";
import { SettingsCrew } from "./crew";
import type { SettingsFrameId } from "./names";

/** Each frame of Crew Designs v23, drawn on sample data. Loaded only through the Preview route. */
export const SETTINGS_SCREENS: Record<SettingsFrameId, ComponentType> = {
  "settings-crew": SettingsCrew,
};
