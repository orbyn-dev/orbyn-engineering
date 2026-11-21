/**
 * MOCK of the issues routes, which are not merged on the service yet. It is never called against the real service: the
 * app's own client reports "nothing" until the routes exist, and only a test or the dev-only preview
 * (`#/preview/issues`) installs this through `setIssuesApi`. Fixtures follow the Engineer frames `t3gap-issue*`; none
 * holds a credential, because the contract has no field for one. Every shape is a guess, see contract.ts.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { Thread } from "../src/api/types.ts";
import {
  TRACKER_KINDS,
  type IssueDetail, type IssueList, type IssueQuery, type IssueRef, type IssueSummary, type IssuesApi, type StartThreadInput, type TrackerKind, type TrackerRow,
} from "../src/features/issues/contract.ts";
import { applyQuery } from "../src/features/issues/model.ts";

const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
const REPO = "orbyn-dev/orbyn-engineering";

const issue = (key: string, title: string, minutes: number, extra: Partial<IssueSummary> = {}): IssueSummary => ({
  tracker: "github", scope: REPO, key, display_key: `#${key}`, title, url: `https://github.com/${REPO}/issues/${key}`,
  author: { login: "riley" }, state: { category: "open", name: "Open" }, created_at: ago(minutes + 2880), updated_at: ago(minutes),
  assignees: [], labels: [], comment_count: 0, ...extra,
});

export function issueFixtures(): IssueSummary[] {
  return [
    issue("412", "Login sends people back to /home", 4, { labels: [{ name: "bug" }, { name: "auth" }], assignees: [{ login: "riley" }], comment_count: 2 }),
    issue("409", "Project icon picker is not reachable by keyboard", 120, { author: { login: "mika" }, labels: [{ name: "a11y" }, { name: "design" }], assignees: [{ login: "mika" }] }),
    issue("405", "Usage cost filters reset on reload", 1440, { author: { login: "sam" }, labels: [{ name: "bug" }] }),
    issue("398", "Calendar feed token should expire", 2880, { author: { login: "sam" }, labels: [{ name: "security" }], assignees: [{ login: "sam" }, { login: "riley" }] }),
    issue("391", "Settings rows jump when a scope changes", 4320, { state: { category: "closed", name: "Closed", close_reason: "completed" }, labels: [{ name: "bug" }] }),
    issue("377", "Offer a dark favicon", 8000, { state: { category: "closed", name: "Closed", close_reason: "not_planned" } }),
    issue("ORB-87", "Show Issues next to Pull requests", 600, { tracker: "linear", scope: "Orbyn team", display_key: "ORB-87", url: "https://linear.app/orbyn/issue/ORB-87", author: { login: "mika" }, labels: [{ name: "design" }] }),
    issue("ORB-91", "Icon picker search is slow", 700, { tracker: "linear", scope: "Orbyn team", display_key: "ORB-91", url: "https://linear.app/orbyn/issue/ORB-91", labels: [{ name: "perf" }] }),
  ];
}

export function trackerFixtures(): TrackerRow[] {
  const connected: Partial<Record<TrackerKind, string>> = { github: REPO, forgejo: "git.orbyn.dev", linear: "Orbyn team" };
  return TRACKER_KINDS.map((tracker) => ({ tracker, connected: tracker in connected, scope: connected[tracker] ?? null }));
}

const refOf = (one: IssueSummary): IssueRef => ({ tracker: one.tracker, scope: one.scope, key: one.key, display_key: one.display_key, title: one.title, state: one.state, labels: one.labels });

export interface IssuesMock extends IssuesApi {
  /** Every write the screens made, in order: `link:<thread>:<key>`, `unlink:<thread>:<key>`, `start:<key>:<project>`. */
  writes: string[];
  /** Make the next read of that kind fail. */
  failNext(what: "list" | "get" | "start" | null): void;
}

export function createIssuesMock(init: {
  trackers?: TrackerRow[] | null; issues?: IssueSummary[]; links?: Record<string, IssueRef[]>; delayMs?: number; viewer?: string;
} = {}): IssuesMock {
  const trackers = init.trackers === undefined ? trackerFixtures() : init.trackers;
  const issues = init.issues ?? issueFixtures();
  let links: Record<string, IssueRef[]> = init.links ?? (issues[0] ? { "thread-1": [refOf(issues[0])] } : {});
  const writes: string[] = [];
  let fail: "list" | "get" | "start" | null = null;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const take = (what: "list" | "get" | "start") => {
    if (fail !== what) return;
    fail = null;
    throw new ApiFailure("tracker_unreachable", "GitHub did not answer.", 502);
  };
  const connectedIssues = () => issues.filter((one) => trackers?.some((row) => row.tracker === one.tracker && row.connected));
  const find = (tracker: TrackerKind, key: string) => connectedIssues().find((one) => one.tracker === tracker && one.key === key);
  return {
    writes,
    failNext: (what) => { fail = what; },
    trackers: async () => trackers,
    list: async (query: IssueQuery): Promise<IssueList> => {
      await wait();
      take("list");
      const pool = connectedIssues().filter((one) => query.tracker === null || one.tracker === query.tracker);
      const open = pool.filter((one) => one.state.category === "open").length;
      return { issues: applyQuery(pool, query, init.viewer ?? "riley"), counts: { open, closed: pool.length - open }, failed: [] };
    },
    get: async (tracker, key): Promise<IssueDetail> => {
      await wait();
      take("get");
      const one = find(tracker, key);
      if (!one) throw new ApiFailure("not_found", "No such issue.", 404);
      return {
        ...one, body: "After sign-in the app opens `/home` even when `next` names another page.\n\nExpected: the page that asked for sign-in.",
        comments: [{ id: "c1", author: { login: "sam" }, body: "Reproduced. The redirect reads `next` but never uses it.", created_at: ago(1440) }, { id: "c2", author: { login: "mika" }, body: "The picker has the same problem when it is opened from a link.", created_at: ago(240) }],
      };
    },
    links: async () => { await wait(); return links; },
    find: async (input) => {
      await wait();
      const text = input.trim().replace(/^#/, "").toLowerCase();
      if (!text) return [];
      return connectedIssues().filter((one) => one.key.toLowerCase() === text || one.title.toLowerCase().includes(text)).map(refOf);
    },
    link: async (threadId, ref) => {
      writes.push(`link:${threadId}:${ref.key}`);
      await wait();
      const one = find(ref.tracker, ref.key);
      if (!one) throw new ApiFailure("not_found", `No issue ${ref.key} in ${ref.scope}.`, 404);
      const held = links[threadId] ?? [];
      if (!held.some((entry) => entry.tracker === ref.tracker && entry.key === ref.key)) links = { ...links, [threadId]: [...held, refOf(one)] };
    },
    unlink: async (threadId, ref) => {
      writes.push(`unlink:${threadId}:${ref.key}`);
      await wait();
      links = { ...links, [threadId]: (links[threadId] ?? []).filter((entry) => !(entry.tracker === ref.tracker && entry.key === ref.key)) };
    },
    startThread: async (ref, input: StartThreadInput) => {
      writes.push(`start:${ref.key}:${input.project_id}`);
      await wait();
      take("start");
      const one = find(ref.tracker, ref.key);
      if (!one) throw new ApiFailure("not_found", "No such issue.", 404);
      const id = `thread-${writes.length + 100}`;
      links = { ...links, [id]: [refOf(one)] };
      return { id, project_id: input.project_id, title: one.title, engine: "claude", model: null, cwd: "/", worktree: null, skill_profile: null, active_run: null, last_run_state: null, created_at: new Date().toISOString(), updated_at: new Date().toISOString() } as Thread;
    },
  };
}
