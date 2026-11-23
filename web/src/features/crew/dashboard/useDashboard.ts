import { toFailure, useAsync } from "../../../lib/useAsync";
import type { ApiFailure } from "../../../api/client";
import type { AutopilotAreaEvent } from "../../../generated/AutopilotAreaEvent";
import type { AutopilotAreas } from "../../../generated/AutopilotAreas";
import type { AutopilotThresholds } from "../../../generated/AutopilotThresholds";
import type { DashboardClient, Period } from "./contract";
import { fromHistory, fromTrust, type DashModel } from "./model";

/** At most this many repositories are read for All projects. */
export const MAX_REPOS = 24;
/** At most this many areas have their trail read for the changes of mode. */
export const MAX_TRAILS = 6;

/**
 * One repository's figures: the history read when the service has it, else the trust record, which holds less. A service without the
 * route answers `404` (so does Crew Preview off, which the page is behind anyway): the page then draws the trust record.
 */
export async function readModel(client: DashboardClient, project: string, days: Period, signal: AbortSignal): Promise<DashModel> {
  if (client.history) {
    try {
      return fromHistory(await client.history({ project, days }, signal), days);
    } catch (error) {
      if (signal.aborted || toFailure(error).status !== 404) throw error;
    }
  }
  return fromTrust(await client.trust(project, signal));
}

/** One repository. A newer request replaces an older one (the key names the repository and the period), so a slow answer never lands on the wrong page. */
export const useModel = (client: DashboardClient, project: string, days: Period) =>
  useAsync(`crew-dashboard:${project}:${days}`, (signal) => readModel(client, project, days, signal));

export type RepoRead = { project: string; model: DashModel } | { project: string; failure: ApiFailure };

/** Every listed repository, each read on its own: one that can't be read says so in its row and leaves the others. */
export const useEveryRepo = (client: DashboardClient, projects: readonly string[], days: Period) =>
  useAsync(`crew-dashboard-all:${projects.join(",")}:${days}`, async (signal): Promise<RepoRead[]> => {
    const rows = await Promise.all(
      projects.slice(0, MAX_REPOS).map((project) =>
        readModel(client, project, days, signal).then(
          (model): RepoRead => ({ project, model }),
          (error: unknown): RepoRead => {
            // A read that was cut short is not a failure to show: let the owner of the request see it.
            if (signal.aborted) throw error;
            return { project, failure: toFailure(error) };
          },
        ),
      ),
    );
    if (signal.aborted) throw new DOMException("Aborted", "AbortError");
    return rows;
  });

export interface AreaTrail {
  areas: AutopilotAreas;
  limits: AutopilotThresholds;
  events: Array<{ area: string; event: AutopilotAreaEvent }>;
}

/**
 * Autopilot by area for one repository: the areas, the thresholds, and the trail of the first few areas for the changes of mode. A trail that can't
 * be read leaves its area out of the list; the table does not depend on it.
 */
export const useAreaTrail = (client: DashboardClient, project: string) =>
  useAsync(`crew-dashboard-areas:${project}`, async (signal): Promise<AreaTrail> => {
    const [areas, limits] = await Promise.all([client.areas(project, signal), client.thresholds(signal)]);
    const wanted = (Array.isArray(areas.rows) ? areas.rows : []).slice(0, MAX_TRAILS);
    const trails = await Promise.all(
      wanted.map((row) =>
        client.areaHistory(project, row.area_key, signal).then(
          (reply) => (Array.isArray(reply.events) ? reply.events : []).map((event) => ({ area: row.area, event })),
          () => {
            if (signal.aborted) throw new DOMException("Aborted", "AbortError");
            return [];
          },
        ),
      ),
    );
    return { areas, limits, events: trails.flat() };
  });
