// Which Crew task made a pull request. No read of the service says so yet (docs/crew-run-page.md), so the production client
// reports none and the pull request page draws no Run tab. A client that can say is set once, at start. Never a match by
// branch name or title.
export interface RunLink {
  /** The Crew task whose Flow read the Run tab shows. */
  task: string;
}

export interface RunLinkClient {
  forPull(project: string, number: number, signal?: AbortSignal): Promise<RunLink | null>;
}

const none: RunLinkClient = { forPull: () => Promise.resolve(null) };

let client: RunLinkClient = none;

export const runLinkClient = (): RunLinkClient => client;
export const setRunLinkClient = (next: RunLinkClient | null): void => {
  client = next ?? none;
};
