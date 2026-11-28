import type { FlowTaskArtifacts } from "../../../../generated/FlowTaskArtifacts";
import type { FlowTaskDetail } from "../../../../generated/FlowTaskDetail";
import { taskDetail } from "../../runpage/fixture";

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

/** One finished task's gates: code and a person at the checks, a confirmed finding, and code at a review lane. */
export function taskGates(over: Partial<FlowTaskDetail> = {}): FlowTaskDetail {
  return taskDetail({
    gates: [
      { stage: "checks", decision: "pass", decided_by: "code", lane: null, reason: "3 of 3 checks ran on the commit and passed", at: "2026-11-07T14:10:00Z" },
      { stage: "checks", decision: "pass", decided_by: "person", lane: null, reason: "A person checked this, and it holds.", at: "2026-11-07T14:20:00Z" },
      { stage: "finding_check", decision: "confirmed", decided_by: "confirmed_finding", lane: null, reason: "A repro failed on the commit, so the finding blocks.", at: "2026-11-07T14:25:00Z" },
      { stage: "review", decision: "pass", decided_by: "code", lane: "security", reason: "This lane read the current head and no finding holds it.", at: "2026-11-07T14:30:00Z" },
    ],
    ...over,
  });
}

/** The same gates while the task still runs: a reason the service leaked for a task that is not over. */
export function unfinishedTaskGates(over: Partial<FlowTaskDetail> = {}): FlowTaskDetail {
  const detail = taskGates();
  return taskGates({
    row: { ...detail.row, state: "verifying" },
    gates: detail.gates.map((gate) => ({ ...gate, reason: "A reason for a task that is not over" })),
    ...over,
  });
}
