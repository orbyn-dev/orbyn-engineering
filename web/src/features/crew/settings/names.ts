/** Frame ids of Crew Designs v23 built here. Each opens at `#/preview/crew/<id>`. */
export const SETTINGS_FRAMES = [
  "settings-crew",
] as const;

export type SettingsFrameId = (typeof SETTINGS_FRAMES)[number];
