import { useEffect, useRef } from "react";
import { ALL_DATA, onDataChanged } from "../api/shell";

/**
 * The service answers some reads from its last known value and sends `data_changed` when a
 * fresher one has landed (`api/shell.ts` lists the topics). One subscription, made once by the
 * app, hears every event. A screen registers what it shows with `useRefreshOn`, so only what is
 * mounted reads again; anything else reads when it opens. A burst of events is held for
 * `COALESCE_MS`, then each screen's read runs once however many topics named it.
 */
export const COALESCE_MS = 200;
export const EVERYTHING = ALL_DATA;

interface Refresher {
  matches: (what: string) => boolean;
  /** Gets the topics of the burst that named it, or `*` when the stream came back. */
  run: (heard: string[]) => unknown;
}

const refreshers = new Set<Refresher>();
let pending = new Set<string>();
let timer: ReturnType<typeof setTimeout> | undefined;

function flush() {
  timer = undefined;
  const topics = pending;
  pending = new Set();
  for (const refresher of [...refreshers]) {
    const heard = [...topics].filter((what) => what === EVERYTHING || refresher.matches(what));
    if (heard.length === 0) continue;
    try {
      void Promise.resolve(refresher.run(heard)).catch(() => undefined);
    } catch {
      // One screen failing to read again must not stop the others.
    }
  }
}

function hear(what: string) {
  pending.add(what);
  timer ??= setTimeout(flush, COALESCE_MS);
}

/** Starts hearing the service's change events. Returns a stop function. */
export function startDataRefresh(): () => void {
  const stop = onDataChanged(hear);
  return () => {
    stop();
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    pending = new Set();
  };
}

/** Registers a read to run again, once per burst, when an event names one of its topics. */
export function registerRefresh(matches: (what: string) => boolean, run: (heard: string[]) => unknown): () => void {
  const refresher = { matches, run };
  refreshers.add(refresher);
  return () => {
    refreshers.delete(refresher);
  };
}

/** The topics a screen shows: it matches exactly these names. */
export const topics =
  (...names: string[]) =>
  (what: string) =>
    names.includes(what);

/** A family of topics: `limits` and every `limits:<engine>`. */
export const under = (name: string) => (what: string) => what === name || what.startsWith(`${name}:`);

/** For a screen that shows one thing: reads again while mounted. */
export function useRefreshOn(matches: ((what: string) => boolean) | undefined, run: () => unknown) {
  const latest = useRef({ matches, run });
  useEffect(() => {
    latest.current = { matches, run };
  });
  const active = matches !== undefined;
  useEffect(() => {
    if (!active) return;
    return registerRefresh(
      (what) => latest.current.matches?.(what) ?? false,
      () => latest.current.run(),
    );
  }, [active]);
}
