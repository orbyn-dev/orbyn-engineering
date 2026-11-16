// The Taskboard's one read: a crew's work tasks as seven columns. Reads only: Cancel and Re-run checks have routes, but this view
// does not call them (a later PR). Every Crew route answers 404 while Crew Preview is off.
import { request } from "../../../api/client";
import type { FlowBoard } from "../../../generated/FlowBoard";
import type { FlowColumnId } from "../../../generated/FlowColumnId";

/** The route's own cap on the cards one column page returns. */
export const COLUMN_PAGE = 50;

export interface BoardQuery {
  crew: string;
  engine: string | null;
  kind: string | null;
}

function params(query: BoardQuery): URLSearchParams {
  const found = new URLSearchParams({ crew: query.crew });
  if (query.engine) found.set("engine", query.engine);
  if (query.kind) found.set("kind", query.kind);
  return found;
}

export const boardPath = (query: BoardQuery): string => `/api/crew/flow/board?${params(query).toString()}`;

/** One column's next cards: the same filters, `column` set, paged by `limit` and `offset`. */
export function columnPath(query: BoardQuery, column: FlowColumnId, offset: number): string {
  const found = params(query);
  found.set("column", column);
  found.set("limit", String(COLUMN_PAGE));
  found.set("offset", String(offset));
  return `/api/crew/flow/board?${found.toString()}`;
}

export const boardApi = {
  board: (query: BoardQuery, signal?: AbortSignal): Promise<FlowBoard> => request<FlowBoard>("GET", boardPath(query), undefined, signal),
  column: (query: BoardQuery, column: FlowColumnId, offset: number, signal?: AbortSignal): Promise<FlowBoard> => request<FlowBoard>("GET", columnPath(query, column, offset), undefined, signal),
};
