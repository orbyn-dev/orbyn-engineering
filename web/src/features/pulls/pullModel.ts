import type { PullDetailsResponse, PullListEntry, PullRequest } from "../../api/types";

/** What the details request returns for one pull request. */
export type PullDetails = PullDetailsResponse["details"][number];

export type PullState = "open" | "closed" | "merged";
export type ReviewKey = "approved" | "changes_requested" | "review_required";
export type CheckKey = "passing" | "failing" | "running";

/** What the list orders by, and which end comes first. */
export interface PullSort {
  by: "updated" | "created" | "comments";
  dir: "newest" | "oldest";
}

export interface PullFilters {
  /** Which states to list. The service answers one state at a time, so each is asked for on its own. */
  states: PullState[];
  draft: "all" | "only" | "hide";
  review: ReviewKey[];
  checks: CheckKey[];
  project: string | null;
  author: string | null;
  /** Only pull requests by one of the signed-in accounts. */
  me: boolean;
  labels: string[];
  /** Whose review is asked for, as `user:<login>` or `team:<slug>`. Any one matches. */
  requestedFrom: string[];
}

/**
 * What the forge contract does not carry yet. Each flag turns one hidden control back on, in the
 * place the design draws it, once its field arrives. All false for now.
 */
export interface PullData {
  /** Who is asked to review: the "Needs my review" filter. */
  reviewRequests: boolean;
  /** The signed-in person's login: the Author "Me" choice. */
  viewerLogin: boolean;
  /** A comment count: the "Most comments" sort. */
  commentCount: boolean;
  /** A creation time: Created and Oldest across several repositories. */
  createdAt: boolean;
}
export const pullData: PullData = { reviewRequests: false, viewerLogin: false, commentCount: false, createdAt: false };

/** The details the service sends for one listed pull request, by `project:number`. */
export type DetailsByKey = ReadonlyMap<string, PullDetails>;
export const detailsKey = (project: string, number: number) => `${project}:${number}`;

/** Whether a filter can only be answered from details, which the list does not carry. */
export const needsDetails = (filters: PullFilters) => filters.review.includes("review_required") || filters.requestedFrom.length > 0;

/** The same filters with the ones that need details left out, for the rows shown while those are missing. */
export const withoutDetailFilters = (filters: PullFilters): PullFilters => ({
  ...filters,
  review: filters.review.filter((key) => key !== "review_required"),
  requestedFrom: [],
});

export const defaultSort: PullSort = { by: "updated", dir: "newest" };

/** The service says `pass` and `fail`; older stand-ins say `passing` and `failing`. */
export const checksOf = (state: string | null): "pass" | "fail" | "pending" | "none" =>
  state === "pass" || state === "passing" ? "pass" : state === "fail" || state === "failing" ? "fail" : state === "pending" ? "pending" : "none";

export const noFilters: PullFilters = {
  states: ["open"],
  draft: "all",
  review: [],
  checks: [],
  project: null,
  author: null,
  me: false,
  labels: [],
  requestedFrom: [],
};

/** Splits `label:x` out of the search, which the list filters on itself. */
export function parseSearch(text: string): { query: string; labels: string[] } {
  const words = text.trim().split(/\s+/).filter(Boolean);
  const labels = words.filter((word) => word.length > 6 && word.startsWith("label:")).map((word) => word.slice(6).toLowerCase());
  return { query: words.filter((word) => !(word.length > 6 && word.startsWith("label:"))).join(" "), labels };
}

/**
 * Pulls finished `label:x` words out of what is being typed. A word is finished once a space
 * follows it, or when `final` says the text is being submitted or left.
 */
export function takeLabels(value: string, final: boolean): { labels: string[]; rest: string } {
  const labels: string[] = [];
  const rest = value
    .replace(final ? /(^|\s)label:(\S+)(?=\s|$)/gi : /(^|\s)label:(\S+)(?=\s)/gi, (_, lead: string, name: string) => {
      labels.push(name.toLowerCase());
      return lead;
    })
    .replace(/\s{2,}/g, " ")
    .replace(/^\s+/, "");
  return { labels, rest: final ? rest.trim() : rest };
}

/** The distinct values a list holds, for a filter to offer. */
export const authorsOf = (entries: readonly PullListEntry[]) =>
  [...new Set(entries.flatMap(({ pull }) => pull.author ?? []))].sort();
export const labelsOf = (entries: readonly PullListEntry[]) =>
  [...new Set(entries.flatMap(({ pull }) => pull.labels))].sort();

const REVIEW_LABELS: Record<ReviewKey, string> = {
  review_required: "needs my review",
  approved: "approved",
  changes_requested: "changes requested",
};

/** `Me`-free display of a `user:` or `team:` entry. */
export const requestedName = (entry: string) => (entry.startsWith("team:") ? `@${entry.slice(5)}` : entry.slice(5));

/** One removable chip per filter that is on. Open, drafts included and anyone are the defaults and never count. */
export interface FilterChip {
  id: string;
  label: string;
  remove(filters: PullFilters): PullFilters;
}

export function activeChips(filters: PullFilters): FilterChip[] {
  const chips: FilterChip[] = [];
  const isDefaultState = filters.states.length === 1 && filters.states[0] === "open";
  if (!isDefaultState) {
    for (const state of filters.states) {
      chips.push({
        id: `state:${state}`,
        label: `State: ${state}`,
        remove: (f) => {
          const states = f.states.filter((one) => one !== state);
          return { ...f, states: states.length > 0 ? states : ["open"] };
        },
      });
    }
  }
  if (filters.draft !== "all") {
    chips.push({ id: "draft", label: `Draft: ${filters.draft === "hide" ? "exclude" : "only"}`, remove: (f) => ({ ...f, draft: "all" }) });
  }
  for (const key of filters.review) {
    chips.push({ id: `review:${key}`, label: `Review: ${REVIEW_LABELS[key]}`, remove: (f) => ({ ...f, review: f.review.filter((one) => one !== key) }) });
  }
  for (const key of filters.checks) {
    chips.push({ id: `checks:${key}`, label: `Checks: ${key}`, remove: (f) => ({ ...f, checks: f.checks.filter((one) => one !== key) }) });
  }
  if (filters.me) chips.push({ id: "author:me", label: "Author: me", remove: (f) => ({ ...f, me: false }) });
  if (filters.author) chips.push({ id: "author", label: `Author: ${filters.author}`, remove: (f) => ({ ...f, author: null }) });
  for (const entry of filters.requestedFrom) {
    chips.push({
      id: `requested:${entry}`,
      label: `Review requested from: ${requestedName(entry)}`,
      remove: (f) => ({ ...f, requestedFrom: f.requestedFrom.filter((one) => one !== entry) }),
    });
  }
  for (const name of filters.labels) {
    chips.push({ id: `label:${name}`, label: `Label: ${name}`, remove: (f) => ({ ...f, labels: f.labels.filter((one) => one !== name) }) });
  }
  return chips;
}

/** Every filter back to its default. The project is its own control and stays. */
export const clearedFilters = (filters: PullFilters): PullFilters => ({ ...noFilters, project: filters.project });

/** The service state words to ask for: all three at once is the service's `all`. */
export const statesToAsk = (states: readonly PullState[]): string[] => (states.length === 3 ? ["all"] : [...states]);

/** `Updated`, `Created` or `Oldest`: the sort as the button names it. */
export const sortName = (sort: PullSort) =>
  sort.by === "updated" ? "Updated" : sort.by === "comments" ? "Most comments" : sort.dir === "oldest" ? "Oldest" : "Created";

const sameLogin = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** Whether one of the signed-in accounts is among the people or teams asked to review. */
export const reviewAskedOf = (details: PullDetails | undefined, viewers: readonly string[]) =>
  Boolean(details?.requested_reviewers?.some((one) => one.kind === "user" && viewers.some((login) => sameLogin(login, one.name))));

export function visiblePulls(
  entries: readonly PullListEntry[],
  filters: PullFilters,
  labels: readonly string[],
  sort: PullSort,
  /** The signed-in logins, once the service reports them. */
  viewers: readonly string[] = [],
  /** What the details request has returned so far. */
  details: DetailsByKey = new Map(),
): PullListEntry[] {
  const detailsOf = ({ project_id, pull }: PullListEntry) => details.get(detailsKey(project_id, pull.number));
  const kept = entries.filter((entry) => {
    const { project_id, pull } = entry;
    if (filters.project && project_id !== filters.project) return false;
    if (filters.draft === "only" && !pull.draft) return false;
    if (filters.draft === "hide" && pull.draft) return false;
    const asked = filters.review.filter((key) => key !== "review_required");
    const needsMe = filters.review.includes("review_required");
    // A row whose reviewers are unknown is kept: a filter never runs on missing data.
    const unknown = detailsOf(entry)?.requested_reviewers === undefined;
    if (asked.length > 0 || needsMe) {
      const byState = (asked as string[]).includes(pull.review_state ?? "none");
      if (!byState && !(needsMe && (unknown || reviewAskedOf(detailsOf(entry), viewers)))) return false;
    }
    if (filters.requestedFrom.length > 0 && !unknown) {
      const people = detailsOf(entry)?.requested_reviewers ?? [];
      if (!people.some((one) => filters.requestedFrom.includes(`${one.kind}:${one.name}`))) return false;
    }
    if (filters.checks.length > 0) {
      const word = checksOf(pull.checks_state);
      const key = word === "pass" ? "passing" : word === "fail" ? "failing" : word === "pending" ? "running" : null;
      if (!key || !filters.checks.includes(key)) return false;
    }
    if (filters.me && !(pull.author && viewers.some((login) => sameLogin(login, pull.author ?? "")))) return false;
    if (filters.author && pull.author !== filters.author) return false;
    const names = pull.labels.map((label) => label.toLowerCase());
    if (!filters.labels.every((label) => names.includes(label.toLowerCase()))) return false;
    return labels.every((label) => names.includes(label));
  });
  // The forge reports no creation time in the list, so the number stands in for it until details bring one.
  const number = (entry: PullListEntry) => String(entry.pull.number).padStart(12, "0");
  const anyCreated = kept.some((entry) => detailsOf(entry)?.created_at);
  const key = (entry: PullListEntry): string | null =>
    sort.by === "updated"
      ? (entry.pull.updated_at ?? "")
      : sort.by === "comments"
        ? ((count) => (count === undefined ? null : String(count).padStart(12, "0")))(detailsOf(entry)?.comment_count)
        : anyCreated
          ? ((at) => (at ? `${at}|${number(entry)}` : null))(detailsOf(entry)?.created_at)
          : number(entry);
  const order = sort.dir === "newest" ? -1 : 1;
  // A pull request whose forge gives no value for the sort comes last, whichever way it runs.
  return [...kept].sort((a, b) => {
    const [ka, kb] = [key(a), key(b)];
    if (ka === null || kb === null) return ka === kb ? 0 : ka === null ? 1 : -1;
    return order * ka.localeCompare(kb);
  });
}

/** What the Checks cell says for a pull request, from the counts the list carries. `null` when it carries none. */
export interface ChecksView {
  tone: "fail" | "running" | "pass" | "neutral" | "none";
  label: string;
  /** The tooltip's lines, when there is one. */
  tip?: { title: string; lines: string[] };
}

export function checksView(pull: PullRequest): ChecksView | null {
  const counts = pull.checks;
  if (!counts) return null;
  if (counts.total === 0) return { tone: "none", label: "No checks" };
  if (counts.failing > 0) {
    const first = counts.failing_names[0];
    const more = counts.failing - 1;
    const hidden = counts.failing - counts.failing_names.length;
    return {
      tone: "fail",
      label: `Failing${first ? ` \u00b7 ${first}` : ""}${more > 0 && first ? ` +${more}` : ""}`,
      tip:
        counts.failing > 1 || hidden > 0
          ? { title: "Failing", lines: [...counts.failing_names, ...(hidden > 0 ? [`and ${hidden} more`] : [])] }
          : undefined,
    };
  }
  if (counts.running > 0) return { tone: "running", label: `Running \u00b7 ${counts.total - counts.running} of ${counts.total}` };
  if (counts.passing === 0) return { tone: "neutral", label: "No checks ran" };
  const skipped = counts.total - counts.passing;
  return { tone: "pass", label: skipped > 0 ? `Passing \u00b7 ${skipped} skipped` : "Passing" };
}

/** The Your review cell. `null` leaves it empty: no review of yours, and nobody asked you. */
export interface YourReview {
  tone: "approved" | "neutral" | "muted" | "act";
  label: string;
}

export function yourReview(details: PullDetails, viewers: readonly string[]): YourReview | null | undefined {
  switch (details.viewer_review_state) {
    case "approved":
      return { tone: "approved", label: "You approved" };
    case "changes_requested":
      return { tone: "neutral", label: "You requested changes" };
    case "commented":
      return { tone: "neutral", label: "You commented" };
    case "dismissed":
      return { tone: "muted", label: "Your review was dismissed" };
    case "pending":
      return { tone: "act", label: "Finish your review" };
    case "none":
      // Review requested stays hidden until the app knows who is signed in.
      return viewers.length > 0 && reviewAskedOf(details, viewers) ? { tone: "act", label: "Review requested" } : null;
    default:
      return undefined;
  }
}

/** The state a pull request's page opens in, which picks its banner. */
export type PageState = "merged" | "closed" | "draft" | "conflict" | "failed" | "changes" | "review" | "approved" | "running" | "open";

export function pageState(pull: PullRequest, mergeable: string | null, reviewAsked: boolean): PageState {
  if (pull.state === "merged") return "merged";
  if (pull.state === "closed") return "closed";
  if (pull.draft) return "draft";
  if (mergeable === "conflicting") return "conflict";
  const checks = checksOf(pull.checks_state);
  if (checks === "fail") return "failed";
  if (pull.review_state === "changes_requested") return "changes";
  if (reviewAsked && pull.review_state !== "approved") return "review";
  if (pull.review_state === "approved" && checks === "pass" && mergeable === "mergeable") return "approved";
  return checks === "pending" ? "running" : "open";
}

/** Whether Merge can be pressed, and the line under it that says why not, or what is true. */
export function mergeGate(
  pull: PullRequest,
  mergeable: string | null,
  options: { offline: boolean; auto: boolean; forge: string },
): { enabled: boolean; hint: string; tip?: string } {
  if (pull.state === "merged") return { enabled: false, hint: `Nothing left to do on ${options.forge}.` };
  if (pull.state === "closed") return { enabled: false, hint: "Reopen to merge it." };
  if (pull.draft) return { enabled: false, hint: "Mark it ready for review to merge." };
  if (options.offline) return { enabled: false, hint: "Merge needs a connection." };
  if (mergeable === "conflicting") return { enabled: false, hint: "Resolve the conflict to merge." };
  const checks = checksOf(pull.checks_state);
  if (checks === "fail") return { enabled: false, hint: "Merge is blocked by a failing check." };
  if (checks === "pending" && !options.auto) return { enabled: false, hint: "Merge unlocks when checks pass.", tip: "Checks are still running." };
  if (pull.review_state === "review_required" || pull.review_state === "changes_requested") return { enabled: false, hint: "Merge needs an approval." };
  return { enabled: true, hint: mergeable === "mergeable" ? "No conflicts" : "" };
}

/** The Review line of the status card, from what the forge reports. */
export function reviewWords(pull: PullRequest, reviewers: readonly string[]): string {
  if (pull.draft) return "Not requested";
  if (pull.review_state === "approved") return "Approved";
  if (pull.review_state === "changes_requested") return "Changes requested";
  return reviewers.length > 0 ? `Requested from ${reviewers.map((name) => `@${name}`).join(", ")}` : "Not requested";
}

/** True when the markdown's first line is a heading with this text (any level, any case), so a card titled the same would say it twice. */
export function bodyOpensWith(body: string | null | undefined, title: string): boolean {
  const first = (body ?? "").split("\n").find((line) => line.trim() !== "");
  const heading = first?.match(/^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$/);
  return heading !== undefined && heading !== null && (heading[1] ?? "").trim().toLowerCase() === title.toLowerCase();
}

/**
 * Columns with nothing to show. Reviewers and Your review are left out of the table when every row's details have
 * arrived and none has anything for them (pr-list-no-review-data); while any row's details are still coming, or
 * the details failed, the columns stay so the table does not jump.
 */
export function emptyColumns(
  rows: readonly PullListEntry[],
  details: DetailsByKey,
  viewers: readonly string[],
): { reviewers: boolean; review: boolean } {
  if (rows.length === 0) return { reviewers: false, review: false };
  const known = rows.map((entry) => details.get(detailsKey(entry.project_id, entry.pull.number)));
  if (known.some((row) => row === undefined)) return { reviewers: false, review: false };
  const rowsWith = known as PullDetails[];
  return {
    reviewers: rowsWith.every((row) => (row.requested_reviewers ?? []).length === 0),
    review: rowsWith.every((row) => (yourReview(row, viewers) ?? null) === null),
  };
}

/** Whether the service says the person can change this pull request's base branch. It reports no such permission yet, so this is false until it does (a `can_change_base` flag on the detail). */
export const baseChangeAllowed = (detail: object): boolean => (detail as { can_change_base?: unknown }).can_change_base === true;
