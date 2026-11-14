// The reads the inbox adds to the Autopilot client: the after-merge Review queue and the background crews with their runs.
// Held commands and stops come from `approvals/api.ts`. Reads only: no write path is added here, and every answer goes through
// the guards that already exist. Every Crew route answers 404 while Crew Preview is off.
import { request } from "../../../api/client";
import type { AutopilotReviewItem } from "../../../generated/AutopilotReviewItem";
import type { AutopilotReviewQueue } from "../../../generated/AutopilotReviewQueue";
import type { BackgroundCrew } from "../../../generated/BackgroundCrew";
import type { BackgroundCrewList } from "../../../generated/BackgroundCrewList";
import type { BackgroundRun } from "../../../generated/BackgroundRun";
import type { BackgroundRunList } from "../../../generated/BackgroundRunList";

const PAGE = 200;
/** The most pages one read follows, so a very long queue cannot hold the inbox open. */
const PAGES = 5;

const get = <T,>(path: string, signal?: AbortSignal) => request<T>("GET", path, undefined, signal);

/** Reads a paged route until it ends or `PAGES` pages are read. */
async function pages<T extends { next_offset?: number | null }, Item>(path: (offset: number) => string, items: (page: T) => Item[], signal?: AbortSignal): Promise<Item[]> {
  const all: Item[] = [];
  let next: number | null = 0;
  for (let count = 0; next !== null && count < PAGES; count += 1) {
    const page: T = await get<T>(path(next), signal);
    all.push(...items(page));
    next = page.next_offset ?? null;
  }
  return all;
}

export const needsApi = {
  /** The after-merge Review queue. */
  reviewQueue: (signal?: AbortSignal): Promise<AutopilotReviewItem[]> =>
    pages<AutopilotReviewQueue, AutopilotReviewItem>((offset) => `/api/crew/autopilot/review?limit=${PAGE}&offset=${offset}`, (page) => page.items, signal),
  /** The background crews that have a run waiting for review, each with its runs. */
  async backgroundWaiting(signal?: AbortSignal): Promise<{ crew: BackgroundCrew; runs: BackgroundRun[] }[]> {
    const crews = await pages<BackgroundCrewList, BackgroundCrew>((offset) => `/api/crew/background?limit=${PAGE}&offset=${offset}`, (page) => page.crews, signal);
    return Promise.all(
      crews
        .filter((crew) => crew.reviews_waiting > 0)
        .map(async (crew) => ({
          crew,
          runs: await pages<BackgroundRunList, BackgroundRun>((offset) => `/api/crew/background/${encodeURIComponent(crew.id)}/runs?limit=${PAGE}&offset=${offset}`, (page) => page.runs, signal),
        })),
    );
  },
};
