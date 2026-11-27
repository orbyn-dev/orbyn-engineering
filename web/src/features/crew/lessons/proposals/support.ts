import { vi } from "vitest";
import type { Project } from "../../../../api/types";
import type { Lesson, LessonList } from "../types";
import type { SkillProposal } from "./types";

/** The project every proposals read names, as the mock's first project. */
export const project = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;

/** The design's sample SKILL.md: 14 lines, the three numbered checks and the template's last line. */
export const SKILL_TEXT = [
  "---",
  'name: redirect-checks',
  'description: "Check where a redirect or returnTo link goes before changing code that follows it."',
  "---",
  "",
  "# Redirect checks",
  "",
  "Use when a task changes how the app follows a redirect or a returnTo link.",
  "",
  "1. Keep the target inside the app. A redirect that followed an outside target failed the web/nav checks before.",
  "2. Drop a returnTo that points to another host. A login link that kept one failed the web/nav checks before.",
  "3. Before renaming or removing a helper that builds returnTo links, look for old links that still use it. A helper rename once stopped old returnTo links from redirecting.",
  "",
  "Filled in from a fixed template, using 3 lessons from orbyn-engineering.",
].join("\n");

export const lesson = (over: Partial<Lesson> = {}): Lesson => ({
  id: "les-returnto",
  scope: "repo",
  repo: "p-app",
  source: "fingerprint",
  text: "Checks like a unit test failed on web/nav before: a redirect followed a target outside the app.",
  state: "active",
  hidden_by_cap: false,
  in_next_brief: true,
  edited_by_person: false,
  names_path: false,
  uses: 2,
  left_out_of: 0,
  skill: null,
  evidence: [],
  created_at: "2026-11-07T14:20:00Z",
  updated_at: "2026-11-07T14:20:00Z",
  last_used_at: "2026-11-07T16:05:00Z",
  rev: 1,
  ...over,
});

/** The three repeated failures the sample proposal is based on. */
export const sourceLessons = (): Lesson[] => [
  lesson({ evidence: [{ kind: "task", id: "5b2d8a3" }, { kind: "task", id: "91c4e07" }] }),
  lesson({
    id: "les-navcleanup",
    text: "Checks like a unit test failed on web/nav before: old returnTo links stopped redirecting after a helper was renamed.",
    uses: 1,
    evidence: [{ kind: "task", id: "8c1d4f6" }],
    created_at: "2026-11-07T15:40:00Z",
    updated_at: "2026-11-07T15:40:00Z",
  }),
  lesson({
    id: "les-loginlink",
    text: "Checks like a unit test failed on web/nav before: a login link kept a returnTo from another host.",
    uses: 0,
    last_used_at: null,
    evidence: [{ kind: "task", id: "3e6a9b0" }, { kind: "task", id: "6d2f8a1" }],
    created_at: "2026-11-07T16:30:00Z",
    updated_at: "2026-11-07T16:30:00Z",
  }),
];

export const lessons = (over: Partial<LessonList> = {}): LessonList => ({
  repo: "p-app",
  use_lessons: true,
  brief_cap: { lessons: 5, chars: 1500 },
  counts: { active: 3, disabled: 0, hidden: 0 },
  lessons: sourceLessons(),
  ...over,
});

export const proposal = (over: Partial<SkillProposal> = {}): SkillProposal => {
  const one: SkillProposal = {
    id: "prop-redirect-checks",
    repo: "p-app",
    state: "open",
    text: SKILL_TEXT,
    lessons: [
      { id: "les-returnto", evidence: [{ kind: "task", id: "5b2d8a3" }, { kind: "task", id: "91c4e07" }] },
      { id: "les-navcleanup", evidence: [{ kind: "task", id: "8c1d4f6" }] },
      { id: "les-loginlink", evidence: [{ kind: "task", id: "3e6a9b0" }, { kind: "task", id: "6d2f8a1" }] },
    ],
    can_approve: true,
    skill: null,
    rev: 1,
    created_at: "2026-11-07T16:31:00Z",
    approved_at: null,
    ...over,
  };
  // An approved proposal was written at a time; a dismissed or open one has none.
  return one.state === "approved" && one.approved_at === null
    ? { ...one, approved_at: "2026-11-07T16:33:00Z" }
    : one;
};

/** "never" waits forever; "network" fails the way a lost connection does, so the page cannot tell whether the write was made. */
export type Answer = { status: number; body?: unknown } | "never" | "network";
export type Call = { url: string; init?: RequestInit };
/** What a proposal write answered: the method, the path with its query, and the parsed body when it has one. */
export type Write = { method: string; url: string; body: Record<string, unknown> | undefined };

const json = (answer: Answer): Response => {
  if (answer === "network") throw new TypeError("Failed to fetch");
  if (answer === "never") return new Promise<Response>(() => undefined) as unknown as Response;
  return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
};

/**
 * One fetch stub for the KC9 screens: the two reads they make (one repository's proposals, its lessons), the project list and
 * the Crew preview flag, and each write a person sends. Every other route answers 404, as a service without the routes does.
 */
export function stubFetch(
  answers: {
    preview?: Answer;
    projects?: Answer;
    proposals?: Answer;
    lessons?: Answer;
    write?: (one: Write) => Answer | Promise<Answer>;
  },
  calls: Call[],
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method ?? "GET").toUpperCase();
      calls.push({ url, init });
      const body = init?.body === undefined ? undefined : (JSON.parse(String(init.body)) as Record<string, unknown>);
      if (url.startsWith("/api/projects")) return json(answers.projects ?? { status: 200, body: [project] });
      if (url.includes("/api/crew/preview")) return json(answers.preview ?? { status: 200, body: { enabled: true } });
      if (method !== "GET") {
        return json(answers.write ? await answers.write({ method, url, body }) : { status: 404, body: { code: "not_found", message: "No such route." } });
      }
      if (url.includes("/api/crew/lessons?")) return json(answers.lessons ?? { status: 200, body: lessons() });
      if (url.includes("/api/crew/skill-proposals?")) return json(answers.proposals ?? { status: 200, body: { proposals: [] } });
      return json({ status: 404, body: { code: "not_found", message: "No such route." } });
    }),
  );
}

/** The writes the screens sent, by method. */
export const writes = (calls: readonly Call[], method: string) => calls.filter(({ init }) => (init?.method ?? "GET") === method);
export const bodyOf = (init?: RequestInit) => JSON.parse(String(init?.body)) as Record<string, unknown>;
