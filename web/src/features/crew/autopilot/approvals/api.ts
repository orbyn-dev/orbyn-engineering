// The one door the Autopilot screens use. None of these routes exists in `orbynd` yet (the core has
// the ledger and the modes only), so on a real service they answer 404 and the screens stay hidden.
import { request } from "../../../../api/client";
import type {
  ApprovalFilter,
  ApprovalPage,
  AutopilotSettings,
  BackgroundView,
  CrewPreview,
  Mode,
  NewBackgroundCrew,
  TripwireStop,
  TrustView,
} from "./model";

export const PAGE = 18;

export const crewApi = {
  /** Whether Crew Preview is on. */
  preview: (signal?: AbortSignal) => request<CrewPreview>("GET", "/api/crew/preview", undefined, signal),
  approvals: (query: { filter: ApprovalFilter; q: string; offset: number }, signal?: AbortSignal) => {
    const params = new URLSearchParams({ filter: query.filter, offset: String(query.offset), limit: String(PAGE) });
    if (query.q.trim()) params.set("q", query.q.trim());
    return request<ApprovalPage>("GET", `/api/crew/approvals?${params}`, undefined, signal);
  },
  /** One guarded command answered by the person: this one command, once. */
  approveOnce: (id: string) => request<void>("POST", `/api/crew/approvals/${encodeURIComponent(id)}/approve-once`, {}),
  deny: (id: string) => request<void>("POST", `/api/crew/approvals/${encodeURIComponent(id)}/deny`, {}),
  dismiss: (id: string) => request<void>("POST", `/api/crew/approvals/${encodeURIComponent(id)}/dismiss`, {}),
  stops: (signal?: AbortSignal) => request<TripwireStop[]>("GET", "/api/crew/stops", undefined, signal),
  settings: (signal?: AbortSignal) => request<AutopilotSettings>("GET", "/api/crew/autopilot", undefined, signal),
  setMode: (mode: Mode, crew?: string) => request<void>("PUT", "/api/crew/autopilot/mode", { mode, crew: crew ?? null }),
  confirmMode: () => request<void>("POST", "/api/crew/autopilot/confirm", {}),
  trust: (signal?: AbortSignal) => request<TrustView>("GET", "/api/crew/trust", undefined, signal),
  background: (signal?: AbortSignal) => request<BackgroundView>("GET", "/api/crew/background", undefined, signal),
  setBackground: (id: string, enabled: boolean) =>
    request<void>("PUT", `/api/crew/background/${encodeURIComponent(id)}`, { enabled }),
  createBackground: (crew: NewBackgroundCrew) => request<void>("POST", "/api/crew/background", crew),
};
