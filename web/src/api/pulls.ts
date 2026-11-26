import { request, type RequestOptions } from "./client";
import { readPreferences } from "./preferences";
import type {
  AskedAboutStop,
  ForgeStatus,
  MergePull,
  PullBulkResult,
  PullCandidate,
  PullCheck,
  PullCheckout,
  PullConversation,
  PullDetailsRequest,
  PullDetailsResponse,
  PullFile,
  PullGuideState,
  PullList,
  PullRequest,
  PullRequestDetail,
  PullStack,
  PullTimelineEvent,
  PullWatch,
  ReviewLineComment,
  UpdatePull,
  Thread,
} from "./types";
import type { PullRevision } from "../generated/PullRevision";
import type { PullChangeBreakdown } from "../generated/PullChangeBreakdown";

const q = encodeURIComponent;
/** `?repo_id=` for a repository other than the primary; nothing when there is none, so single-repository calls are unchanged. */
const repoQuery = (repoId?: string) => (repoId ? `?repo_id=${q(repoId)}` : "");
const base = (projectId: string, number: number) => `/api/projects/${q(projectId)}/pulls/${number}`;

export const pullsApi = {
  /** `rescan` asks the forge programs again, for a Refresh the person pressed. */
  forges: (signal?: AbortSignal, rescan = false) => request<ForgeStatus[]>("GET", `/api/forges${rescan ? "?rescan=true" : ""}`, undefined, signal),
  list: (state: string, query: string, signal?: AbortSignal, options?: RequestOptions, force = false) =>
    request<PullList>("GET", `/api/pulls?state=${q(state)}&q=${q(query)}${force ? "&refresh=1" : ""}`, undefined, signal, options),
  /**
   * What the base list leaves out (reviewers, the viewer's review, comment totals, when
   * opened), for rows already shown, so the list paints first. Pass `fresh` only for a
   * refresh the person started; background refreshes reuse what is known.
   */
  details: (body: PullDetailsRequest, signal?: AbortSignal) =>
    request<PullDetailsResponse>("POST", "/api/pulls/details", body, signal),
  detail: (projectId: string, number: number, signal?: AbortSignal, options?: RequestOptions, force = false) =>
    request<PullRequestDetail>("GET", `${base(projectId, number)}${force ? "?refresh=1" : ""}`, undefined, signal, options),
  checks: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullCheck[]>("GET", `${base(projectId, number)}/checks`, undefined, signal),
  timeline: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullTimelineEvent[]>("GET", `${base(projectId, number)}/timeline`, undefined, signal),
  files: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullFile[]>("GET", `${base(projectId, number)}/files`, undefined, signal),
  /** The files by change kind, with the lines added and deleted. */
  breakdown: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullChangeBreakdown>("GET", `${base(projectId, number)}/breakdown`, undefined, signal),
  /** Every thread of the project that links this pull request. */
  linkedThreads: (projectId: string, number: number, signal?: AbortSignal) =>
    request<Thread[]>("GET", `${base(projectId, number)}/linked-threads`, undefined, signal),
  revisions: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullRevision[]>("GET", `/api/projects/${q(projectId)}/pulls/${number}/revisions`, undefined, signal),
  commitFiles: (projectId: string, sha: string, signal?: AbortSignal) =>
    request<PullFile[]>("GET", `/api/projects/${q(projectId)}/commits/${q(sha)}/files`, undefined, signal),
  conversation: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullConversation>("GET", `${base(projectId, number)}/comments`, undefined, signal),
  comment: (projectId: string, number: number, body: string, replyTo?: string) =>
    request<void>("POST", `${base(projectId, number)}/comments`, { body, ...(replyTo ? { reply_to: replyTo } : {}) }),
  editComment: (projectId: string, number: number, comment: string, kind: string, body: string) =>
    request<void>("PATCH", `${base(projectId, number)}/comments/${q(comment)}`, { kind, body }),
  react: (projectId: string, number: number, comment: string, kind: string, content: string) =>
    request<void>("POST", `${base(projectId, number)}/comments/${q(comment)}/react`, { kind, content }),
  resolve: (projectId: string, number: number, thread: string, resolved: boolean) =>
    request<void>("POST", `${base(projectId, number)}/threads/${q(thread)}/resolve`, { resolved }),
  review: (projectId: string, number: number, event: string, body: string, comments: ReviewLineComment[] = []) =>
    request<void>("POST", `${base(projectId, number)}/review`, { event, body, comments }),
  update: (projectId: string, number: number, change: UpdatePull) =>
    request<void>("PATCH", base(projectId, number), change),
  /** The account setting Remove agent credits on merge as it applies to the project; null where the service does not say. */
  creditsSetting: async (projectId: string, signal?: AbortSignal): Promise<boolean | null> => {
    const rows = await readPreferences(projectId, signal);
    const row = rows.find((one) => one.key === "remove_agent_credits_on_merge");
    return typeof row?.value === "boolean" ? row.value : null;
  },
  /**
   * The merge method the project merges with by default, with the project's own override applied; null
   * for Settings' Ask and for a service that does not list the setting.
   */
  mergeMethodSetting: async (projectId: string, signal?: AbortSignal): Promise<string | null> => {
    const rows = await readPreferences(projectId, signal);
    const value = rows.find((one) => one.key === "pull_request_merge_method")?.value;
    return value === "merge" || value === "squash" || value === "rebase" ? value : null;
  },
  merge: (projectId: string, number: number, options: MergePull) =>
    request<void>("POST", `${base(projectId, number)}/merge`, options),
  action: (projectId: string, number: number, action: string) =>
    request<void>("POST", `${base(projectId, number)}/action`, { action }),
  work: (projectId: string, number: number, engine: string) =>
    request<{ id: string }>("POST", `${base(projectId, number)}/work`, { engine }),
  guide: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullGuideState>("GET", `${base(projectId, number)}/guide`, undefined, signal),
  writeGuide: (projectId: string, number: number, engine: string) =>
    request<{ id: string }>("POST", `${base(projectId, number)}/guide`, { engine }),
  askAboutStop: (projectId: string, number: number, engine: string, stop: number) =>
    request<AskedAboutStop>("POST", `${base(projectId, number)}/guide/ask`, { engine, stop }),
  checkout: (projectId: string, number: number, worktree: boolean) =>
    request<PullCheckout>("POST", `${base(projectId, number)}/checkout`, { worktree }),
  setViewed: (projectId: string, number: number, paths: string[], viewed: boolean) =>
    request<void>("POST", `${base(projectId, number)}/viewed`, { paths, viewed }),
  reviewerCandidates: (projectId: string, signal?: AbortSignal) =>
    request<PullCandidate[]>("GET", `/api/projects/${q(projectId)}/pulls/reviewers`, undefined, signal),
  labelCandidates: (projectId: string, signal?: AbortSignal) =>
    request<PullCandidate[]>("GET", `/api/projects/${q(projectId)}/pulls/labels`, undefined, signal),
  setReviewers: (projectId: string, number: number, add: string[], remove: string[]) =>
    request<void>("POST", `${base(projectId, number)}/reviewers`, { add, remove }),
  setLabels: (projectId: string, number: number, add: string[], remove: string[]) =>
    request<void>("POST", `${base(projectId, number)}/labels`, { add, remove }),
  approveWorkflow: (projectId: string, run: number) =>
    request<void>("POST", `/api/projects/${q(projectId)}/workflows/${run}/approve`),
  closeMany: (projectId: string, numbers: number[]) =>
    request<PullBulkResult[]>("POST", `/api/projects/${q(projectId)}/pulls/close`, { numbers }),
  stack: (projectId: string, number: number, signal?: AbortSignal) =>
    request<PullStack>("GET", `${base(projectId, number)}/stack`, undefined, signal),
  stackAction: (projectId: string, number: number, action: string, method?: string) =>
    request<PullBulkResult[]>("POST", `${base(projectId, number)}/stack`, { action, ...(method ? { method } : {}) }),
  projectPulls: (projectId: string, query: string, signal?: AbortSignal) =>
    request<PullRequest[]>("GET", `/api/projects/${q(projectId)}/pulls?state=open&q=${q(query)}`, undefined, signal),
  /** The thread's pull requests. `repoId` picks one repository of a multi-repository thread; absent is the primary and sends no parameter. */
  linked: (threadId: string, signal?: AbortSignal, repoId?: string) =>
    request<PullRequest[]>("GET", `/api/threads/${q(threadId)}/pulls${repoQuery(repoId)}`, undefined, signal),
  refreshLinked: (threadId: string, repoId?: string) =>
    request<PullRequest[]>("POST", `/api/threads/${q(threadId)}/pulls/refresh${repoQuery(repoId)}`),
  link: (threadId: string, number: number, repoId?: string) =>
    request<void>("POST", `/api/threads/${q(threadId)}/pulls${repoQuery(repoId)}`, { number }),
  unlink: (threadId: string, number: number, repoId?: string) =>
    request<void>("DELETE", `/api/threads/${q(threadId)}/pulls/${number}${repoQuery(repoId)}`),
  /** A review of one of the thread's pull requests, in the repository `repoId` names (absent: the primary). */
  threadReview: (threadId: string, number: number, event: string, body: string, repoId?: string) =>
    request<void>("POST", `/api/threads/${q(threadId)}/pulls/${number}/review${repoQuery(repoId)}`, { event, body, comments: [] }),
  threadMerge: (threadId: string, number: number, options: MergePull, repoId?: string) =>
    request<void>("POST", `/api/threads/${q(threadId)}/pulls/${number}/merge${repoQuery(repoId)}`, options),
  watches: (threadId: string, signal?: AbortSignal) =>
    request<PullWatch[]>("GET", `/api/threads/${q(threadId)}/watches`, undefined, signal),
  watch: (threadId: string, number: number) => request<void>("POST", `/api/threads/${q(threadId)}/pulls/${number}/watch`),
  unwatch: (threadId: string, number: number) =>
    request<void>("DELETE", `/api/threads/${q(threadId)}/pulls/${number}/watch`),
};
