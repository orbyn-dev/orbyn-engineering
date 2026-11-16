import type { ComponentType } from "react";
import "./flow.css";
import { FlowAll } from "./all";
import { FlowTask } from "./task";
import { FlowTaskLoop } from "./task-loop";
import { FlowCheckFailed } from "./check-failed";
import { FlowArtifactDiff } from "./artifact-diff";
import { FlowArtifacts } from "./artifacts";
import { FlowGates } from "./gates";
import { FlowReplay } from "./replay";
import { FlowRunning } from "./running";
import { FlowLanesMany } from "./lanes-many";
import { FlowLongLoop } from "./long-loop";
import { FlowSkipped } from "./skipped";
import { FlowOffline } from "./offline";
import { FlowPrReview } from "./pr-review";
import { FlowYouReady } from "./you-ready";
import { FlowYouRule } from "./you-rule";
import { FlowYouBlocked } from "./you-blocked";
import { FlowYouWaitingOn } from "./you-waiting-on";
import { FlowYouOff } from "./you-off";
import { FlowNarrowPanel } from "./narrow-panel";
import { FlowNarrowWindow } from "./narrow-window";
import { FlowArtifactPruned } from "./artifact-pruned";
import { FlowEmptyCrews } from "./empty-crews";
import { FlowEmptyTasks } from "./empty-tasks";
import { FlowEmptyFilter } from "./empty-filter";
import { FlowTruncated } from "./truncated";
import { FlowTask404 } from "./task-404";
import { FlowSourceError } from "./source-error";
import type { FlowFrameId } from "./names";

/** Each Flow frame of Crew Designs, drawn on sample data. Loaded only through the Preview route. */
export const FLOW_SCREENS: Record<FlowFrameId, ComponentType> = {
  "flow-all": FlowAll,
  "flow-task": FlowTask,
  "flow-task-loop": FlowTaskLoop,
  "flow-check-failed": FlowCheckFailed,
  "flow-artifact-diff": FlowArtifactDiff,
  "flow-artifacts": FlowArtifacts,
  "flow-gates": FlowGates,
  "flow-replay": FlowReplay,
  "flow-running": FlowRunning,
  "flow-lanes-many": FlowLanesMany,
  "flow-long-loop": FlowLongLoop,
  "flow-skipped": FlowSkipped,
  "flow-offline": FlowOffline,
  "flow-pr-review": FlowPrReview,
  "flow-you-ready": FlowYouReady,
  "flow-you-rule": FlowYouRule,
  "flow-you-blocked": FlowYouBlocked,
  "flow-you-waiting-on": FlowYouWaitingOn,
  "flow-you-off": FlowYouOff,
  "flow-narrow-panel": FlowNarrowPanel,
  "flow-narrow-window": FlowNarrowWindow,
  "flow-artifact-pruned": FlowArtifactPruned,
  "flow-empty-crews": FlowEmptyCrews,
  "flow-empty-tasks": FlowEmptyTasks,
  "flow-empty-filter": FlowEmptyFilter,
  "flow-truncated": FlowTruncated,
  "flow-task-404": FlowTask404,
  "flow-source-error": FlowSourceError,
};
