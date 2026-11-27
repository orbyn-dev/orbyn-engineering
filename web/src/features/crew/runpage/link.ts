// Which Crew task made a pull request: the service's own run link read (`docs/crew-run-page.md`). The task's forge intent
// is the only thing that says so, so a branch name or a title never links. A client that can say is set once, at start; the
// service's own is the one installed, and a test swaps in its own.
import { ApiFailure } from "../../../api/client";
import { useConnection } from "../../../product/state/connection";
import { readRunLink } from "../flow/live/api";

export interface RunLink {
  /** The Crew task whose Flow read the Run tab shows. */
  task: string;
}

export interface RunLinkClient {
  forPull(project: string, number: number, signal?: AbortSignal): Promise<RunLink | null>;
}

/**
 * The service's answer for one pull request. It asks only while Crew Preview is on: with the flag off every Crew route
 * answers 404, which the browser logs as an error (`useCrewFlag`). With the flag on, a 404 means no Crew task made this
 * pull request, or a service older than the read, and both are the same answer here: no Run tab.
 */
const service: RunLinkClient = {
  forPull: async (project, number, signal) => {
    if (!useConnection.getState().crewPreview) return null;
    try {
      const link = await readRunLink(project, number, signal);
      return { task: link.task };
    } catch (error) {
      if (error instanceof ApiFailure && error.status === 404) return null;
      throw error;
    }
  },
};

let client: RunLinkClient = service;

export const runLinkClient = (): RunLinkClient => client;
/** Sets the client for a test; `null` puts the service's own answer back. */
export const setRunLinkClient = (next: RunLinkClient | null): void => {
  client = next ?? service;
};
