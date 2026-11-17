import { ApiFailure } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../../generated/BackgroundRun";
import { crewApi } from "./api";
import { RECENT_RUNS, type BackgroundRead } from "./backgroundModel";

const PAGE = 100;
/** The most pages of crews one read follows, so a very long list cannot hold the page open. */
const PAGES = 5;
/** The most runs reads in flight at once. */
const AT_ONCE = 4;

/** A crew's newest runs. A crew deleted since the list was read answers 404 and has none. */
async function runsOf(crew: BackgroundCrew, signal: AbortSignal): Promise<BackgroundRun[]> {
  try {
    return (await crewApi.backgroundRuns(crew.id, { offset: 0, limit: RECENT_RUNS }, signal)).runs;
  } catch (failure) {
    if (failure instanceof ApiFailure && failure.status === 404) return [];
    throw failure;
  }
}

/** The background crews of one repository, by name, each with its newest runs (read four crews at a time). */
export function useBackground(project: string) {
  return useAsync<BackgroundRead>(
    `crew-background:${project}`,
    async (signal) => {
      const crews: BackgroundCrew[] = [];
      let total = 0;
      let next: number | null = 0;
      for (let count = 0; next !== null && count < PAGES; count += 1) {
        const page = await crewApi.backgroundCrews({ project, offset: next, limit: PAGE }, signal);
        crews.push(...page.crews);
        total = page.total;
        next = page.next_offset ?? null;
      }
      const runs: Record<string, BackgroundRun[]> = {};
      for (let at = 0; at < crews.length; at += AT_ONCE) {
        await Promise.all(crews.slice(at, at + AT_ONCE).map(async (crew) => { runs[crew.id] = await runsOf(crew, signal); }));
      }
      return { crews, total, runs };
    },
    { keep: true },
  );
}
