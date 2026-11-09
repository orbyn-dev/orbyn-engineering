import { useCallback, useEffect, useRef, useState } from "react";
import { pullsApi } from "../../../api/pulls";
import type { PullListEntry } from "../../../api/types";
import { useWaited } from "../../../lib/motion";
import { detailsKey, type DetailsByKey, type PullDetails } from "./pullModel";

/** The service answers at most this many rows in one details request. */
const CHUNK = 100;
/** How many of those requests run at once. */
const CONCURRENT = 3;
/** When the limit gives no reset time, it is checked again after this long. */
const FALLBACK_MS = 60_000;

export type DetailsStatus = "idle" | "loading" | "ready" | "failed";

export interface PullDetailsState {
  status: DetailsStatus;
  /** Everything returned so far, kept while a later request runs or fails. */
  byKey: DetailsByKey;
  /** Fields the service withheld because GitHub's rate limit is spent. */
  paused: readonly string[];
  /** When that pause ends, as the service knows it. */
  resumeAt: string | null;
  /** When the limit was still in force after a check at the reset time: the banner then says so. */
  stillLimited: boolean;
  /** When the next automatic check runs, in ms, while limited. */
  nextCheckAt: number | null;
  /** True once the request has run 400 ms: only then do the cells show their pending look. */
  waited: boolean;
  /** Asks again, as a person's own request. */
  retry(): void;
  /** Marks the next request as the person's own refresh, so it recounts instead of reusing. */
  markFresh(): void;
}

/**
 * The reviewers, review state, comment counts and opening times the base list leaves out,
 * asked for once the rows are known so the list paints first. A request is made per list
 * read; a failure leaves what was already known in place and says so.
 */
export function usePullDetails(entries: readonly PullListEntry[]): PullDetailsState {
  const [status, setStatus] = useState<DetailsStatus>("idle");
  const [byKey, setByKey] = useState<DetailsByKey>(new Map());
  const [paused, setPaused] = useState<readonly string[]>([]);
  const [resumeAt, setResumeAt] = useState<string | null>(null);
  const [stillLimited, setStillLimited] = useState(false);
  const [nextCheckAt, setNextCheckAt] = useState<number | null>(null);
  const fresh = useRef(false);
  const generation = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const rows = useRef(entries);
  rows.current = entries;

  /** `holdPause` is a check at the reset time: the pause on screen stays until the answer says otherwise. */
  const run = useCallback((own: boolean, holdPause = false) => {
    const list = rows.current;
    controller.current?.abort();
    const id = ++generation.current;
    if (list.length === 0) {
      setStatus("idle");
      return;
    }
    const abort = new AbortController();
    controller.current = abort;
    setStatus("loading");
    if (!holdPause) {
      setPaused([]);
      setResumeAt(null);
      setStillLimited(false);
      setNextCheckAt(null);
    }
    // The service answers at most 100 rows a request, so the rows go in chunks, the top of the list first.
    const chunks: PullListEntry[][] = [];
    for (let at = 0; at < list.length; at += CHUNK) chunks.push(list.slice(at, at + CHUNK));
    let failed = false;
    let resume: string | null = null;
    const pausedFields = new Set<string>();
    // At most a few requests at once, the top of the list first.
    let next = 0;
    const worker = async () => {
      while (next < chunks.length && !abort.signal.aborted) {
        const chunk = chunks[next++]!;
        try {
          const response = await pullsApi.details(
            { pulls: chunk.map(({ project_id, pull }) => ({ project_id, number: pull.number, updated_at: pull.updated_at })), fresh: own },
            abort.signal,
          );
          if (id !== generation.current) return;
          setByKey((known) => {
            const merged = new Map(known);
            for (const one of response.details) merged.set(detailsKey(one.project_id, one.number), one as PullDetails);
            return merged;
          });
          for (const field of response.paused) pausedFields.add(field);
          if (response.resume_at) resume = response.resume_at;
        } catch {
          if (!abort.signal.aborted) failed = true;
        }
      }
    };
    void Promise.all(Array.from({ length: Math.min(CONCURRENT, chunks.length) }, worker)).then(() => {
      if (id !== generation.current) return;
      const limited = pausedFields.size > 0;
      setPaused([...pausedFields]);
      if (limited) {
        // A check that finds the limit still on says so, and the time it gives is the next check.
        setStillLimited(holdPause);
        if (!holdPause) setResumeAt(resume);
        setNextCheckAt(resume ? new Date(resume).getTime() : Date.now() + FALLBACK_MS);
      } else {
        setResumeAt(null);
        setStillLimited(false);
        setNextCheckAt(null);
      }
      // A chunk that could not be read leaves its rows without details, which is a failure to say so about.
      setStatus(failed ? "failed" : "ready");
    });
  }, []);

  // One request per list read: a new list object is a new read.
  useEffect(() => {
    run(fresh.current);
    fresh.current = false;
  }, [entries, run]);
  useEffect(() => () => controller.current?.abort(), []);

  const retry = useCallback(() => run(true), [run]);
  const recheck = useCallback(() => run(false, true), [run]);
  const markFresh = useCallback(() => {
    fresh.current = true;
  }, []);
  // The next check is at a fixed time, not a delay that starts over with every render.
  useEffect(() => {
    if (nextCheckAt === null) return;
    const timer = setTimeout(recheck, Math.max(0, nextCheckAt - Date.now()) + 500);
    return () => clearTimeout(timer);
  }, [nextCheckAt, recheck]);

  return {
    status,
    byKey,
    paused,
    resumeAt,
    stillLimited,
    nextCheckAt,
    waited: useWaited(status === "loading"),
    retry,
    markFresh,
  };
}
