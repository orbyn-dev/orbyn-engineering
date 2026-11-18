// The one door the Autopilot screens use: the Crew routes of `orbynd`, with the generated wire types.
// Every Crew route answers 404 while Crew Preview is off.
import { request } from "../../../../api/client";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundCrewCreate } from "../../../../generated/BackgroundCrewCreate";
import type { BackgroundCrewList } from "../../../../generated/BackgroundCrewList";
import type { BackgroundCrewUpdate } from "../../../../generated/BackgroundCrewUpdate";
import type { BackgroundRunList } from "../../../../generated/BackgroundRunList";
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
import type { AutopilotSecondEngineNotices } from "../../../../generated/AutopilotSecondEngineNotices";
import type { AutopilotMode } from "../../../../generated/AutopilotMode";
import type { AutopilotModes } from "../../../../generated/AutopilotModes";
import type { AutopilotTripwires } from "../../../../generated/AutopilotTripwires";
import type { AutopilotTrust } from "../../../../generated/AutopilotTrust";
import type { CrewClassesView } from "../../../../generated/CrewClassesView";
import type { CrewClassTest } from "../../../../generated/CrewClassTest";
import type { CrewPreviewState } from "../../../../generated/CrewPreviewState";
import type { CrewRouteView } from "../../../../generated/CrewRouteView";
import type { GuardTest } from "../../../../generated/GuardTest";
import type { GuardTargets } from "../../../../generated/GuardTargets";
import type { CrewWorkerProvider } from "../../../../generated/CrewWorkerProvider";
import type { CrewWorkerProviderView } from "../../../../generated/CrewWorkerProviderView";

export const PAGE = 18;

/** The one reason a control that needs a route not served yet is disabled. */
export const NOT_CONNECTED = "Crew is not connected to this app yet";

export const crewApi = {
  /** The project's stored Worker provider choice; `same_as_lead` when none was saved. */
  workerProvider: (project: string, signal?: AbortSignal) => request<CrewWorkerProviderView>("GET", `/api/crew/projects/${encodeURIComponent(project)}/worker-provider`, undefined, signal),
  /** A Person write: the client sends the person key on it. */
  setWorkerProvider: (project: string, worker_provider: CrewWorkerProvider) => request<CrewWorkerProviderView>("POST", `/api/crew/projects/${encodeURIComponent(project)}/worker-provider/set`, { worker_provider }),
  preview: (signal?: AbortSignal) => request<CrewPreviewState>("GET", "/api/crew/preview", undefined, signal),
  secondEngineNotices: (before: number | null, signal?: AbortSignal) => {
    const query = new URLSearchParams({ limit: "200" });
    if (before !== null) query.set("before", String(before));
    return request<AutopilotSecondEngineNotices>("GET", `/api/crew/approvals/second-engine?${query}`, undefined, signal);
  },
  dismissSecondEngineNotice: (id: number) => request<{ dismissed: boolean }>("POST", `/api/crew/approvals/second-engine/${id}/dismiss`, {}),
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
  /** This device's two lists of targets the guard watches. Read-only here: adding or removing is not drawn. */
  guardTargets: (signal?: AbortSignal) => request<GuardTargets>("GET", "/api/crew/guard/targets", undefined, signal),
  /**
   * The rules of `.orbyn/crew.json` for a project. The service takes `project` (a project id) as the alternative to `thread` once its change
   * merges; until then it answers 400 "thread is required" and the panel shows that error.
   */
  classes: (project: string, signal?: AbortSignal) => request<CrewClassesView>("GET", `/api/crew/classes?${new URLSearchParams({ project })}`, undefined, signal),
  /** 1 to 10 paths against those rules. Nothing is saved. */
  classesTest: (project: string, paths: string[], signal?: AbortSignal) => {
    const params = new URLSearchParams({ project });
    for (const path of paths) params.append("path", path);
    return request<CrewClassTest>("GET", `/api/crew/classes/test?${params}`, undefined, signal);
  },
  /** What the guard says about a command. Read-only: nothing runs. `thread` is optional on the service and not sent. */
  guardTest: (command: string, signal?: AbortSignal) => request<GuardTest>("GET", `/api/crew/guard/test?${new URLSearchParams({ command })}`, undefined, signal),
  /** The background crews, one page, ordered by name; `project` limits them to one repository. */
  backgroundCrews: (query: { project?: string; offset: number; limit: number }, signal?: AbortSignal) => {
    const params = new URLSearchParams({ offset: String(query.offset), limit: String(query.limit) });
    if (query.project) params.set("project", query.project);
    return request<BackgroundCrewList>("GET", `/api/crew/background?${params}`, undefined, signal);
  },
  /** One crew's runs with what each spent against its budget, newest first. */
  backgroundRuns: (id: string, query: { offset: number; limit: number }, signal?: AbortSignal) =>
    request<BackgroundRunList>("GET", `/api/crew/background/${encodeURIComponent(id)}/runs?${new URLSearchParams({ offset: String(query.offset), limit: String(query.limit) })}`, undefined, signal),
  /** Creates a crew, off: turning it on is its own step. A name already used in the project answers 409. */
  backgroundCreate: (body: BackgroundCrewCreate) => request<BackgroundCrew>("POST", "/api/crew/background/create", body),
  /** The name and the trigger only: the repository, template and budget are fixed at creation. No screen draws editing yet. */
  backgroundUpdate: (id: string, body: BackgroundCrewUpdate) => request<BackgroundCrew>("POST", `/api/crew/background/${encodeURIComponent(id)}/update`, body),
  /** The person's switch. Nothing is started: no trigger fires yet. */
  backgroundEnable: (id: string) => request<BackgroundCrew>("POST", `/api/crew/background/${encodeURIComponent(id)}/enable`, {}),
  /** Turning a crew off stops a run in progress with the reason `disabled`. */
  backgroundDisable: (id: string) => request<BackgroundCrew>("POST", `/api/crew/background/${encodeURIComponent(id)}/disable`, {}),
  /** Refused while a run is in progress; the crew's runs and waiting reviews go with it. No screen draws deleting yet. */
  backgroundDelete: (id: string) => request<{ deleted: boolean }>("POST", `/api/crew/background/${encodeURIComponent(id)}/delete`, {}),
  /** How one task was routed: the decision or the refusal, and the `route_class` rows. */
  route: (task: string, signal?: AbortSignal) => request<CrewRouteView>("GET", `/api/crew/route/${encodeURIComponent(task)}`, undefined, signal),
};
