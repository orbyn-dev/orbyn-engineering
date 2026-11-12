import { useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";

/**
 * Whether Crew Preview is on. A service with it off answers 404, and so does one with no Crew routes; both read
 * as off, and nothing of Crew is drawn.
 */
export function useCrewPreview(): boolean {
  const read = useAsync("crew-preview", (signal) => crewApi.preview(signal), { keep: true });
  return read.status === "ready" && read.data.enabled;
}

export const useAutopilotModes = () => useAsync("crew-autopilot", (signal) => crewApi.modes(signal), { keep: true });
export const useAutopilotGuards = () => useAsync("crew-guards", (signal) => crewApi.guards(signal), { keep: true });
