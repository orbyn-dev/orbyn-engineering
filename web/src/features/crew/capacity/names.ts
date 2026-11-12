/** Frame ids of Crew Designs v21 built here. Each opens at `#/preview/crew/<id>`. */
export const CAPACITY_FRAMES = [
  "capacity-settings",
  "capacity-settings-unset",
  "capacity-over",
  "capacity-resumed",
  "capacity-paused-board",
  "capacity-paused-card",
  "capacity-sampling",
  "capacity-digest",
  "capacity-coverage",
  "capacity-coverage-area",
  "capacity-coverage-gaps",
  "capacity-coverage-empty",
  "capacity-coverage-low",
  "capacity-coverage-error",
  "capacity-narrow-panel",
  "capacity-narrow-window",
  "capacity-coverage-narrow-panel",
  "capacity-coverage-narrow-window",
] as const;

export type CapacityFrameId = (typeof CAPACITY_FRAMES)[number];
