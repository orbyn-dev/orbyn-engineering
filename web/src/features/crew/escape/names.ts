/**
 * The escape frame ids built here, each opening at its Preview hash. The log with its states and narrow variants, the
 * possible list, the escape pages, the check frames and the settings sheet are Crew Designs v79 on the real routes
 * (./live/screens). The rest are drawn frames with no read behind them: the mark dialog, the dashboard and the counts
 * cards are v79, the create-check proposal form is v20.
 */
export const ESCAPE_FRAMES = [
  "escape-log",
  "escape-possible",
  "escape-confirmed",
  "escape-dismissed",
  "escape-gap",
  "escape-create-check",
  "escape-create-check-tooling",
  "escape-report",
  "escape-detail",
  "escape-detail-tooling",
  "escape-check-proposed",
  "escape-check-writing",
  "escape-check-approve",
  "escape-check-proven",
  "escape-check-invalid",
  "escape-check-retry",
  "escape-check-withdraw",
  "escape-check-withdrawn",
  "escape-limit",
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
