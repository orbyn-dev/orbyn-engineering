// The two Crew writes the beta has: cancel a task, and run its checks again. Each is the person's own click on a card or a task
// (never a load, a focus or a retry of a read), and has no body unless the person typed a reason. The answers carry counts and
// states only: no command, output or check text exists in them.
import { ApiFailure, request } from "../../../api/client";
import type { CrewTaskCancel } from "../../../generated/CrewTaskCancel";
import type { CrewTaskCancelled } from "../../../generated/CrewTaskCancelled";
import type { CrewTaskRerun } from "../../../generated/CrewTaskRerun";
import type { CrewTaskRerunDone } from "../../../generated/CrewTaskRerunDone";

/** The route's own limit on a reason: 1 to 200 characters. */
export const REASON_MAX = 200;

/** The body of a write: nothing when the person gave no reason, else `{ reason }`. */
export function bodyOf(reason: string): CrewTaskCancel | CrewTaskRerun | undefined {
  const text = reason.trim().slice(0, REASON_MAX);
  return text === "" ? undefined : { reason: text };
}

/**
 * A task id as the service takes it (its `valid_id`): 1 to 96 of letters, digits and : _ - . , and never a leading dot. Anything
 * else is refused here, before a request: `encodeURIComponent` leaves dots alone, so `..` would be resolved by the browser into a
 * different route.
 */
export const validTaskId = (id: string): boolean => id.length <= 96 && /^[A-Za-z0-9:_-][A-Za-z0-9:_.-]*$/.test(id);

function path(id: string, what: "cancel" | "rerun-checks"): string {
  if (!validTaskId(id)) throw new ApiFailure("invalid", "That task’s id is not valid.", 0);
  return `/api/crew/tasks/${encodeURIComponent(id)}/${what}`;
}

export const writesApi = {
  cancel: (id: string, reason: string): Promise<CrewTaskCancelled> => request<CrewTaskCancelled>("POST", path(id, "cancel"), bodyOf(reason)),
  /** Can take up to 30 minutes: nothing times it out, retries it or polls it. */
  rerun: (id: string, reason: string): Promise<CrewTaskRerunDone> => request<CrewTaskRerunDone>("POST", path(id, "rerun-checks"), bodyOf(reason)),
};
