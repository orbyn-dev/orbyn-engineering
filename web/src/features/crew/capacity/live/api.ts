// The four Review capacity routes. The two reads are Local; the two writes are Person-only and are never sent by a load, a
// refresh or a retry of a read: only a person's own click may call them, and no screen has that control yet.
import { ApiFailure, request } from "../../../../api/client";
import type { CapacitySettingsInput } from "../../../../generated/CapacitySettingsInput";
import type { CapacityState } from "../../../../generated/CapacityState";
import type { CapacityTaskAnswer } from "../../../../generated/CapacityTaskAnswer";
import type { CapacityTaskInput } from "../../../../generated/CapacityTaskInput";
import type { ReviewCoverage } from "../../../../generated/ReviewCoverage";
import { validTaskId } from "../../writes/api";

const query = (project: string) => `project=${encodeURIComponent(project)}`;

export const capacityApi = {
  state: (project: string, signal?: AbortSignal) => request<CapacityState>("GET", `/api/crew/capacity?${query(project)}`, undefined, signal),
  /** `since` is left out: the map reads every merge since provenance began. */
  coverage: (project: string, signal?: AbortSignal) => request<ReviewCoverage>("GET", `/api/crew/capacity/coverage?${query(project)}`, undefined, signal),
  /** A person's own save of the limit, sampling and the pause switch (off until they turn it on). */
  saveSettings: (input: CapacitySettingsInput) => request<CapacityState>("POST", "/api/crew/capacity/settings", input),
  /** A person's own change of a task's priority or of who started it. */
  saveTask: (id: string, input: CapacityTaskInput): Promise<CapacityTaskAnswer> => {
    if (!validTaskId(id)) throw new ApiFailure("invalid", "That task’s id is not valid.", 0);
    return request<CapacityTaskAnswer>("POST", `/api/crew/capacity/tasks/${encodeURIComponent(id)}`, input);
  },
};
