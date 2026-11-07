import { useCallback, useEffect, useRef, useState } from "react";
import { pullsApi } from "../../api/pulls";
import type { PullListEntry } from "../../api/types";
import { useWaited } from "../../lib/motion";
import { detailsKey, type DetailsByKey, type PullDetails } from "./pullModel";

/** The service answers at most this many rows in one details request. */
const CHUNK = 100;
/** How many of those requests run at once. */
const CONCURRENT = 3;

export type DetailsStatus = "idle" | "loading" | "ready" | "failed";

export interface PullDetailsState {
  status: DetailsStatus;
  /** Everything returned so far, kept while a later request runs or fails. */
  byKey: DetailsByKey;
  /** Fields the service withheld because GitHub's rate limit is spent. */
  paused: readonly string[];
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
  const fresh = useRef(false);
  const generation = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const rows = useRef(entries);
  rows.current = entries;

  const run = useCallback((own: boolean) => {
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
    setPaused([]);
    // The service answers at most 100 rows a request, so the rows go in chunks, the top of the list first.
    const chunks: PullListEntry[][] = [];
    for (let at = 0; at < list.length; at += CHUNK) chunks.push(list.slice(at, at + CHUNK));
    let failed = false;
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
          setPaused([...pausedFields]);
        } catch {
          if (!abort.signal.aborted) failed = true;
        }
      }
    };
    void Promise.all(Array.from({ length: Math.min(CONCURRENT, chunks.length) }, worker)).then(() => {
      if (id !== generation.current) return;
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

  return {
    status,
    byKey,
    paused,
    waited: useWaited(status === "loading"),
    retry: () => run(true),
    markFresh: () => {
      fresh.current = true;
    },
  };
}
