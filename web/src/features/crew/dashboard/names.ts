/** Ids of the Crew dashboard frames (Crew Designs, "23. Dashboard"). Each opens at `#/preview/crew/<id>`. */
export const DASHBOARD_FRAMES = [
  "dash-default",
  "dash-metric",
  "dash-breakdown",
  "dash-low-sample",
  "dash-empty",
  "dash-many-repos",
  "dash-filters",
  "dash-loading",
  "dash-error",
  "dash-narrow-panel",
  "dash-narrow-window",
  "dash-area-history",
] as const;
export type DashboardFrameId = (typeof DASHBOARD_FRAMES)[number];
