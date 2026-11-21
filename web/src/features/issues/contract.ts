/**
 * The issues the tracker screens read and the writes they make, as docs/issues.md defines them (Eng 4, PR #922).
 *
 * Hand-written from that doc: the Rust types in `orbyn-contracts` and the generated types are not on main yet. When they land,
 * replace these types with the generated ones in the same commit that checks the client against the service. Field names follow
 * the T3 Code issue schema (MIT, 9168ca73d, names only, no code copied) in snake_case.
 *
 * Nothing here can carry a secret on the way in except `TrackerConnectInput.token`, which goes outward once, in the JSON body of
 * the connect route. No shape has a field for a token on the way out. Titles, bodies, labels and names are the tracker's text and
 * are untrusted: render them as text only.
 */

export const TRACKER_KINDS = ["github", "gitlab", "azure_devops", "forgejo", "linear"] as const;
export type TrackerKind = (typeof TRACKER_KINDS)[number];

/**
 * Where a tracker stands, worked out from its row alone (`trackerStatus` in model.ts): `signed_in` is GitHub through the person's
 * own `gh` sign-in, `saved` is a connection with its token in the keychain, `attention` is a stored connection that is not working
 * (the row says why in `detail`), `not_connected` is the rest. The service sends none of these words.
 */
export type TrackerStatus = "signed_in" | "saved" | "attention" | "not_connected";

/** One tracker as the picker lists it. A tracker the service does not know of is simply not in the list. */
export interface TrackerRow {
  tracker: TrackerKind;
  connected: boolean;
  /** What it is connected to: a repository, a project, a team. */
  scope?: string | null;
  /** The host the connection uses: the GitLab or Forgejo address, or the Azure DevOps organisation URL. Not secret; never a token. */
  host?: string | null;
  /** Why it is not connected, in the service's words ("gh is not installed"). */
  detail?: string | null;
}

/**
 * The body of the connect route: the host where the tracker needs one (`https://` only: the service refuses anything else), a scope
 * when the person names one, and the token, once. `token` is absent for GitHub. Nothing answers with any token.
 */
export interface TrackerConnectInput {
  host?: string;
  scope?: string;
  token?: string;
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
  closed_at?: string | null;
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

/** A pull request the tracker reports against an issue, of its own accord. Read only. */
export interface LinkedPullRequest {
  repository: string;
  number: number;
  title: string;
  url: string;
  state: "open" | "closed" | "merged";
  is_draft: boolean;
  closes_issue: boolean;
}

export interface IssueDetail extends IssueSummary {
  body: string;
  /** The first 50, oldest first; never more in the beta. */
  comments: IssueComment[];
  /** What the tracker reports of its own accord; `[]` for a tracker that does not report them. */
  linked_pull_requests: LinkedPullRequest[];
  /** The ids of the threads that link this issue. */
  linked_threads: string[];
  /**
   * The states this issue can move to, in the tracker's own words and order (docs/issues.md, "Issue writes and list controls").
   * Absent: the service does not offer a state change, and the State row shows the state with a Planned note. `[]`: it does, but
   * this tracker has no state to offer. The real client keeps only well-formed entries.
   */
  states?: IssueState[];
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

/** The three fields that name one issue: a key is unique only within a tracker's scope. */
export type IssueTarget = Pick<IssueRef, "tracker" | "scope" | "key">;

/** The newest first by that date; `updated` is the service's default. */
export type IssueSort = "updated" | "created";

/** The filters are state, label, assignee and search, and nothing else; `sort` is how the list is ordered, not a filter. */
export interface IssueQuery {
  /** `null` is every connected tracker. */
  tracker: TrackerKind | null;
  state: "open" | "closed" | "all";
  labels: string[];
  assignee: "any" | "me" | "none";
  text: string;
  /** Absent: the service's own order (`updated`). Sent only when the person chose one. */
  sort?: IssueSort;
}

export interface IssueList {
  issues: IssueSummary[];
  /** Counts for the tabs, before any label, assignee or search filter. `null` when the tracker cannot say. */
  counts: { open: number; closed: number } | null;
  /** A tracker that did not answer while others did: its name and the service's own message. */
  failed: { tracker: TrackerKind; message: string }[];
  /** Where each tracker and scope left off, `"<tracker> <scope>"` to an opaque cursor; only those with more are listed. Send it back as `cursors` for the next page. */
  next_cursors: Record<string, string>;
}

/**
 * The body of `POST /api/issues/item/update`: only the fields present are written; `labels` and `assignees` are the whole new set.
 * `labels` are names; `assignees` are the candidates' `id`s (docs/issues.md: "`id` is how the tracker addresses the person on a write").
 * The client refuses a title over 256 characters or with a line break, a body over 64 KiB, more than 50 labels, more than 25
 * assignees, and a body with none of the four, before anything is sent.
 */
export interface IssueUpdateInput {
  title?: string;
  body?: string;
  labels?: string[];
  assignees?: string[];
}

/** What a label or assignee picker offers, read when the picker opens: at most 200 of each, `truncated` when there were more. */
export interface IssueCandidates {
  labels: { name: string; color?: string | null }[];
  assignees: { login: string; name?: string | null; id: string }[];
  truncated: boolean;
}

export interface StartThreadInput {
  /** The first message, already edited by the person. */
  message: string;
  project_id: string;
}

/** The thread the service created and linked; the same `Thread` the rest of the app holds. */
export type StartedThread = import("../../api/types").Thread;

/** What the draft route answers: the framed first message, for the person to read and edit before it is sent. */
export interface ThreadDraft { message: string }

/**
 * Every call is a route in docs/issues.md. Every write is Person-class: its route is in `PERSON_ROUTES` or, until `routes.json`
 * lists it (`update`, `setState`), `PENDING_PERSON_ROUTES` in `web/src/api/personKey.ts`, so the key goes with it.
 */
export interface IssuesApi {
  /** `null` is "this service does not report issues" (the Issues flag is off, a 404): the screens hide and no other call is made. */
  trackers(signal?: AbortSignal): Promise<TrackerRow[] | null>;
  /**
   * `cursors` is a `next_cursors` the previous answer gave, for the next page of those scopes only, with the same query. The
   * client sends it once, as JSON then URL-encoded, at most 4 KiB; a larger value is refused before anything is sent. A cursor is
   * opaque: the page never builds or edits one.
   */
  list(query: IssueQuery, signal?: AbortSignal, cursors?: Record<string, string>): Promise<IssueList>;
  get(issue: IssueTarget, signal?: AbortSignal): Promise<IssueDetail>;
  /** Every thread's linked issues, by thread id, oldest link first. One read for the chip, the rows and the detail card. */
  links(signal?: AbortSignal): Promise<Record<string, IssueRef[]>>;
  /** A number, words or a pasted issue link, searched over the connected trackers. Empty when nothing matches; empty input lists the latest. */
  find(input: string, signal?: AbortSignal): Promise<IssueRef[]>;
  /** The first message for a thread started from this issue, framed so the issue's text is data. The person sees and edits it; nothing is written. */
  draft(issue: IssueTarget, signal?: AbortSignal): Promise<ThreadDraft>;
  /** One write per call: adds the link. An issue the tracker does not have is a 404 `not_found`. */
  link(threadId: string, issue: IssueTarget): Promise<void>;
  /** One write per call: removes the link from this thread only. The issue is untouched. */
  unlink(threadId: string, issue: IssueTarget): Promise<void>;
  /**
   * WRITE-ONLY. One write per call: connects the tracker (the service keeps the token in the keychain) and answers with its row, which
   * has no token in it. No call reads a token back. GitHub takes none. This is the one place a secret crosses the contract, and only
   * outward, in the JSON body and never in the path or query: the argument is passed once, never stored, echoed, logged or read back.
   */
  connect(tracker: TrackerKind, input: TrackerConnectInput): Promise<TrackerRow>;
  /** One write per call: removes the host, scopes and keychain token, and answers with the tracker's row. */
  disconnect(tracker: TrackerKind): Promise<TrackerRow>;
  /** One write per call: creates the thread, links it to the issue and queues `message`, and answers with the thread. */
  startThread(issue: IssueTarget, input: StartThreadInput): Promise<StartedThread>;
  /**
   * Whether the service offers the Issue writes and the sort (docs/issues.md, "Issue writes and list controls"): `update`,
   * `candidates` and `sort`. The live client asks once with a read that changes nothing, and a 404 or 405 is "not offered"; until it
   * is known the answer is `false`, and the controls stay Planned. A State change is discovered per issue, by `IssueDetail.states`.
   */
  offers(signal?: AbortSignal): Promise<boolean>;
  /** The labels and people a picker can offer for this issue's scope. Read when a picker opens, never with the issue. */
  candidates(issue: IssueTarget, signal?: AbortSignal): Promise<IssueCandidates>;
  /** One write per call, Person-class: `POST /api/issues/item/update`. Answers the updated issue. */
  update(issue: IssueTarget, input: IssueUpdateInput): Promise<IssueDetail>;
  /** One write per call, Person-class: `POST /api/issues/item/state`, `state` one of the issue's `states`. Answers the updated issue. */
  setState(issue: IssueTarget, state: IssueState): Promise<IssueDetail>;
}

export const TRACKER_NAMES: Record<TrackerKind, string> = {
  github: "GitHub",
  gitlab: "GitLab",
  azure_devops: "Azure DevOps",
  forgejo: "Forgejo",
  linear: "Linear",
};

/** What a tracker's host field is asked for, and the placeholder, by tracker. GitHub and Linear have none (Linear's host is fixed). The service takes `https://` hosts only; a bare name is sent with `https://` in front. */
export const TRACKER_ADDRESS: Partial<Record<TrackerKind, { label: string; initial: string; hint: string; title: string }>> = {
  gitlab: { label: "GitLab address", initial: "gitlab.com", hint: "Leave gitlab.com, or enter your own server, for example gitlab.example.com.", title: "GitLab API token" },
  forgejo: { label: "Forgejo address", initial: "", hint: "Your own Forgejo server, or codeberg.org.", title: "Forgejo address and token" },
  azure_devops: { label: "Organisation URL", initial: "", hint: "The address of your organisation, as dev.azure.com/name.", title: "Azure DevOps organisation and token" },
};
