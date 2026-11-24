import type { Evidence, Lesson, LessonDeleted, LessonDetail, LessonFilter, LessonList, LessonScope, LessonSource } from "../src/features/crew/lessons/types.ts";
import type { MockResponse, Scenario } from "./api.ts";

/**
 * The lessons mock (KC5): the routes the web client reads and the writes a person may send, over five sample lessons
 * that follow the design's states. It stands in for the service and ships nowhere: `mock/plugin.ts` loads it in the dev
 * server only, and the production build assertions check that no mock reached the output. Scenarios: "crew-lessons" (five
 * lessons: three in the repository and two for all of them, one disabled and one hidden), "crew-lessons-empty" (nothing
 * learned yet), "crew-lessons-unreadable" (every read answers 500, as a damaged store does) and "crew-lessons-off" (the
 * repository's switch is off, every lesson still readable). Every other scenario answers an empty list.
 */

const T = "2026-11-07T17:00:00Z";

const lesson = (over: Partial<Lesson> = {}): Lesson => ({
  id: "les-1",
  scope: "repo",
  repo: "p-app",
  source: "fingerprint" as LessonSource,
  text: "Checks like repo_required failed on test files before: check the fixture path first.",
  state: "active",
  hidden_by_cap: false,
  in_next_brief: true,
  edited_by_person: false,
  names_path: false,
  uses: 0,
  left_out_of: 0,
  skill: null,
  created_at: "2026-11-07T15:30:00Z",
  updated_at: "2026-11-07T15:30:00Z",
  last_used_at: null,
  rev: 1,
  ...over,
});

const seeds = (): Lesson[] => [
  lesson({
    id: "les-returnto",
    source: "fingerprint",
    text: "Checks like repo_required failed on test files before: put the fixture under tests/ and import it by a relative path.",
    names_path: true,
    uses: 3,
    last_used_at: "2026-11-07T16:10:00Z",
    created_at: "2026-11-07T15:30:00Z",
  }),
  lesson({
    id: "les-cookies",
    source: "person",
    text: "Keep the auth cookie tests in tests/auth, not beside the route.",
    edited_by_person: true,
    names_path: true,
    hidden_by_cap: true,
    in_next_brief: false,
    created_at: "2026-11-06T10:15:00Z",
  }),
  lesson({
    id: "les-session",
    source: "escape",
    text: "A refreshed session must keep a person signed in: check the session route after the refresh.",
    state: "disabled",
    in_next_brief: false,
    uses: 2,
    last_used_at: "2026-11-06T08:01:00Z",
    created_at: "2026-11-05T09:40:00Z",
  }),
  lesson({
    id: "les-return",
    scope: "global",
    repo: null,
    source: "revert",
    text: "A route that reads a query parameter needs a test for a link that still points at its old path.",
    uses: 1,
    last_used_at: "2026-11-05T11:12:00Z",
    created_at: "2026-11-04T18:05:00Z",
  }),
  lesson({
    id: "les-dates",
    scope: "global",
    repo: null,
    source: "person",
    text: "Date formatting goes through the one formatter, never through a second date library.",
    edited_by_person: true,
    created_at: "2026-11-03T12:20:00Z",
  }),
];

/** The design's three repeated-failure lessons (same source, check kind and path class), for the KC9 proposal frames. */
const proposalSeeds = (): Lesson[] => [
  lesson({
    id: "les-returnto",
    source: "fingerprint",
    text: "Checks like a unit test failed on web/nav before: a redirect followed a target outside the app.",
    names_path: true,
    uses: 2,
    last_used_at: "2026-11-07T16:05:00Z",
    created_at: "2026-11-07T14:20:00Z",
  }),
  lesson({
    id: "les-navcleanup",
    source: "fingerprint",
    text: "Checks like a unit test failed on web/nav before: old returnTo links stopped redirecting after a helper was renamed.",
    names_path: true,
    uses: 1,
    last_used_at: "2026-11-07T16:05:00Z",
    created_at: "2026-11-07T15:40:00Z",
  }),
  lesson({
    id: "les-loginlink",
    source: "fingerprint",
    text: "Checks like a unit test failed on web/nav before: a login link kept a returnTo from another host.",
    names_path: true,
    created_at: "2026-11-07T16:30:00Z",
  }),
];

/** The lessons the sample proposal is based on. */
export const PROPOSAL_LESSONS = ["les-returnto", "les-navcleanup", "les-loginlink"] as const;

export function createLessonsMock() {
  let rows: Lesson[] = [];
  let useLessons = true;
  let unreadable = false;
  let added = 0;
  const deleted = new Map<string, { lesson: Lesson; until: string }>();

  function reset(scenario: Scenario) {
    rows =
      scenario === "crew-lessons-empty"
        ? []
        : scenario.startsWith("crew-lessons")
          ? seeds()
          : scenario.startsWith("crew-proposals")
            ? proposalSeeds()
            : [];
    // The proposal frames draw one of the three lessons turned off (the stale card), or all three marked as a skill.
    if (scenario === "crew-proposals-stale")
      rows = rows.map((one) => (one.id === "les-loginlink" ? { ...one, state: "disabled", in_next_brief: false } : one));
    if (scenario === "crew-proposals-approved")
      rows = rows.map((one) => ({ ...one, skill: { name: "redirect-checks" }, in_next_brief: false }));
    useLessons = scenario !== "crew-lessons-off";
    // With the repository switch off, in_next_brief is false for every lesson of it, as the service says.
    if (!useLessons) rows = rows.map((one) => ({ ...one, in_next_brief: false }));
    unreadable = scenario === "crew-lessons-unreadable";
    added = 0;
    deleted.clear();
  }

  const scopeOrder = (one: Lesson) => (one.scope === "repo" ? 0 : 1);
  const newestFirst = (a: Lesson, b: Lesson) => scopeOrder(a) - scopeOrder(b) || b.created_at.localeCompare(a.created_at);

  const counts = () => ({
    active: rows.filter((one) => !one.hidden_by_cap && one.state === "active").length,
    disabled: rows.filter((one) => !one.hidden_by_cap && one.state === "disabled").length,
    hidden: rows.filter((one) => one.hidden_by_cap).length,
  });

  const filtered = (filter: LessonFilter): Lesson[] =>
    rows
      .filter((one) =>
        filter === "hidden" ? one.hidden_by_cap : !one.hidden_by_cap && (filter === "all" || one.state === filter),
      )
      .sort(newestFirst);

  const list = (repo: string, filter: LessonFilter): LessonList => ({
    repo,
    use_lessons: useLessons,
    brief_cap: { lessons: 5, chars: 1500 },
    counts: counts(),
    lessons: filtered(filter),
  });

  // Evidence is ids only, and the ids are the design's own. A repeated failure carries the two tasks behind it, the other
  // sources their escape or merge, and a seeded correction the task it was made on. A lesson added by hand has no task
  // behind it, so it carries none: its id is not in this map.
  const personTask: Record<string, Evidence[]> = {
    "les-cookies": [{ kind: "task", id: "91c4e07" }],
    "les-dates": [{ kind: "task", id: "5d1c8e0" }],
  };
  const detail = (one: Lesson): LessonDetail => ({
    lesson: one,
    evidence:
      one.source === "person"
        ? (personTask[one.id] ?? [])
        : one.source === "fingerprint"
          ? [
              { kind: "task", id: "5b2d8a3" },
              { kind: "task", id: "91c4e07" },
            ]
          : one.source === "escape"
            ? [{ kind: "escape", id: "9be4410" }]
            : [{ kind: "merge", id: "3b9f2c1" }],
  });

  /** The service refuses a write made against another revision. */
  const stale = (one: Lesson | undefined, rev: unknown): MockResponse | null =>
    !one
      ? { status: 404, body: { code: "not_found", message: "No such lesson on this device." } }
      : rev !== one.rev
        ? { status: 409, body: { code: "stale", message: "This lesson changed since you opened it." } }
        : null;

  const bad = (message: string): MockResponse => ({ status: 400, body: { code: "invalid", message } });

  function handle(method: string, pathname: string, search: URLSearchParams, body: unknown): MockResponse | undefined {
    if (!pathname.startsWith("/api/crew/lessons")) return undefined;
    const write = (body ?? {}) as Record<string, unknown>;
    const id = /^\/api\/crew\/lessons\/([A-Za-z0-9_.:-]+)(?:\/(disable|enable|undo|promote|demote))?$/.exec(pathname);
    const suffix = id?.[2] ?? "";

    if (method === "PUT" && pathname === "/api/crew/lessons/settings") {
      if (typeof write.repo !== "string" || !write.repo) return bad("A repository is required.");
      if (typeof write.use_lessons !== "boolean") return bad("use_lessons is true or false.");
      useLessons = write.use_lessons;
      return { status: 200, body: { repo: write.repo, use_lessons: useLessons } };
    }
    if (unreadable) return { status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } };
    if (method === "GET" && pathname === "/api/crew/lessons") {
      const repo = search.get("repo");
      if (!repo) return bad("A repository is required.");
      const filter = search.get("filter");
      const known: LessonFilter[] = ["active", "disabled", "hidden", "all"];
      return { status: 200, body: list(repo, known.find((one) => one === filter) ?? "all") };
    }
    const one = id ? rows.find((entry) => entry.id === id[1]) : undefined;
    if (method === "GET" && id && !suffix) {
      if (!one) return { status: 404, body: { code: "not_found", message: "No such lesson on this device." } };
      return { status: 200, body: detail(one) };
    }
    if (method === "POST" && pathname === "/api/crew/lessons") {
      const scope: LessonScope | null = write.scope === "global" ? "global" : write.scope === "repo" ? "repo" : null;
      const text = typeof write.text === "string" ? write.text.trim() : "";
      if (!scope) return bad("The scope is repo or global.");
      if (!text || text.length > 300) return bad("A lesson is 1 to 300 characters.");
      if (scope === "repo" && (typeof write.repo !== "string" || !write.repo)) return bad("A repository lesson names its repository.");
      const one = lesson({
        id: `les-added-${++added}`,
        scope,
        repo: scope === "repo" ? String(write.repo) : null,
        source: "person",
        text,
        edited_by_person: true,
        created_at: T,
        updated_at: T,
      });
      rows = [one, ...rows];
      return { status: 201, body: one };
    }
    if (!id) return undefined;
    if (method === "PATCH" && pathname === `/api/crew/lessons/${id[1]}`) {
      const refuse = stale(one, write.rev);
      if (refuse) return refuse;
      const text = typeof write.text === "string" ? write.text.trim() : "";
      if (!text || text.length > 300) return bad("A lesson is 1 to 300 characters.");
      if (!one) return { status: 404, body: { code: "not_found", message: "No such lesson on this device." } };
      one.text = text;
      one.edited_by_person = true;
      one.names_path = one.scope === "repo" && /(^|\s)[\w.-]+\/[\w./-]+/.test(text);
      one.rev += 1;
      one.updated_at = T;
      return { status: 200, body: one };
    }
    if (method === "DELETE" && !suffix) {
      const rev = Number(search.get("rev"));
      const refuse = stale(one, rev);
      if (refuse) return refuse;
      if (!one) return { status: 404, body: { code: "not_found", message: "No such lesson on this device." } };
      rows = rows.filter((entry) => entry.id !== one.id);
      const until = new Date(Date.now() + 10_000).toISOString();
      deleted.set(one.id, { lesson: one, until });
      return { status: 200, body: { id: one.id, undo_until: until } satisfies LessonDeleted };
    }
    if (method === "POST" && suffix === "undo") {
      const gone = deleted.get(String(id[1]));
      if (!gone || Date.parse(gone.until) <= Date.now()) return { status: 404, body: { code: "not_found", message: "That delete is final now." } };
      deleted.delete(String(id[1]));
      rows = [gone.lesson, ...rows];
      return { status: 200, body: gone.lesson };
    }
    if (method !== "POST" || !one) return undefined;
    const refuse = stale(one, write.rev);
    if (refuse) return refuse;
    if (suffix === "disable" || suffix === "enable") {
      one.state = suffix === "disable" ? "disabled" : "active";
      one.in_next_brief = one.state === "active" && !one.hidden_by_cap && useLessons && one.skill === null;
    } else if (suffix === "promote") {
      if (one.scope === "global") return bad("This lesson is for all repositories already.");
      if (one.names_path) return bad("This lesson names a path; edit it before promoting it.");
      one.scope = "global";
      one.repo = null;
    } else if (suffix === "demote") {
      if (one.scope === "repo") return bad("This lesson is for one repository already.");
      if (typeof write.repo !== "string" || !write.repo) return bad("A repository is required.");
      one.scope = "repo";
      one.repo = write.repo;
    } else {
      return undefined;
    }
    one.rev += 1;
    one.updated_at = T;
    return { status: 200, body: one };
  }

  /** The lessons a proposal was approved from carry its skill and stop going into briefs (KC9). */
  function approveSkill(name: string, lessonIds: readonly string[]) {
    rows = rows.map((one) => (lessonIds.includes(one.id) ? { ...one, skill: { name }, in_next_brief: false } : one));
  }

  const lessonNow = (id: string): Lesson | undefined => rows.find((one) => one.id === id);

  return { reset, handle, approveSkill, lessonNow };
}
