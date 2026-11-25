import type { FlowTaskArtifacts } from "../../../../generated/FlowTaskArtifacts";

const COMMIT = "a41c9e2c0ffee0123456789abcdef0123456789a";
const BASE = "0b1e5f00c0ffee0123456789abcdef0123456789b";

/** One task's checks, artifacts and diff: two checks, one naming an artifact, and a change pinned to its base commit. */
export function taskArtifacts(over: Partial<FlowTaskArtifacts> = {}): FlowTaskArtifacts {
  return {
    crew: "crew-redirect",
    task: "task-redirect",
    checks: [
      { id: "check-1", kind: "test", task: "task-redirect", outcome: "pass", exit_code: 0, duration_ms: 4000, commit: COMMIT, artifacts: [{ kind: "log", name: "pnpm-test-auth.log", hash: "sha256:7e1c4b94a" }] },
      { id: "check-2", kind: "lint", task: "task-redirect", outcome: "fail", exit_code: 1, duration_ms: 22000, commit: COMMIT, artifacts: [] },
    ],
    diff: {
      base: BASE,
      files: [
        { path: "auth/redirect.ts", status: "modified", added: 12, removed: 3 },
        { path: "auth/redirect.test.ts", status: "modified", added: 6, removed: 1 },
      ],
      files_total: 2,
      added: 18,
      removed: 4,
      truncated: false,
    },
    diff_absent: null,
    truncated: false,
    ...over,
  };
}

/** The same read answered empty: no check ran, and the task has no checkout to read a change from. */
export function emptyTaskArtifacts(over: Partial<FlowTaskArtifacts> = {}): FlowTaskArtifacts {
  return taskArtifacts({ checks: [], diff: null, diff_absent: "no_checkout", truncated: false, ...over });
}
