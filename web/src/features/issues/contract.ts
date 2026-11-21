/**
 * The issues the tracker screens read and the one write they make, as the screens need them.
 *
 * GUESSED CONTRACT. No issues route, Rust type or generated type is on main: Eng 4 is building them (docs/issues.md).
 * Nothing here is generated, and the only implementation of `IssuesApi` besides the "reports nothing" client in
 * api.ts is the mock in web/mock/issues.ts. Field NAMES follow the T3 Code issue schema (MIT, 9168ca73d, names only,
 * no code copied) in snake_case; the shapes and every route in api.ts are guesses for Eng 4 to correct. When the
 * routes land, replace these hand-written types with the generated ones in the same commit that adds the real client.
 *
 * Nothing here can carry a secret. A tracker token lives in the keychain behind the service; the UI never sees one.
 * Titles, bodies, labels and names are the tracker's text and are untrusted: render them as text only.
 */

export const TRACKER_KINDS = ["github", "gitlab", "azure_devops", "forgejo", "linear"] as const;
export type TrackerKind = (typeof TRACKER_KINDS)[number];

/** One tracker as the picker lists it. A tracker the service does not know of is simply not in the list. */
export interface TrackerRow {
  tracker: TrackerKind;
  connected: boolean;
  /** What it is connected to: a repository, a team, a server. Absent when not connected. */
  scope?: string | null;
}

/** `open` or `closed`, plus the tracker's own word, and how it closed when the tracker says so (`not_planned`). */
export interface IssueState {
  category: "open" | "closed";
  name: string;
  close_reason?: "completed" | "not_planned" | null;
}

export interface IssueActor { login: string }
export interface IssueLabel { name: string }

export interface IssueSummary {
  tracker: TrackerKind;
  scope: string;
  /** The tracker's own key: `412`, `ORB-87`. Unique with `tracker` and `scope`. */
  key: string;
  /** How it is written: `#412`, `ORB-87`. */
  display_key: string;
  title: string;
  /** Where the tracker shows it. Only ever opened by a click on "Open on <tracker>". */
  url: string;
  author: IssueActor | null;
  state: IssueState;
  created_at: string;
  updated_at: string;
  assignees: IssueActor[];
  labels: IssueLabel[];
  comment_count: number;
}

export interface IssueComment {
  id: string;
  author: IssueActor | null;
  body: string;
  created_at: string;
}

export interface IssueDetail extends IssueSummary {
  body: string;
  comments: IssueComment[];
}

/** What a thread's link keeps of an issue: enough for the chip, the list and the row marker. */
export interface IssueRef {
  tracker: TrackerKind;
  scope: string;
  key: string;
  display_key: string;
  title: string;
  state: IssueState;
  labels: IssueLabel[];
}

/** The filters are state, label, assignee and search, and nothing else. */
export interface IssueQuery {
  /** `null` is every connected tracker. */
  tracker: TrackerKind | null;
  state: "open" | "closed" | "all";
  labels: string[];
  assignee: "any" | "me" | "none";
  text: string;
}

export interface IssueList {
  issues: IssueSummary[];
  /** Counts for the tabs, before any label, assignee or search filter. `null` when the tracker cannot say. */
  counts: { open: number; closed: number } | null;
  /** A tracker that did not answer while others did: its name and the service's own message. */
  failed: { tracker: TrackerKind; message: string }[];
}

export interface StartThreadInput {
  /** The first message, already edited by the person. */
  message: string;
  project_id: string;
}

/** The thread the service created and linked; the same `Thread` the rest of the app holds. */
export type StartedThread = import("../../api/types").Thread;

export interface IssuesApi {
  /** `null` is "this service does not report issues": the screens hide and no other call is made. */
  trackers(signal?: AbortSignal): Promise<TrackerRow[] | null>;
  list(query: IssueQuery, signal?: AbortSignal): Promise<IssueList>;
  get(tracker: TrackerKind, key: string, signal?: AbortSignal): Promise<IssueDetail>;
  /** Every thread's linked issues, by thread id, oldest link first. One read for the chip, the rows and the detail card. */
  links(signal?: AbortSignal): Promise<Record<string, IssueRef[]>>;
  /** A number, words or a pasted issue link, searched over the connected trackers. Empty when nothing matches. */
  find(input: string, signal?: AbortSignal): Promise<IssueRef[]>;
  /** One write per call: adds the link. A number the tracker does not have is a 404. */
  link(threadId: string, issue: Pick<IssueRef, "tracker" | "scope" | "key">): Promise<void>;
  /** One write per call: removes the link from this thread only. The issue is untouched. */
  unlink(threadId: string, issue: Pick<IssueRef, "tracker" | "scope" | "key">): Promise<void>;
  /** One write per call: creates the thread, links it to the issue and answers with it. */
  startThread(issue: Pick<IssueRef, "tracker" | "scope" | "key">, input: StartThreadInput): Promise<StartedThread>;
}

export const TRACKER_NAMES: Record<TrackerKind, string> = {
  github: "GitHub",
  gitlab: "GitLab",
  azure_devops: "Azure DevOps",
  forgejo: "Forgejo",
  linear: "Linear",
};
