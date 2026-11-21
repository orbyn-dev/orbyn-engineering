import { vi } from "vitest";
import type { Project } from "../../../../api/types";
import type { EscapeCheckRow } from "../../../../generated/EscapeCheckRow";
import type { EscapeDetail } from "../../../../generated/EscapeDetail";
import type { EscapeLog } from "../../../../generated/EscapeLog";
import type { EscapePossible } from "../../../../generated/EscapePossible";
import type { EscapeRow } from "../../../../generated/EscapeRow";

/** The project every escape read names, as the mock's first project. */
export const project = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;

export const row = (over: Partial<EscapeRow> = {}): EscapeRow => ({
  id: "esc-1",
  project: "p-app",
  merge_task_id: "task-1",
  source: "revert",
  state: "possible",
  gap: null,
  gap_by: null,
  engine: "claude",
  model: null,
  areas: ["web/auth"],
  lane: null,
  reason: "Old returnTo links stopped redirecting",
  line: "Possible escape: the merge was reverted. A person has not confirmed it.",
  found_unix: 1_762_500_000,
  decided_unix: null,
  decided_by: null,
  ...over,
});

/** The check of the design's samples: the return-to check as the person and the core leave it. */
export const checkRow = (over: Partial<EscapeCheckRow> = {}): EscapeCheckRow => ({
  state: "proposed",
  path: "tests/auth/return-to.test.ts",
  asserts: "An old returnTo link still redirects to its path",
  run: "npm test -- tests/auth/return-to.test.ts",
  fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a",
  task_id: null,
  proof: null,
  note: null,
  attempts: 1,
  diff_commit: null,
  diff_hash: null,
  diff_paths: [],
  awaiting_approval: false,
  approved: false,
  line: "Proposed: add tests/auth/return-to.test.ts. Nothing runs until a person accepts.",
  proposed_unix: 1_762_503_000,
  updated_unix: 1_762_503_000,
  ...over,
});

export const log = (over: Partial<EscapeLog> = {}): EscapeLog => ({
  project: "p-app",
  as_of: 1_762_536_000,
  window_days: 60,
  merges: 38,
  possible: 2,
  confirmed: 3,
  dismissed: 1,
  low_sample: false,
  min_merges: 10,
  engines: [
    { engine: "claude", merges: 21, possible: 1, confirmed: 2, dismissed: 1, low_sample: false },
    { engine: "codex", merges: 12, possible: 1, confirmed: 1, dismissed: 0, low_sample: false },
    { engine: "opencode", merges: 5, possible: 0, confirmed: 0, dismissed: 0, low_sample: true },
  ],
  total: 6,
  rows: [
    row({ id: "esc-1", state: "confirmed", source: "revert", gap: "test_gap", gap_by: "person", decided_unix: 1_762_501_000, decided_by: "person" }),
    row({ id: "esc-2", state: "confirmed", source: "person_marked", gap: "tooling_gap", gap_by: "person", decided_unix: 1_762_501_000, decided_by: "person", engine: "codex", reason: "The legacy login route lost its rate limit" }),
    row({ id: "esc-3", state: "dismissed", source: "post_merge_check", decided_unix: 1_762_501_000, decided_by: "person", reason: "A repository check failed on this device after the merge, and the merge was not reverted." }),
  ],
  ...over,
});

export const possible = (over: Partial<EscapePossible> = {}): EscapePossible => ({
  project: "p-app",
  as_of: 1_762_536_000,
  total: 2,
  rows: [
    row({ id: "esc-4", source: "follow_up_fix", engine: "codex", areas: ["web/app"], reason: "A later fix commit changed lines this merge added. It may be ordinary follow-up work." }),
    row({ id: "esc-5", source: "post_merge_check", reason: "A repository check failed on this device after the merge, and the merge was not reverted." }),
  ],
  ...over,
});

export const detail = (over: Partial<EscapeDetail> = {}): EscapeDetail => ({
  escape: row({ id: "esc-1", state: "confirmed", gap: null, source: "revert", decided_unix: 1_762_501_000, decided_by: "person" }),
  events: [
    { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:8e21d47", at: 1_762_500_000 },
    { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:41", at: 1_762_501_000 },
  ],
  truncated: false,
  check: null,
  ...over,
});

export type Answer = { status: number; body?: unknown } | "never";

const json = (answer: Answer): Response => {
  if (answer === "never") return new Promise<Response>(() => undefined) as unknown as Response;
  return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
};

/** The calls the screens made, with the request init, so a test can say which write was sent. */
export type Call = { url: string; init?: RequestInit };

/** One fetch stub for the escape screens: answer each route from `answers` and record every call. */
export function stubFetch(
  answers: { preview: Answer; projects: Answer; log: Answer; logConfirmed: Answer; possible: Answer; detail: Answer; decide: Answer; gap: Answer; check?: Answer; thresholds?: Answer },
  calls: Call[],
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.startsWith("/api/projects")) return json(answers.projects);
      if (url.includes("/api/crew/preview")) return json(answers.preview);
      if (url.includes("/api/crew/escapes/thresholds"))
        return json(answers.thresholds ?? { status: 200, body: { report_days: 60, follow_up_days: 14, min_merges: 10, rate_multiple: 2, stored: false } });
      if (url.includes("/api/crew/escapes/possible")) return json(answers.possible);
      if (url.includes("/api/crew/escapes?") && url.includes("state=confirmed")) return json(answers.logConfirmed);
      if (url.includes("/api/crew/escapes?")) return json(answers.log);
      // The five check routes: one answer serves them all, and a test that forgets it sees a refused route, never a pass.
      if (url.includes("/check")) return json(answers.check ?? { status: 404, body: { code: "not_found", message: "No such route." } });
      if (url.includes("/confirm")) return json(answers.decide);
      if (url.includes("/dismiss")) return json(answers.decide);
      if (url.includes("/gap")) return json(answers.gap);
      if (/\/api\/crew\/escapes\/[A-Za-z0-9_.:-]+$/.test(url)) return json(answers.detail);
      return json({ status: 404, body: { code: "not_found", message: "No such route." } });
    }),
  );
}

export const posts = (calls: readonly Call[]) => calls.filter(({ init }) => (init?.method ?? "GET") === "POST");
export const post = (calls: readonly Call[]): Call => {
  const [sent] = posts(calls);
  if (!sent) throw new Error("no write was sent");
  return sent;
};
export const bodyOf = (init?: RequestInit) => JSON.parse(String(init?.body)) as Record<string, unknown>;
