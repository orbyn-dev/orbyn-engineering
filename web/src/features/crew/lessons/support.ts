import { vi } from "vitest";
import type { Project } from "../../../api/types";
import type { Lesson, LessonList } from "./types";

/** The project every lessons read names, as the mock's first project. */
export const project = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;

export const lesson = (over: Partial<Lesson> = {}): Lesson => ({
  id: "les-1",
  scope: "repo",
  repo: "p-app",
  source: "fingerprint",
  text: "Checks like repo_required failed on test files before: check the fixture path first.",
  state: "active",
  hidden_by_cap: false,
  in_next_brief: true,
  edited_by_person: false,
  names_path: false,
  uses: 3,
  left_out_of: 0,
  skill: null,
  created_at: "2026-11-07T15:30:00Z",
  updated_at: "2026-11-07T15:30:00Z",
  last_used_at: "2026-11-07T16:10:00Z",
  rev: 1,
  ...over,
});

export const list = (over: Partial<LessonList> = {}): LessonList => ({
  repo: "p-app",
  use_lessons: true,
  brief_cap: { lessons: 5, chars: 1500 },
  counts: { active: 2, disabled: 0, hidden: 0 },
  lessons: [
    lesson(),
    lesson({
      id: "les-2",
      scope: "global",
      repo: null,
      source: "revert",
      text: "A route that reads a query parameter needs a test for a link that still points at its old path.",
      uses: 0,
      last_used_at: null,
    }),
  ],
  ...over,
});

export type Answer = { status: number; body?: unknown } | "never";
export type Call = { url: string; init?: RequestInit };

const json = (answer: Answer): Response => {
  if (answer === "never") return new Promise<Response>(() => undefined) as unknown as Response;
  return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
};

/** One fetch stub for the lessons screens: answer each route and record every call with its request init. */
export function stubFetch(
  answers: { preview?: Answer; projects?: Answer; list?: Answer; hidden?: Answer; settings?: Answer; detail?: Answer },
  calls: Call[],
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.startsWith("/api/projects")) return json(answers.projects ?? { status: 200, body: [project] });
      if (url.includes("/api/crew/preview")) return json(answers.preview ?? { status: 200, body: { enabled: true } });
      if (url.includes("/api/crew/lessons/settings"))
        return json(answers.settings ?? { status: 200, body: { repo: "p-app", use_lessons: true } });
      if (url.includes("/api/crew/lessons?")) {
        const hidden = url.includes("filter=hidden");
        return json((hidden ? answers.hidden : undefined) ?? answers.list ?? { status: 200, body: list() });
      }
      if (/\/api\/crew\/lessons\/[A-Za-z0-9_.:-]+$/.test(url))
        return json(answers.detail ?? { status: 404, body: { code: "not_found", message: "No such lesson." } });
      return json({ status: 404, body: { code: "not_found", message: "No such route." } });
    }),
  );
}

/** The writes the screens sent, by method. */
export const writes = (calls: readonly Call[], method: string) => calls.filter(({ init }) => (init?.method ?? "GET") === method);
export const bodyOf = (init?: RequestInit) => JSON.parse(String(init?.body)) as Record<string, unknown>;
