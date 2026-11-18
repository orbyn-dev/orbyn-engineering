import { useEffect, useRef } from "react";
import { ApiFailure } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../../generated/BackgroundRun";
import { FLOW_REFRESH_MS } from "../../flow/live/useFlow";
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
  for (let at = 0; at < crews.length; at += AT_ONCE) {
    await Promise.all(crews.slice(at, at + AT_ONCE).map(async (crew) => { runs[crew.id] = await runsOf(crew, signal); }));
  }
  return { crews, total, runs };
}

/**
 * The background crews of one repository, by name, each with its newest runs (read four crews at a time). No service topic names a
 * background run, so while the window is shown the read repeats every `FLOW_REFRESH_MS` like the Flow lists. That read shows nothing
 * while it runs, never replaces the list with an error, never starts while another read is on its way, and stops with the screen.
 */
export function useBackground(project: string) {
  const read = useAsync<BackgroundRead>(`crew-background:${project}`, (signal) => readBackground(project, signal), { keep: true });
  const { setData, refreshing } = read;
  const busy = useRef(refreshing);
  useEffect(() => {
    busy.current = refreshing;
  });
  useEffect(() => {
    let inFlight: AbortController | null = null;
    const timer = setInterval(() => {
      if (document.visibilityState === "hidden" || inFlight || busy.current) return;
      const abort = new AbortController();
      inFlight = abort;
      readBackground(project, abort.signal).then(
        (data) => { if (inFlight === abort) setData(data); },
        () => undefined,
      ).finally(() => { if (inFlight === abort) inFlight = null; });
    }, FLOW_REFRESH_MS);
    return () => {
      clearInterval(timer);
      inFlight?.abort();
      inFlight = null;
    };
  }, [project, setData]);
  return read;
}
