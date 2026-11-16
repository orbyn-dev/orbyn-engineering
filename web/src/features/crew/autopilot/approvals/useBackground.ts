import { useAsync } from "../../../../lib/useAsync";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import { crewApi } from "./api";
import { RECENT_RUNS, type BackgroundRead } from "./backgroundModel";

const PAGE = 100;
/** The most pages of crews one read follows, so a very long list cannot hold the page open. */
const PAGES = 5;

/** The background crews of a repository (all of them when `project` is undefined), each with its newest runs. */
export function useBackground(project: string | undefined) {
  return useAsync<BackgroundRead>(
    `crew-background:${project ?? "all"}`,
    async (signal) => {
      const crews: BackgroundCrew[] = [];
      let next: number | null = 0;
      for (let count = 0; next !== null && count < PAGES; count += 1) {
        const page = await crewApi.backgroundCrews({ project, offset: next, limit: PAGE }, signal);
        crews.push(...page.crews);
        next = page.next_offset ?? null;
      }
      const runs = await Promise.all(crews.map(async (crew) => [crew.id, (await crewApi.backgroundRuns(crew.id, { offset: 0, limit: RECENT_RUNS }, signal)).runs] as const));
      return { crews, runs: Object.fromEntries(runs) };
    },
    { keep: true },
  );
}
