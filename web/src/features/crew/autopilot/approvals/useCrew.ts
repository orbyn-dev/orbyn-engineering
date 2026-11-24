import { useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";

/**
 * The Crew Preview read itself: whether it is still running, and what it said. A screen that reads other Crew routes
 * needs the same distinction the flag makes, so it can tell "off" from "not answered yet" (KC9's Skills page).
 */
export function useCrewPreviewRead() {
  return useAsync("crew-preview", (signal) => crewApi.preview(signal), { keep: true });
}

/**
 * Whether Crew Preview is on. A service with it off answers 404, and so does one with no Crew routes; both read
 * as off, and nothing of Crew is drawn.
 */
export function useCrewPreview(): boolean {
  const read = useCrewPreviewRead();
  return read.status === "ready" && read.data.enabled;
}

export const useAutopilotModes = () => useAsync("crew-autopilot", (signal) => crewApi.modes(signal), { keep: true });
export const useAutopilotGuards = () => useAsync("crew-guards", (signal) => crewApi.guards(signal), { keep: true });
export const useGuardTargets = () => useAsync("crew-guard-targets", (signal) => crewApi.guardTargets(signal));
