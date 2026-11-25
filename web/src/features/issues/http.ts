import { ApiFailure, request } from "../../api/client";
import type {
  IssueDetail, IssueList, IssueQuery, IssueRef, IssueTarget, IssuesApi, ThreadDraft, TrackerKind, TrackerRow,
} from "./contract";
import { commentProblem, readCandidates, readComment, readDetail, routeAbsent, updateProblem } from "./edit";

/**
 * The HTTP client behind `IssuesApi`: one function per route in docs/issues.md, through the app's own `request`, which adds the
 * bearer token and, for a Person-class route, the person key (`PENDING_PERSON_ROUTES` in `web/src/api/personKey.ts`).
 *
 * - Every value in a path is encoded as ONE segment, and every issue is named in the query, never in the path.
 * - A token goes only in the JSON body of the connect route: never in a path, a query, a header or a log. This module keeps no
 *   copy of it and puts none in an error.
 * - A `404` from `GET /api/issues/trackers` means the Issues flag is off: that read answers `null`, and the caller hides Issues.
 *
 * The Issue writes (`update`, `state`) and `candidates` are specified in docs/issues.md and not built in the service yet: the
 * client calls them only when the service has said it offers them (`offers`, and `IssueDetail.states` for a state change), and a
 * 404 or 405 from one of them is "not offered", never a success. None of this has run against a real service.
 */

const segment = encodeURIComponent;

const asArray = <T,>(value: unknown): T[] => (Array.isArray(value) ? (value as T[]) : []);

function target(issue: IssueTarget): URLSearchParams {
  return new URLSearchParams({ tracker: issue.tracker, scope: issue.scope, key: issue.key });
}

/** The body the service reads for a link or a start: the three fields that name the issue, and nothing else of ours. */
const named = (issue: IssueTarget) => ({ tracker: issue.tracker, scope: issue.scope, key: issue.key });

/** The most the `cursors` query value may hold once encoded (docs/issues.md, "Paging"). */
export const CURSORS_LIMIT = 4096;

/** `cursors` as the service reads it: the object as JSON, then URL-encoded. `null` when there is nothing to send; a value over the limit is refused here, not sent. */
function cursorsValue(cursors: Record<string, string> | undefined): string | null {
  if (!cursors || Object.keys(cursors).length === 0) return null;
  const value = encodeURIComponent(JSON.stringify(cursors));
  if (value.length > CURSORS_LIMIT) throw new Error("That is too many pages to ask for at once. Refresh the list instead.");
  return value;
}

function listQuery(query: IssueQuery, cursors?: Record<string, string>): string {
  const params = new URLSearchParams();
  if (query.tracker !== null) params.set("tracker", query.tracker);
  params.set("state", query.state);
  for (const label of query.labels) params.append("label", label);
  if (query.assignee !== "any") params.set("assignee", query.assignee);
  if (query.text !== "") params.set("q", query.text);
  if (query.sort !== undefined) params.set("sort", query.sort);
  const paged = cursorsValue(cursors);
  return paged === null ? params.toString() : `${params.toString()}&cursors=${paged}`;
}

export function createHttpIssuesApi(): IssuesApi {
  return {
    trackers: async (signal) => {
      try {
        const rows = await request<unknown>("GET", "/api/issues/trackers", undefined, signal);
        // Anything but a list is not this service's answer: the same as a service that does not report issues.
        return Array.isArray(rows) ? (rows as TrackerRow[]) : null;
      } catch (failure) {
        if (failure instanceof ApiFailure && failure.status === 404) return null;
        throw failure;
      }
    },
    list: async (query, signal, cursors): Promise<IssueList> => {
      const body = await request<Partial<IssueList>>("GET", `/api/issues?${listQuery(query, cursors)}`, undefined, signal);
      return { issues: asArray(body?.issues), counts: body?.counts ?? null, failed: asArray(body?.failed), next_cursors: body?.next_cursors ?? {} };
    },
    get: async (issue, signal): Promise<IssueDetail> => readDetail(await request<IssueDetail>("GET", `/api/issues/item?${target(issue)}`, undefined, signal)),
    links: async (signal) => (await request<Record<string, IssueRef[]> | undefined>("GET", "/api/issues/links", undefined, signal)) ?? {},
    find: async (input, signal) => asArray<IssueRef>(await request<unknown>("GET", `/api/issues/find?${new URLSearchParams({ q: input })}`, undefined, signal)),
    draft: (issue, signal) => request<ThreadDraft>("GET", `/api/issues/draft?${target(issue)}`, undefined, signal),
    link: (threadId, issue) => request<void>("POST", `/api/threads/${segment(threadId)}/issue-links`, named(issue)),
    unlink: (threadId, issue) => request<void>("POST", `/api/threads/${segment(threadId)}/issue-links/remove`, named(issue)),
    connect: (tracker: TrackerKind, input) => request<TrackerRow>("POST", `/api/issues/trackers/${segment(tracker)}/connect`, input),
    disconnect: (tracker: TrackerKind) => request<TrackerRow>("POST", `/api/issues/trackers/${segment(tracker)}/disconnect`),
    startThread: (issue, input) => request("POST", "/api/issues/start-thread", { ...named(issue), message: input.message, project_id: input.project_id }),
    // A read that changes nothing: with no query the route can only answer 400 or 409 when it exists, and a 405, a 404 whose code
    // is not `not_found`, or the SPA fallback's own 404 `not_found` (`No such route.`) says it does not (#1530).
    offers: async (signal) => {
      try {
        await request<unknown>("GET", "/api/issues/candidates", undefined, signal);
        return true;
      } catch (failure) {
        if (failure instanceof ApiFailure && failure.status > 0) return !routeAbsent(failure.status, failure.code, failure.message);
        throw failure;
      }
    },
    candidates: async (issue, signal) => {
      const body = await request<unknown>("GET", `/api/issues/candidates?${new URLSearchParams({ tracker: issue.tracker, scope: issue.scope })}`, undefined, signal);
      return readCandidates(body);
    },
    update: async (issue, input) => {
      // Over a limit, or empty: refused here, before anything is sent. Only the fields present are written.
      const problem = updateProblem(input);
      if (problem) throw new Error(problem);
      const fields: Record<string, unknown> = {};
      for (const name of ["title", "body", "labels", "assignees"] as const) if (input[name] !== undefined) fields[name] = input[name];
      return readDetail(await request<IssueDetail>("POST", "/api/issues/item/update", { ...named(issue), ...fields }));
    },
    setState: async (issue, state) => readDetail(await request<IssueDetail>("POST", "/api/issues/item/state", { ...named(issue), state: state.name, ...(state.close_reason ? { close_reason: state.close_reason } : {}) })),
    comment: async (issue, body) => {
      // Blank or over a limit: refused here, before anything is sent.
      const problem = commentProblem(body);
      if (problem) throw new Error(problem);
      return readComment(await request<unknown>("POST", "/api/issues/item/comment", { ...named(issue), body }));
    },
  };
}

/**
 * The client the app starts with. It asks `trackers()` first and stays out of the way until the service says it reports issues:
 * a `404` (the flag is off, or the routes do not exist) is "nothing", cached for the life of the page, and every other read then
 * answers empty without a request and every write is refused, so no screen draws and no error shows. A reply that is not a 404
 * turns the real client on.
 */
export function createLiveIssuesApi(http: IssuesApi = createHttpIssuesApi()): IssuesApi {
  let seen: "unknown" | "absent" | "present" = "unknown";
  let probe: Promise<void> | null = null;
  let offered: boolean | null = null;
  const trackers: IssuesApi["trackers"] = async (signal) => {
    if (seen === "absent") return null;
    const rows = await http.trackers(signal);
    seen = rows === null ? "absent" : "present";
    return rows;
  };
  /** Whether the real client is on. One probe is shared; a network failure asks again next time and counts as not on for now. */
  const present = async (): Promise<boolean> => {
    if (seen === "unknown") {
      probe ??= trackers().then(() => undefined, () => undefined).finally(() => { probe = null; });
      await probe;
    }
    return seen === "present";
  };
  const quiet = <A extends unknown[], R>(call: (...args: A) => Promise<R>, empty: R) =>
    async (...args: A): Promise<R> => ((await present()) ? call(...args) : empty);
  const strict = <A extends unknown[], R>(call: (...args: A) => Promise<R>) =>
    async (...args: A): Promise<R> => {
      if (!(await present())) throw new Error("Issues are not reported by this service.");
      return call(...args);
    };
  return {
    trackers,
    list: quiet(http.list, { issues: [], counts: null, failed: [], next_cursors: {} }),
    get: strict(http.get),
    links: quiet(http.links, {}),
    find: quiet(http.find, []),
    draft: strict(http.draft),
    link: strict(http.link),
    unlink: strict(http.unlink),
    connect: strict(http.connect),
    disconnect: strict(http.disconnect),
    startThread: strict(http.startThread),
    // A 404 or 405 from the first probe is cached as "not offered" for the life of the page; a network failure asks again next time.
    offers: async (signal) => {
      if (offered !== null) return offered;
      if (!(await present())) return false;
      try {
        offered = await http.offers(signal);
      } catch {
        return false;
      }
      return offered;
    },
    candidates: strict(http.candidates),
    update: strict(http.update),
    setState: strict(http.setState),
    comment: strict(http.comment),
  };
}
