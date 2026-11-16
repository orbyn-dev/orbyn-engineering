/** Frame ids of Crew Designs v20 built here. Each opens at `#/preview/crew/<id>`. */
export const ESCAPE_FRAMES = [
  "escape-log",
  "escape-possible",
  "escape-create-check",
  "escape-create-check-tooling",
  "escape-report",
  "escape-detail",
  "escape-detail-tooling",
  "escape-check-invalid",
  "escape-dashboard",
  "escape-route-counts",
  "escape-route-shifted",
  "escape-settings",
  "escape-empty",
  "escape-low-sample",
  "escape-error",
  "escape-narrow-panel",
  "escape-narrow-window",
] as const;

export type EscapeFrameId = (typeof ESCAPE_FRAMES)[number];
