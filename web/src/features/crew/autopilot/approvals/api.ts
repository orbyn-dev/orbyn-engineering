// The one door the Autopilot screens use: the Crew routes of `orbynd`, with the generated wire types.
// Every Crew route answers 404 while Crew Preview is off.
import { request } from "../../../../api/client";
import type { AutopilotGuards } from "../../../../generated/AutopilotGuards";
import type { AutopilotLog } from "../../../../generated/AutopilotLog";
import type { AutopilotMode } from "../../../../generated/AutopilotMode";
import type { AutopilotModes } from "../../../../generated/AutopilotModes";
import type { AutopilotTripwires } from "../../../../generated/AutopilotTripwires";
import type { AutopilotTrust } from "../../../../generated/AutopilotTrust";
import type { CrewPreviewState } from "../../../../generated/CrewPreviewState";

export const PAGE = 18;

/** The one reason a control that needs a route not served yet is disabled. */
export const NOT_CONNECTED = "Crew is not connected to this app yet";

export const crewApi = {
  preview: (signal?: AbortSignal) => request<CrewPreviewState>("GET", "/api/crew/preview", undefined, signal),
  /** The ledger, newest first. `kind` is the server's own filter; `task` limits it to one task. */
  approvals: (query: { kind?: string; task?: string; offset: number; limit?: number }, signal?: AbortSignal) => {
    const params = new URLSearchParams({ offset: String(query.offset), limit: String(query.limit ?? PAGE) });
    if (query.kind) params.set("kind", query.kind);
    if (query.task) params.set("task", query.task);
    return request<AutopilotLog>("GET", `/api/crew/approvals?${params}`, undefined, signal);
  },
  stops: (signal?: AbortSignal) => request<AutopilotTripwires>("GET", "/api/crew/stops", undefined, signal),
  modes: (signal?: AbortSignal) => request<AutopilotModes>("GET", "/api/crew/autopilot", undefined, signal),
  guards: (signal?: AbortSignal) => request<AutopilotGuards>("GET", "/api/crew/autopilot/guards", undefined, signal),
  setMode: (mode: AutopilotMode) => request<AutopilotModes>("PUT", "/api/crew/autopilot/mode", { scope: "default", mode }),
  confirmMode: () => request<AutopilotModes>("POST", "/api/crew/autopilot/confirm", {}),
  trust: (signal?: AbortSignal) => request<AutopilotTrust>("GET", "/api/crew/trust", undefined, signal),
};
