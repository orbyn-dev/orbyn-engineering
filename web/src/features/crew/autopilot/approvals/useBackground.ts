import { useCallback, useRef } from "react";
import { ApiFailure } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../../generated/BackgroundRun";
import type { BackgroundRunDetail } from "../../../../generated/BackgroundRunDetail";
import { useRefresh } from "../../flow/live/useFlow";
import { crewApi } from "./api";
import { RECENT_RUNS, type BackgroundRead, type WatchReadRow } from "./backgroundModel";

const PAGE = 100;
/** The most pages of crews one read follows, so a very long list cannot hold the page open. */
const PAGES = 5;
/** The most runs and watch reads in flight at once. */
const AT_ONCE = 4;
/** The watch changes one crew read follows: the newest page, enough to name every change the design's screens draw. */
export const WATCH_CHANGES = 20;

/** A crew's newest runs. A crew deleted since the list was read answers 404 and has none. */
async function runsOf(crew: BackgroundCrew, signal: AbortSignal): Promise<BackgroundRun[]> {
  try {
    return (await crewApi.backgroundRuns(crew.id, { offset: 0, limit: RECENT_RUNS }, signal)).runs;
  } catch (failure) {
    if (failure instanceof ApiFailure && failure.status === 404) return [];
    throw failure;
  }
}

/**
 * A watch's two reads: its last check and its newest changes. A service without them answers 404 (an older build, or Crew
 * Preview off): that is not an error, it means the watch is not drawn at all, so it answers null. Any other failure is the
 * page's own error.
 */
async function watchOf(crew: BackgroundCrew, signal: AbortSignal): Promise<WatchReadRow | null> {
  if (crew.trigger.kind !== "watch") return null;
  try {
    const [status, page] = await Promise.all([
      crewApi.backgroundWatch(crew.id, signal),
      crewApi.backgroundWatchChanges(crew.id, { offset: 0, limit: WATCH_CHANGES }, signal),
    ]);
    return { status, changes: page.changes };
  } catch (failure) {
    if (failure instanceof ApiFailure && failure.status === 404) return null;
    throw failure;
  }
}

async function readBackground(project: string, signal: AbortSignal): Promise<BackgroundRead> {
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
  const watches: Record<string, WatchReadRow> = {};
  for (let at = 0; at < crews.length; at += AT_ONCE) {
    await Promise.all(
      crews.slice(at, at + AT_ONCE).map(async (crew) => {
        const [crewRuns, watch] = await Promise.all([runsOf(crew, signal), watchOf(crew, signal)]);
        runs[crew.id] = crewRuns;
        if (watch) watches[crew.id] = watch;
      }),
    );
  }
  return { crews, total, runs, watches };
}

/**
 * The background crews of one repository, by name, each with its newest runs (read four crews at a time). No service topic names a
 * background run, so while the window is shown the read repeats like the Flow lists. It takes `useAsync`'s quiet path: no loading
 * state, never an error over the list, behind a person's own read, and not started while an earlier one is on its way.
 */
export function useBackground(project: string) {
  const read = useAsync<BackgroundRead>(`crew-background:${project}`, (signal) => readBackground(project, signal), { keep: true });
  const { refreshQuietly } = read;
  const inFlight = useRef(false);
  const guarded = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      await refreshQuietly();
    } finally {
      inFlight.current = false;
    }
  }, [refreshQuietly]);
  useRefresh(guarded, read.status !== "ready");
  return read;
}

/**
 * One run with its crew, for the run's own page: the one read "Open the run" makes. A run that is not stored answers 404
 * `not_found`; the page draws nothing of it and the list stays.
 */
export function useBackgroundRun(runId: string) {
  return useAsync<BackgroundRunDetail>(`crew-background-run:${runId}`, (signal) => crewApi.backgroundRun(runId, signal));
}
