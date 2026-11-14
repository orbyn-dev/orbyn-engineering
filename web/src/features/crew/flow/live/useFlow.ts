import { useCallback, useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../../../api/client";
import type { FlowTaskRow } from "../../../../generated/FlowTaskRow";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { flowApi, type TaskQuery } from "./api";

/** How often the open list and task read again while the window is shown (reads only). */
export const FLOW_REFRESH_MS = 30_000;

export const useFlowCrews = () => useAsync("flow-crews", (signal) => flowApi.crews(signal));

export const useFlowTask = (id: string) => useAsync(`flow-task:${id}`, (signal) => flowApi.task(id, signal));

/** Reads again on a timer while the window is shown. Nothing is written. */
export function useRefresh(reload: () => Promise<void>, paused: boolean) {
  useEffect(() => {
    if (paused) return;
    const timer = setInterval(() => {
      if (document.visibilityState !== "hidden") void reload();
    }, FLOW_REFRESH_MS);
    return () => clearInterval(timer);
  }, [reload, paused]);
}

const keyOf = (query: Omit<TaskQuery, "offset">) => `${query.crew}|${query.filter}|${query.q}|${query.engine ?? ""}`;

/**
 * One crew's tasks for a filter, a search and an engine: the first page, and the pages "Show more" added. The extra pages
 * belong to the query they were read for and are dropped when it changes or when the first page is read again.
 */
export function useFlowTasks(query: Omit<TaskQuery, "offset">) {
  const key = keyOf(query);
  const first = useAsync(`flow-tasks:${key}`, (signal) => flowApi.tasks({ ...query, offset: 0 }, signal));
  const [more, setMore] = useState<{ key: string; of: unknown; rows: FlowTaskRow[]; next: number | null } | null>(null);
  const [busy, setBusy] = useState(false);
  // A second click while a page is on its way sends nothing: state is read after the next render, a ref at once.
  const inFlight = useRef(false);
  const [failure, setFailure] = useState<ApiFailure | null>(null);
  const page = first.data;
  const mine = more && more.key === key && more.of === page ? more : null;
  const next = mine ? mine.next : (page?.next_offset ?? null);
  const rows = [...(page?.tasks ?? []), ...(mine?.rows ?? [])];

  const showMore = useCallback(async () => {
    if (!page || next === null || inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setFailure(null);
    try {
      const read = await flowApi.tasks({ ...query, offset: next });
      setMore((now) => ({ key, of: page, rows: [...(now && now.key === key && now.of === page ? now.rows : []), ...read.tasks], next: read.next_offset ?? null }));
    } catch (error) {
      setFailure(toFailure(error));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
    // `query` is rebuilt every render; its parts are in `key`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page, next, key]);

  return { first, rows, next, showMore, busy, failure };
}
