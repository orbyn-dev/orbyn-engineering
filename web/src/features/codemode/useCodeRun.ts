import { useEffect } from "react";
import { create } from "zustand";
import { codeModeLive, getCodeRun } from "./api";
import type { CodeRunRecord } from "./types";

const POLL_MS = 2_000;

interface RunStore {
  records: Record<string, CodeRunRecord>;
  /** Runs whose record could not be read (gone after seven days, or not this project's). They are not asked again. */
  missing: Record<string, true>;
}

const useRuns = create<RunStore>(() => ({ records: {}, missing: {} }));
const inflight = new Map<string, Promise<void>>();

const settled = (record: CodeRunRecord | undefined) => record !== undefined && (record.status === "completed" || record.status === "error");

/** One read of a run record, shared by every block that shows the run. A failed read marks the run missing; it never throws. */
function refresh(runId: string): Promise<void> {
  const open = inflight.get(runId);
  if (open) return open;
  const read = getCodeRun(runId)
    .then((record) => useRuns.setState((state) => ({ records: { ...state.records, [runId]: record }, missing: { ...state.missing } })))
    .catch(() => useRuns.setState((state) => ({ missing: { ...state.missing, [runId]: true } })))
    .finally(() => inflight.delete(runId));
  inflight.set(runId, read);
  return read;
}

/** Forgets everything read, for tests. */
export function resetCodeRuns() {
  inflight.clear();
  useRuns.setState({ records: {}, missing: {} });
}

/**
 * The run record for a run id, read again every two seconds while the run is not over (or while `keepWatching`, for an
 * approval that waits on it). Null until it is read, and null for good when it cannot be: the caller draws from the engine's
 * reply then. In a production build nothing is read.
 */
export function useCodeRun(runId: string | null, keepWatching = false): CodeRunRecord | null {
  const record = useRuns((state) => (runId ? state.records[runId] : undefined));
  useEffect(() => {
    if (!runId || !codeModeLive) return;
    let stopped = false;
    const tick = () => {
      const now = useRuns.getState();
      if (stopped || now.missing[runId]) return;
      if (settled(now.records[runId]) && !keepWatching) return;
      void refresh(runId);
    };
    tick();
    const timer = setInterval(tick, POLL_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [runId, keepWatching]);
  return record ?? null;
}
