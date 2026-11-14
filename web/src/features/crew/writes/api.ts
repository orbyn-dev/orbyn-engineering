// The two Crew writes the beta has: cancel a task, and run its checks again. Each is the person's own click on a card or a task
// (never a load, a focus or a retry of a read), and has no body unless the person typed a reason. The answers carry counts and
// states only: no command, output or check text exists in them.
import { request } from "../../../api/client";
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

const path = (id: string, what: "cancel" | "rerun-checks") => `/api/crew/tasks/${encodeURIComponent(id)}/${what}`;

export const writesApi = {
  cancel: (id: string, reason: string): Promise<CrewTaskCancelled> => request<CrewTaskCancelled>("POST", path(id, "cancel"), bodyOf(reason)),
  /** Can take up to 30 minutes: nothing times it out, retries it or polls it. */
  rerun: (id: string, reason: string): Promise<CrewTaskRerunDone> => request<CrewTaskRerunDone>("POST", path(id, "rerun-checks"), bodyOf(reason)),
};
