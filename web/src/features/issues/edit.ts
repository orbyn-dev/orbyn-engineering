import type { IssueCandidates, IssueComment, IssueDetail, IssueState, IssueUpdateInput } from "./contract";

/**
 * The limits docs/issues.md ("Issue writes and list controls") puts on `POST /api/issues/item/update`, and the checks that keep the
 * page inside them. Issue text going in is as untrusted as issue text coming out: it is sent as a field value, never rendered as
 * markup here, and a request over a limit is refused on this side before anything is sent.
 */
export const TITLE_MAX = 256;
/** 64 KiB, counted in bytes of UTF-8 as the service counts them. */
export const BODY_MAX = 64 * 1024;
export const LABELS_MAX = 50;
export const ASSIGNEES_MAX = 25;
/** The most of each a picker draws; the service reads at most 200 and says `truncated` when there were more. */
export const CANDIDATES_MAX = 200;
/** 64 KiB, counted in bytes of UTF-8 as the service counts them: a comment is 1 to 64 KiB and not blank. */
export const COMMENT_MAX = 64 * 1024;

const encoder = new TextEncoder();
export const byteLength = (text: string): number => encoder.encode(text).length;

/** Why this update cannot be sent, in words for the person, or `null` when it can. */
export function updateProblem(input: IssueUpdateInput): string | null {
  const { title, body, labels, assignees } = input;
  if (title === undefined && body === undefined && labels === undefined && assignees === undefined) return "There is nothing to change.";
  if (title !== undefined) {
    if (title.trim() === "") return "The title cannot be empty.";
    if (/[\r\n]/.test(title)) return "The title must be one line.";
    if (title.length > TITLE_MAX) return `The title is over ${TITLE_MAX} characters.`;
  }
  if (body !== undefined && byteLength(body) > BODY_MAX) return `The description is over ${BODY_MAX / 1024} KiB.`;
  if (labels !== undefined && labels.length > LABELS_MAX) return `An issue can have at most ${LABELS_MAX} labels.`;
  if (assignees !== undefined && assignees.length > ASSIGNEES_MAX) return `An issue can have at most ${ASSIGNEES_MAX} assignees.`;
  return null;
}

/** Why this comment cannot be sent, in words for the person, or `null` when it can. */
export function commentProblem(body: string): string | null {
  if (body.trim() === "") return "The comment cannot be empty.";
  if (byteLength(body) > COMMENT_MAX) return `The comment is over ${COMMENT_MAX / 1024} KiB.`;
  return null;
}

/**
 * The transport's own sentence for a path no route claimed: a request under `/api` that the router did not claim is answered by
 * the SPA fallback as `404` `not_found` with this message (`crates/orbynd/src/routes/ui.rs`, the `SERVICE_PREFIXES` branch, where
 * the sentence is written). A route that exists and cannot find the thing it was asked for answers `not_found` with a sentence
 * naming that thing, so the two `404`s are different facts that only this sentence tells apart. It is the one copy of the string
 * on the web side; the service's copy is pinned by `a_service_prefix_miss_answers_no_such_route` in
 * `crates/orbynd/src/routes/tests.rs`, so a reword there fails a test and this constant has to change with it.
 */
export const NO_SUCH_ROUTE = "No such route.";

/**
 * Whether an answer says "this service has no such route" rather than "the thing you asked for is not here": a `405`, a `404`
 * whose code is not `not_found`, or the SPA fallback's own `404 not_found`. The fallback answering a path no route matched is not
 * an offered route, and a route's own `not_found` is not an absent one: reading either as the other offers controls the service
 * does not have (#1530) or hides controls it does.
 */
export const routeAbsent = (status: number, code: string, message: string): boolean =>
  status === 405 || (status === 404 && (code !== "not_found" || message === NO_SUCH_ROUTE));

const asText = (value: unknown, length: number): string | null => (typeof value === "string" ? value.slice(0, length) : null);

/** `states` as the service sent them, keeping only well-formed entries with capped names; `undefined` when it sent no list at all. */
export function readStates(value: unknown): IssueState[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const states: IssueState[] = [];
  for (const entry of value.slice(0, 50)) {
    if (typeof entry !== "object" || entry === null) continue;
    const row = entry as Record<string, unknown>;
    const name = asText(row.name, 80);
    if ((row.category !== "open" && row.category !== "closed") || name === null || name === "") continue;
    const reason = row.close_reason === "completed" || row.close_reason === "not_planned" ? row.close_reason : null;
    states.push({ category: row.category, name, ...(reason ? { close_reason: reason } : {}) });
  }
  return states;
}

/** The candidates as the service sent them: capped lists, capped text, no entry without the field a write needs. */
export function readCandidates(value: unknown): IssueCandidates {
  const row = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const labels: IssueCandidates["labels"] = [];
  for (const entry of (Array.isArray(row.labels) ? row.labels : []).slice(0, CANDIDATES_MAX)) {
    const name = typeof entry === "object" && entry !== null ? asText((entry as Record<string, unknown>).name, 80) : null;
    if (name) labels.push({ name });
  }
  const assignees: IssueCandidates["assignees"] = [];
  for (const entry of (Array.isArray(row.assignees) ? row.assignees : []).slice(0, CANDIDATES_MAX)) {
    if (typeof entry !== "object" || entry === null) continue;
    const who = entry as Record<string, unknown>;
    const login = asText(who.login, 80);
    const id = typeof who.id === "string" || typeof who.id === "number" ? String(who.id).slice(0, 120) : null;
    if (login && id) assignees.push({ login, id, name: asText(who.name, 80) });
  }
  return { labels, assignees, truncated: row.truncated === true || (Array.isArray(row.labels) && row.labels.length > CANDIDATES_MAX) || (Array.isArray(row.assignees) && row.assignees.length > CANDIDATES_MAX) };
}

/** A stored comment as the service answered it, keeping only the fields the issue page draws and capping its text. */
export function readComment(value: unknown): IssueComment {
  const row = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const who = typeof row.author === "object" && row.author !== null ? (row.author as Record<string, unknown>) : null;
  const login = who === null ? null : asText(who.login, 80);
  return {
    id: asText(row.id, 200) ?? "",
    author: login ? { login } : null,
    body: asText(row.body, COMMENT_MAX) ?? "",
    created_at: asText(row.created_at, 64) ?? "",
  };
}

/** The detail an update or a state write answers, with what an older answer leaves out filled in, as `get` does. */
export function readDetail(body: IssueDetail): IssueDetail {
  const list = <T,>(value: unknown): T[] => (Array.isArray(value) ? (value as T[]) : []);
  const { states, ...detail } = body;
  const kept = readStates(states);
  return { ...detail, comments: list(body.comments), linked_pull_requests: list(body.linked_pull_requests), linked_threads: list(body.linked_threads), assignees: list(body.assignees), labels: list(body.labels), ...(kept ? { states: kept } : {}) };
}
