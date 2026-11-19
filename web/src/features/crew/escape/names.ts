/**
 * The escape frame ids built here, each opening at its Preview hash. The log with its states, the possible list, the
 * escape pages and the check frames are Crew Designs v79 on the real routes (./live/screens); the rest are still the
 * v20 drawn frames.
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
