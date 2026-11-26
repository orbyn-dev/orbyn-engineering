import type { PullChecks } from "../../generated/PullChecks";
import type { IssueQuery, IssueRef, IssueState, IssueSummary, LinkedPullRequest, TrackerKind, TrackerRow, TrackerStatus } from "./contract";
import { TRACKER_KINDS, TRACKER_NAMES } from "./contract";
import { relativeTime } from "../../lib/format";

/** The word the state shows: Open, Closed, or Closed as not planned when the tracker says so. Never colour alone. */
export function stateWord(state: IssueState): string {
  if (state.category === "open") return "Open";
  return state.close_reason === "not_planned" ? "Closed as not planned" : "Closed";
}

export const trackerName = (tracker: TrackerKind): string => TRACKER_NAMES[tracker];

export const isTracker = (value: string): value is TrackerKind => (TRACKER_KINDS as readonly string[]).includes(value);

export const noQuery: IssueQuery = { tracker: null, state: "open", labels: [], assignee: "any", text: "" };

/**
 * The search box holds words and `label:` words. `label:bug reload` is the label bug and the words "reload";
 * `label:"good first"` is not understood, a label with a space is chosen in the panel.
 */
export function parseSearch(input: string): { text: string; labels: string[] } {
  const labels: string[] = [];
  const words: string[] = [];
  for (const word of input.trim().split(/\s+/).filter(Boolean)) {
    const label = /^label:(.+)$/i.exec(word)?.[1];
    if (label) labels.push(label);
    else words.push(word);
  }
  return { text: words.join(" "), labels };
}

export function sameLabels(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((label, at) => label.toLowerCase() === b[at]?.toLowerCase());
}

/** One removable chip per active filter, in the order the toolbar shows them. */
export function activeChips(query: IssueQuery): { id: string; label: string }[] {
  return [
    ...(query.text ? [{ id: "text", label: `“${query.text}”` }] : []),
    ...query.labels.map((label) => ({ id: `label:${label}`, label: `label:${label}` })),
    ...(query.assignee === "me" ? [{ id: "assignee", label: "Assigned to me" }] : query.assignee === "none" ? [{ id: "assignee", label: "Unassigned" }] : []),
  ];
}

/** Applies a query to a list: what a service does, kept here so the mock and the tests agree on it. */
export function applyQuery(issues: readonly IssueSummary[], query: IssueQuery, viewer: string | null): IssueSummary[] {
  const words = query.text.toLowerCase().split(/\s+/).filter(Boolean);
  return issues
    .filter((issue) => query.tracker === null || issue.tracker === query.tracker)
    .filter((issue) => query.state === "all" || issue.state.category === query.state)
    .filter((issue) => query.labels.every((wanted) => issue.labels.some((label) => label.name.toLowerCase() === wanted.toLowerCase())))
    .filter((issue) =>
      query.assignee === "any" ? true : query.assignee === "none" ? issue.assignees.length === 0 : viewer !== null && issue.assignees.some((who) => who.login === viewer),
    )
    .filter((issue) => {
      const hay = `${issue.display_key} ${issue.title}`.toLowerCase();
      return words.every((word) => hay.includes(word));
    })
    .sort((a, b) => (query.sort === "created" ? Date.parse(b.created_at) - Date.parse(a.created_at) : Date.parse(b.updated_at) - Date.parse(a.updated_at)));
}

/** What the sort control says: `Sort: Updated` or `Sort: Created`. */
export const sortWord = (sort: IssueQuery["sort"]): string => (sort === "created" ? "Created" : "Updated");

/** The labels in a list with how many issues carry each, most used first. */
export function labelCounts(issues: readonly IssueSummary[]): { name: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const issue of issues) for (const label of issue.labels) counts.set(label.name, (counts.get(label.name) ?? 0) + 1);
  return [...counts].map(([name, count]) => ({ name, count })).sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}

/** "No issue #9999 in orbyn-dev/orbyn-engineering on GitHub." The reason is what the service did not find. */
export const notFoundWords = (display: string, ref: { scope: string; tracker: TrackerKind }) => `No issue ${display} in ${ref.scope} on ${trackerName(ref.tracker)}.`;

export const refKey = (ref: Pick<IssueRef, "tracker" | "scope" | "key">) => `${ref.tracker}:${ref.scope}:${ref.key}`;

/**
 * Where a person's input points: a bare number or `#412` is a number in the project's tracker, anything else is words.
 * A pasted issue URL stays words for the service to resolve; the UI never fetches it.
 */
export function numberOf(input: string): string | null {
  return /^#?(\d{1,16})$/.exec(input.trim())?.[1] ?? null;
}

/** How many of the shown issues are open, closed, for the "1 of 14 open issues" line. */
export function shownLine(shown: number, counts: { open: number; closed: number } | null, state: IssueQuery["state"]): string | null {
  if (!counts) return null;
  const total = state === "open" ? counts.open : state === "closed" ? counts.closed : counts.open + counts.closed;
  return `${shown} of ${total} ${state === "all" ? "" : `${state} `}issues`;
}

export function threadMarker(issues: readonly IssueRef[]): { count: number; closed: boolean; label: string } | null {
  if (issues.length === 0) return null;
  const closed = issues.every((issue) => issue.state.category === "closed");
  const names = issues.map((issue) => issue.display_key).join(", ");
  return { count: issues.length, closed, label: issues.length === 1 ? `Issue ${names}, ${closed ? "closed" : "open"}` : `${issues.length} issues: ${names}` };
}

/** The word a state choice shows: the tracker's own name for a workflow state (In review, Done), the plain words otherwise. */
export function choiceLabel(state: IssueState): string {
  return state.name && state.name !== "Open" && state.name !== "Closed" ? state.name : stateWord(state);
}

export const sameState = (a: IssueState, b: IssueState): boolean =>
  a.category === b.category && a.name === b.name && (a.close_reason ?? null) === (b.close_reason ?? null);

/**
 * Where a tracker stands, from the row alone. A connection that is stored (a host or a scope) but not connected, with the service's
 * reason in `detail`, is `attention`: the token may be rejected or the host may not answer, and the service's words say which.
 */
export function trackerStatus(row: TrackerRow): TrackerStatus {
  if (row.connected) return row.tracker === "github" ? "signed_in" : "saved";
  if (row.tracker !== "github" && row.detail && (row.host || row.scope)) return "attention";
  return "not_connected";
}

const capped = (text: string, length: number): string => (text.length > length ? `${text.slice(0, length - 1)}…` : text);

/** The service's reason a tracker is not working, as text, capped. Empty when it gave none. */
export const trackerDetail = (row: TrackerRow): string => (row.detail ? capped(row.detail, 200) : "");

/** The one line under a tracker's name in Settings and on the Welcome page. Service text is capped; it is drawn as text. */
export function trackerLine(row: TrackerRow): string {
  const where = row.host || row.scope || null;
  const at = where ? ` · ${capped(where, 80)}` : "";
  switch (trackerStatus(row)) {
    case "signed_in": return `Uses the gh sign-in on this computer${row.scope ? ` · ${capped(row.scope, 80)}` : ""}`;
    case "saved": return `Saved in your keychain${at}`;
    case "attention": return `Saved in your keychain${at}`;
    default: {
      const why = trackerDetail(row);
      if (why) return why;
      return row.tracker === "github" ? "Uses the gh sign-in on this computer" : row.tracker === "linear" ? "API token, kept in your keychain" : "Host and API token, kept in your keychain";
    }
  }
}

/**
 * The host the service wants for what the person typed: `https://` in front of a bare name. Anything that already has a scheme is
 * sent as it is, so the service refuses an `http://` one with its own words.
 */
export const hostFor = (typed: string): string => {
  const text = typed.trim();
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`;
};

/** What replaces a failure's words when they carry the token the person typed, in any form. */
export const TOKEN_REFUSED = "The tracker refused the token.";

const b64 = (text: string): string[] => {
  try {
    const plain = btoa(unescape(encodeURIComponent(text)));
    return [plain, plain.replace(/=+$/, ""), plain.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")];
  } catch { return []; }
};

/** Every form a typed token can come back in: as typed, trimmed, URL-encoded, JSON-escaped and base64 (alone and as Basic `:token`). */
function tokenForms(token: string): string[] {
  const trimmed = token.trim();
  const forms = new Set<string>();
  for (const one of [token, trimmed]) {
    if (!one) continue;
    forms.add(one);
    forms.add(encodeURIComponent(one));
    forms.add(JSON.stringify(one).slice(1, -1));
    for (const form of [...b64(one), ...b64(`:${one}`)]) forms.add(form);
  }
  return [...forms].filter((form) => form.length > 0).map((form) => form.toLowerCase());
}

/**
 * The words for a failed write: the service's own message, capped at 300. When a `secret` (the token just typed) turns up in them
 * in any form, the whole message is replaced: the match is made on the whole message BEFORE it is cut, never on the cut one.
 * Never anything the person typed.
 */
export const reasonOf = (failure: unknown, secret?: string): string => {
  const text = failure instanceof Error && failure.message ? failure.message : "Something went wrong.";
  if (secret) {
    const lower = text.toLowerCase();
    if (tokenForms(secret).some((form) => lower.includes(form))) return TOKEN_REFUSED;
  }
  return text.length > 300 ? `${text.slice(0, 299)}…` : text;
};

/** A key the service gave in a URL or a link must be one plain segment before it is asked for: letters, digits and `. _ : # -`. */
export const isIssueKey = (key: string): boolean => /^[\w.:#-]{1,64}$/.test(key);

const MARKS = new Map<string, string>([["github", "GH"], ["gitlab", "GL"], ["azure_devops", "AZ"], ["forgejo", "FJ"], ["linear", "LN"]]);
/** The two letters on a tracker's mark. */
export const trackerMark = (tracker: TrackerKind): string => MARKS.get(tracker) ?? "";

/** The rows this screen knows how to draw, in the order the design lists them; a tracker the service adds later is not drawn. */
export function knownTrackers(rows: readonly TrackerRow[]): TrackerRow[] {
  const byKind = new Map(rows.filter((row) => isTracker(row.tracker)).map((row) => [row.tracker, row] as const));
  return TRACKER_KINDS.flatMap((kind) => { const row = byKind.get(kind); return row ? [row] : []; });
}

/**
 * When a closed issue was closed, in words: "closed 5 days ago". Only for a closed issue that the tracker gave a date for; an open
 * issue never has one, whatever a tracker sends, and a tracker with no date gives none.
 */
export function closedWords(issue: Pick<IssueSummary, "state" | "closed_at">): string | null {
  if (issue.state.category !== "closed" || !issue.closed_at) return null;
  const when = relativeTime(issue.closed_at).toLowerCase();
  return when ? `closed ${when}` : null;
}

/** The state of a linked pull request as a word: an open one has none (it is the plain case), a draft is Draft. */
export function pullWord(pull: Pick<LinkedPullRequest, "state" | "is_draft">): string | null {
  if (pull.state === "merged") return "Merged";
  if (pull.state === "closed") return "Closed";
  return pull.is_draft ? "Draft" : null;
}

/** Where a linked pull request's checks stand, in the Pull requests list's own words: the same `PullChecks` counts. */
export interface LinkedChecksWord {
  tone: "fail" | "running" | "pass" | "neutral" | "none";
  label: string;
}

/**
 * The checks word for a linked pull request, from the counts the tracker reported. `null` when it reported none: absent is
 * unknown, never "no checks" (docs/issues.md), so the row says nothing rather than claiming there are no checks.
 */
export function linkedChecksWord(checks: PullChecks | null | undefined): LinkedChecksWord | null {
  if (!checks) return null;
  if (checks.total === 0) return { tone: "none", label: "No checks" };
  if (checks.failing > 0) {
    const names = Array.isArray(checks.failing_names) ? checks.failing_names : [];
    const first = names[0] ? capped(names[0], 60) : "";
    const more = checks.failing - 1;
    return { tone: "fail", label: `Failing${first ? ` · ${first}` : ""}${more > 0 && first ? ` +${more}` : ""}` };
  }
  if (checks.running > 0) return { tone: "running", label: `Running · ${checks.total - checks.running} of ${checks.total}` };
  if (checks.passing === 0) return { tone: "neutral", label: "No checks ran" };
  const skipped = checks.total - checks.passing;
  return { tone: "pass", label: skipped > 0 ? `Passing · ${skipped} skipped` : "Passing" };
}

export const capText = capped;
