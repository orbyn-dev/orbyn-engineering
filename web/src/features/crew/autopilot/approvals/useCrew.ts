import { useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";

/**
 * Whether Crew Preview is on. Until the service says so, and whenever it cannot be asked (a service
 * with no Crew routes answers 404), the answer is off and nothing of Crew is drawn.
 */
export function useCrewPreview(): boolean {
  const read = useAsync("crew-preview", (signal) => crewApi.preview(signal), { keep: true });
  return read.status === "ready" && read.data.enabled;
}

export function useAutopilotSettings() {
  return useAsync("crew-autopilot", (signal) => crewApi.settings(signal), { keep: true });
}
