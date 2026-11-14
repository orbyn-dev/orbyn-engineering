// The three reads Flow has: the crews that have work, a crew's tasks with the ten cells of each, and one task zoomed.
// Reads only: no write path is added here (Cancel and Re-run checks are a later PR). Every Crew route answers 404 while
// Crew Preview is off.
import { request } from "../../../../api/client";
import type { FlowCrewList } from "../../../../generated/FlowCrewList";
import type { FlowCrewRow } from "../../../../generated/FlowCrewRow";
import type { FlowFilter } from "../../../../generated/FlowFilter";
import type { FlowTaskDetail } from "../../../../generated/FlowTaskDetail";
import type { FlowTaskList } from "../../../../generated/FlowTaskList";

/** The route's own cap on one page. */
export const CREW_PAGE = 200;
export const TASK_PAGE = 50;
/** The most crew pages one read follows. */
const CREW_PAGES = 5;

const get = <T,>(path: string, signal?: AbortSignal) => request<T>("GET", path, undefined, signal);

export interface TaskQuery {
  crew: string;
  filter: FlowFilter;
  q: string;
  engine: string | null;
  offset: number;
}

/** The query string of a task read. A filter of `all`, an empty search and no engine are left out. */
export function tasksPath(query: TaskQuery): string {
  const params = new URLSearchParams({ crew: query.crew, limit: String(TASK_PAGE) });
  if (query.filter !== "all") params.set("filter", query.filter);
  if (query.q.trim() !== "") params.set("q", query.q.trim());
  if (query.engine) params.set("engine", query.engine);
  if (query.offset > 0) params.set("offset", String(query.offset));
  return `/api/crew/flow/tasks?${params.toString()}`;
}

export const flowApi = {
  /** The crews that have work, newest first, following `next_offset` for a few pages. */
  async crews(signal?: AbortSignal): Promise<FlowCrewRow[]> {
    const all: FlowCrewRow[] = [];
    let next: number | null = 0;
    for (let count = 0; next !== null && count < CREW_PAGES; count += 1) {
      const page: FlowCrewList = await get<FlowCrewList>(`/api/crew/flow/crews?limit=${CREW_PAGE}&offset=${next}`, signal);
      all.push(...page.crews);
      next = page.next_offset ?? null;
    }
    return all;
  },
  tasks: (query: TaskQuery, signal?: AbortSignal): Promise<FlowTaskList> => get<FlowTaskList>(tasksPath(query), signal),
  task: (id: string, signal?: AbortSignal): Promise<FlowTaskDetail> => get<FlowTaskDetail>(`/api/crew/flow/tasks/${encodeURIComponent(id)}`, signal),
};
