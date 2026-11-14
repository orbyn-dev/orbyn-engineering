// The one door the Autopilot screens use: the Crew routes of `orbynd`, with the generated wire types.
// Every Crew route answers 404 while Crew Preview is off.
import { request } from "../../../../api/client";
import type { AutopilotHeld } from "../../../../generated/AutopilotHeld";
import type { AutopilotHeldList } from "../../../../generated/AutopilotHeldList";
import type { AutopilotAreaFollowBody } from "../../../../generated/AutopilotAreaFollowBody";
import type { AutopilotAreaHistory } from "../../../../generated/AutopilotAreaHistory";
import type { AutopilotAreaOverrideBody } from "../../../../generated/AutopilotAreaOverrideBody";
import type { AutopilotAreaRow } from "../../../../generated/AutopilotAreaRow";
import type { AutopilotAreas } from "../../../../generated/AutopilotAreas";
import type { AutopilotThresholds } from "../../../../generated/AutopilotThresholds";
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
  /** The commands the guard holds for the person, and the ones decided lately. */
  held: (signal?: AbortSignal) => request<AutopilotHeldList>("GET", "/api/crew/approvals/held", undefined, signal),
  /** Needs the nonce the desktop shell made after the person clicked Approve once in its native dialog, for exactly this hold, sent once in `x-orbyn-confirm`. */
  approveOnce: (id: string, nonce: string) =>
    request<AutopilotHeld>("POST", `/api/crew/approvals/${encodeURIComponent(id)}/approve-once`, {}, undefined, { headers: { "x-orbyn-confirm": nonce } }),
  /** No password: it only ever refuses. */
  denyHeld: (id: string) => request<AutopilotHeld>("POST", `/api/crew/approvals/${encodeURIComponent(id)}/deny`, {}),
  /** No password: it only ever hides. */
  dismissHeld: (id: string) => request<AutopilotHeld>("POST", `/api/crew/approvals/${encodeURIComponent(id)}/dismiss`, {}),
  stops: (signal?: AbortSignal) => request<AutopilotTripwires>("GET", "/api/crew/stops", undefined, signal),
  modes: (signal?: AbortSignal) => request<AutopilotModes>("GET", "/api/crew/autopilot", undefined, signal),
  guards: (signal?: AbortSignal) => request<AutopilotGuards>("GET", "/api/crew/autopilot/guards", undefined, signal),
  setMode: (mode: AutopilotMode) => request<AutopilotModes>("PUT", "/api/crew/autopilot/mode", { scope: "default", mode }),
  confirmMode: () => request<AutopilotModes>("POST", "/api/crew/autopilot/confirm", {}),
  /** The areas of one project under the circuit breaker (slice 4j), lowered ones first. */
  areas: (project: string, signal?: AbortSignal) => request<AutopilotAreas>("GET", `/api/crew/autopilot/areas?${new URLSearchParams({ project, limit: "200" })}`, undefined, signal),
  /** The trail of one area, newest first. `area` is the row's `area_key`: it contains `/`, so it travels in the query. */
  areaHistory: (project: string, area: string, signal?: AbortSignal) => request<AutopilotAreaHistory>("GET", `/api/crew/autopilot/areas/history?${new URLSearchParams({ project, area })}`, undefined, signal),
  /** The breaker's constants. Engineering's proposal: the reply says `proposed`. */
  thresholds: (signal?: AbortSignal) => request<AutopilotThresholds>("GET", "/api/crew/autopilot/thresholds", undefined, signal),
  /** A person's choice for one area, with the record they saw. A record that moved since answers 409 `stale`. */
  overrideArea: (body: AutopilotAreaOverrideBody) => request<AutopilotAreaRow>("POST", "/api/crew/autopilot/areas/override", body),
  followArea: (body: AutopilotAreaFollowBody) => request<AutopilotAreaRow>("POST", "/api/crew/autopilot/areas/follow", body),
  trust: (signal?: AbortSignal) => request<AutopilotTrust>("GET", "/api/crew/trust", undefined, signal),
};
