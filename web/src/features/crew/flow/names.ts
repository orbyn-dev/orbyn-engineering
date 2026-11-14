/** Frame ids of Crew Designs built by the Flow slice. Each opens at `#/preview/crew/<id>`. */
export const FLOW_FRAMES = [
  "flow-all",
  "flow-task",
  "flow-task-loop",
  "flow-check-failed",
  "flow-artifact-diff",
  "flow-artifacts",
  "flow-gates",
  "flow-replay",
  "flow-running",
  "flow-lanes-many",
  "flow-long-loop",
  "flow-skipped",
  "flow-offline",
  "flow-pr-review",
  "flow-you-ready",
  "flow-you-rule",
  "flow-you-blocked",
  "flow-you-waiting-on",
  "flow-you-off",
  "flow-narrow-panel",
  "flow-narrow-window",
  "flow-artifact-pruned",
] as const;

export type FlowFrameId = (typeof FLOW_FRAMES)[number];
