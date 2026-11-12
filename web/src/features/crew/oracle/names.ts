/** Frame ids of Crew Designs v22 built here. Each opens at `#/preview/crew/<id>`. */
export const ORACLE_FRAMES = [
  "oracle-settings",
  "oracle-languages",
  "oracle-report",
  "oracle-key",
  "oracle-gate-running",
  "oracle-gate-survivors",
  "oracle-gate-failed",
  "oracle-strengthen",
  "oracle-needs-you",
  "oracle-no-second-engine",
  "oracle-not-measured",
  "oracle-tool-missing",
  "oracle-tool-error",
  "oracle-dashboard",
  "oracle-empty",
  "oracle-low-sample",
  "oracle-narrow-panel",
  "oracle-narrow-window",
] as const;

export type OracleFrameId = (typeof ORACLE_FRAMES)[number];
