import type { FlowTaskDetail } from "../../../generated/FlowTaskDetail";

/** A finished task with hostile text in every field a person or a model wrote. */
export function taskDetail(over: Partial<FlowTaskDetail> = {}): FlowTaskDetail {
  return {
    crew: "crew-redirect",
    row: {
      id: "task-redirect", name: "Use next in the auth redirect", kind: "implement", engine: "claude", owner: "lead", state: "passed", could_not_start: false, round: 2,
      cells: [
        { stage: "checks", state: "done", passed: 3, of: 3 },
        { stage: "code", state: "done", lanes: 1 },
      ],
      created_at: "2026-11-07T12:44:00Z", updated_at: "2026-11-07T14:58:00Z",
    },
    brief: { purpose: "Return people to the page they came from.", files: ["auth/**", "src/login.ts"], budget_turns: 30, budget_minutes: 45 },
    route: { engine: "claude", model: "Sonnet 5.5", tier: "builder" },
    lanes: [{ lane: "security", group: "security", task: "task-sec", round: 2, state: "passed", engine: "codex", model: "GPT-5.5", findings: 1, confirmed: 1, unconfirmed: 0, commit: "a41c9e2" }],
    findings: [
      { id: "f1", lane: "security", round: 1, severity: "high", path: "auth/redirect.ts", line: 18, claim: "next accepts //host as a path", check: "confirmed", status: "fixed", blocks: false },
      { id: "f2", lane: "code", round: 1, severity: "low", path: "auth/redirect.ts", line: 31, claim: "The full next value is logged", check: "unconfirmed", status: "superseded", blocks: false },
    ],
    gates: [{ stage: "finding", decision: "pass", decided_by: "code", at: "2026-11-07T14:50:00Z" }],
    rounds: [{ n: 1, task: "task-redirect", state: "failed" }, { n: 2, task: "task-redirect", state: "passed" }],
    merge: { state: "merged", risk: "low", reasons: [], mode: "balanced" },
    watch: "healthy",
    you: null,
    truncated: false,
    ...over,
  };
}

export const HOSTILE = `<img src=x onerror=alert(1)> "><script>alert(2)</script> [click](https://evil.example/x) @octocat see #123 and https://evil.example/path www.evil.example`;
