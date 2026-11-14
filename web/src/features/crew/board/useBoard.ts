import { useCallback, useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../../api/client";
import type { FlowCard } from "../../../generated/FlowCard";
import type { FlowColumnId } from "../../../generated/FlowColumnId";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { boardApi, type BoardQuery } from "./api";

/** How often the open board reads again while the window is shown (reads only). */
export const BOARD_REFRESH_MS = 30_000;

const keyOf = (query: BoardQuery) => `${query.crew}|${query.engine ?? ""}|${query.kind ?? ""}`;

interface More {
  key: string;
  of: unknown;
  cards: Partial<Record<FlowColumnId, FlowCard[]>>;
  next: Partial<Record<FlowColumnId, number | null>>;
}

/**
 * One crew's board: the first read (the newest cards of each column), and the pages "+N more" added to a column. The extra cards
 * belong to the query and to the read they were added to, and are dropped when either changes. A column reads one page at a time:
 * a second call while one is on its way sends nothing.
 */
export function useBoard(query: BoardQuery) {
  const key = keyOf(query);
  const first = useAsync(`board:${key}`, (signal) => boardApi.board(query, signal));
  const [more, setMore] = useState<More | null>(null);
  const [busy, setBusy] = useState<Partial<Record<FlowColumnId, boolean>>>({});
  const [failure, setFailure] = useState<{ column: FlowColumnId; error: ApiFailure } | null>(null);
  const inFlight = useRef<Partial<Record<FlowColumnId, boolean>>>({});
  const board = first.data;
  const mine = more && more.key === key && more.of === board ? more : null;

  const cardsOf = (column: FlowColumnId): FlowCard[] => [...(board?.columns.find((one) => one.column === column)?.cards ?? []), ...(mine?.cards[column] ?? [])];
  const nextOf = (column: FlowColumnId): number | null => (mine && column in mine.next ? (mine.next[column] ?? null) : (board?.columns.find((one) => one.column === column)?.next_offset ?? null));

  const showMore = useCallback(
    async (column: FlowColumnId) => {
      const read = board?.columns.find((one) => one.column === column);
      const offset = mine && column in mine.next ? mine.next[column] : read?.next_offset;
      if (!board || offset == null || inFlight.current[column]) return;
      inFlight.current[column] = true;
      setBusy((now) => ({ ...now, [column]: true }));
      setFailure(null);
      try {
        const page = await boardApi.column(query, column, offset);
        const got = page.columns.find((one) => one.column === column);
        setMore((now) => {
          const same = now && now.key === key && now.of === board ? now : { key, of: board, cards: {}, next: {} };
          return { ...same, cards: { ...same.cards, [column]: [...(same.cards[column] ?? []), ...(got?.cards ?? [])] }, next: { ...same.next, [column]: got?.next_offset ?? null } };
        });
      } catch (error) {
        setFailure({ column, error: toFailure(error) });
      } finally {
        inFlight.current[column] = false;
        setBusy((now) => ({ ...now, [column]: false }));
      }
    },
    // `query` is rebuilt every render; its parts are in `key`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [board, mine, key],
  );

  const reload = first.reload;
  const paged = mine !== null;
  useEffect(() => {
    if (first.status !== "ready" || paged) return;
    const timer = setInterval(() => {
      if (document.visibilityState !== "hidden") void reload();
    }, BOARD_REFRESH_MS);
    return () => clearInterval(timer);
  }, [first.status, paged, reload]);

  return { first, board, cardsOf, nextOf, showMore, busy, failure };
}
