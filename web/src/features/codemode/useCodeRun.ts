import { useEffect } from "react";
import { create } from "zustand";
import { getCodeRun } from "./api";
import type { CodeRunRecord } from "./types";

const POLL_MS = 2_000;

interface RunStore {
  records: ReadonlyMap<string, CodeRunRecord>;
  /** Runs whose record could not be read (gone after seven days, not this project's, or Code Mode not offered). They are not asked again. */
  missing: ReadonlySet<string>;
}

const useRuns = create<RunStore>(() => ({ records: new Map(), missing: new Set() }));
const inflight = new Map<string, Promise<void>>();

const settled = (record: CodeRunRecord | undefined) => record !== undefined && (record.status === "completed" || record.status === "error");

/** One read of a run record, shared by every block that shows the run. (Maps and sets, not plain objects: a run id comes from a model and may be `__proto__`.) A failed read marks the run missing; it never throws. */
function refresh(runId: string): Promise<void> {
  const open = inflight.get(runId);
  if (open) return open;
  const read = getCodeRun(runId)
    .then((record) =>
      useRuns.setState((state) =>
        record ? { records: new Map(state.records).set(runId, record) } : { missing: new Set(state.missing).add(runId) },
      ),
    )
    .catch(() => useRuns.setState((state) => ({ missing: new Set(state.missing).add(runId) })))
    .finally(() => inflight.delete(runId));
  inflight.set(runId, read);
  return read;
}

/** Forgets everything read, for tests. */
export function resetCodeRuns() {
  inflight.clear();
  useRuns.setState({ records: new Map(), missing: new Set() });
}

/** Puts the record a write answered into the store, so every block showing the run draws it at once. */
export function applyCodeRun(record: CodeRunRecord): void {
  useRuns.setState((state) => {
    const records = new Map(state.records).set(record.run_id, record);
    if (!state.missing.has(record.run_id)) return { records };
    const missing = new Set(state.missing);
    missing.delete(record.run_id);
    return { records, missing };
  });
}

/** Reads a run again now, as the two-second poll does. A failed read marks it missing and never throws. */
export function refreshCodeRun(runId: string): Promise<void> {
  return refresh(runId);
}

/**
 * The run record for a run id, read again every two seconds while the run is not over (or while `keepWatching`, for an
 * approval that waits on it). Null until it is read, and null for good when it cannot be: the caller draws from the engine's
 * reply then (also when the service does not offer Code Mode: every route answers 404).
 */
export function useCodeRun(runId: string | null, keepWatching = false): CodeRunRecord | null {
  const record = useRuns((state) => (runId ? state.records.get(runId) : undefined));
  useEffect(() => {
    if (!runId) return;
    let stopped = false;
    const tick = () => {
      const now = useRuns.getState();
      if (stopped || now.missing.has(runId)) return;
      if (settled(now.records.get(runId)) && !keepWatching) return;
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
