import { ApiFailure, request } from "../../../api/client";
import { serviceUrl } from "../../../api/origin";
import type { CrewReviewControlView } from "../../../generated/CrewReviewControlView";
import type { CrewLaneRemovalAnswer } from "../../../generated/CrewLaneRemovalAnswer";
import type { CrewAcpTierView } from "../../../generated/CrewAcpTierView";
import type { CrewAcpTierSave } from "../../../generated/CrewAcpTierSave";
import type { CrewAcpTierSettings } from "../../../generated/CrewAcpTierSettings";
import { isCrewTaskId } from "../../../product/state/route";

export const controlScope = () => new URL(serviceUrl("/api/crew/settings/engine-tiers"), window.location.href).origin;
export function assertControlScope(scope: string) {
  if (controlScope() !== scope) throw new ApiFailure("context_changed", "The connection changed. Reopen this view before continuing.", 0);
}
const path = (task: string) => {
  if (!isCrewTaskId(task)) throw new ApiFailure("invalid", "That task’s id is not valid.", 0);
  return `/api/crew/review/${encodeURIComponent(task)}`;
};
export const reviewControlApi = {
  lanes: (task: string, signal?: AbortSignal) => request<CrewReviewControlView>("GET", `${path(task)}/controls`, undefined, signal),
  remove: (task: string, lane: string, reason: string) => request<CrewLaneRemovalAnswer>("POST", `${path(task)}/remove-lane`, { lane, reason }),
  tiers: (signal?: AbortSignal) => request<CrewAcpTierView>("GET", "/api/crew/settings/engine-tiers", undefined, signal),
  saveTiers: (body: CrewAcpTierSave) => request<CrewAcpTierSettings>("POST", "/api/crew/settings/engine-tiers/save", body),
};
