import { describe, expect, it } from "vitest";
import { UNAVAILABLE_REASON } from "../autopilot/review/Parts";
import { openFrame } from "../shared/test-support";
import { FLOW_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "flow-all": ["Move session helpers to auth/session.ts", "implement · Claude Code · owner Lead", "Review the diff for open redirects", "Rename the legacy returnTo helper"],
  "flow-task": ["Tracing the next parameter into the redirect response", "Reading auth/redirect.ts and its 3 callers", "Codex · GPT-5.5 · lead tier", "OpenCode · DeepSeek V4"],
  "flow-task-loop": ["1 confirmed finding", "After the fix", "1 confirmed"],
  "flow-check-failed": ["auth/redirect.test.ts: expected “/” but got “//evil.com”", "Check 3 of 3 failed: npm test auth", "Sent to Build", "After checks"],
  "flow-artifact-diff": ["Diff · Build → Checks", "2 files · +18 −4", "crew/redirect-impl", "Open the diff"],
  "flow-artifacts": ["auth/redirect.test.ts:12 · the example uses //evil.com", "The listed tests pass. Not that other callers work.", "Use next when it points inside the app", "2 lanes · comments link back here"],
  "flow-gates": ["A finding was reproduced with a test that fails (exit 1) on a41c9e2.", "3 of 3 checks ran on commit a41c9e2 and exited 0.", "A confirmed finding. Nothing blocks without one.", "You. Under Full, the queue decides by code."],
  "flow-replay": ["Correctness has read 6 of 9 changed hunks. This is a replay: nothing here can be changed.", "Lane Correctness started on Codex · GPT-5.5 · read-only checkout a41c9e2", "3 of 3 passed on a41c9e2 · decided by code", "Replay · 18:40 of 41:06"],
  "flow-running": ["Findings so far", "Different engine from the author", "Open the run", "Stop this lane"],
  "flow-lanes-many": ["Accessibility", "Click a line to open what it carries", "a confirmed finding", "Gate decided by:"],
  "flow-long-loop": ["4 earlier findings, each confirmed and sent back once", "Click a round to see its graph", "F1 returned after the refactor", "next survives a failed login"],
  "flow-skipped": ["skipped: docs only (no auth, key, network or dependency files changed)", "Nothing to check: no review reported a finding", "built in · for docs-only diffs", "built in · optional"],
  "flow-offline": ["Claude Code 412k of 1M tokens · Codex not reported while offline · 2h 14m", "Not signed in", "Click a line to open what it carries", "a confirmed finding"],
  "flow-pr-review": ["→ back to Build when confirmed", "Dashed lanes: on PR #482", "PR findings", "Click a line to open what it carries"],
  "flow-you-ready": ["Merged by Crew at 14:32 on Sat 7 Nov 2026 · Medium risk", "You: review what reached main", "Review by Sun 8 Nov, 14:32", "re-checked combined"],
  "flow-you-rule": ["Merged by Crew at 14:12 on Sat 7 Nov 2026", "Merged under a standing rule", "Where this does not apply", "Checks, Code review"],
  "flow-you-blocked": ["4 of 5 stages before the merge are not green yet", "Performance still reviewing", "Nothing to review yet", "running · 1m 40s"],
  "flow-you-waiting-on": ["High risk: waiting for you before the merge", "Not verified: key rotation under load.", "Autopilot is On for this repo", "auth/login.ts, keys/rotate.ts"],
  "flow-you-off": ["A documentation change. Shown so you can review quickly; it does not change what waits.", "Every change waits for you before the merge", "Autopilot is Off for this repo", "Approve, don’t merge"],
  "flow-narrow-panel": ["Open in Crew", "Performance · OpenCode", "Correctness · Codex", "Docs · OpenCode"],
  "flow-narrow-window": ["Performance · OpenCode", "Correctness · Codex", "Docs · OpenCode", "After the merge"],
  "flow-artifact-pruned": ["Check log · npm test auth · a41c9e2", "Wed 28 Oct 2026, after 30 days", "exit 0 · 21 s · commit a41c9e2", "Artifact pruned · hash kept"],
};

const USAGE_LINE = "Claude Code 412k of 1M tokens \u00b7 Codex 62% of its weekly limit \u00b7 2h 14m";
const OFFLINE_USAGE_LINE = "Claude Code 412k of 1M tokens \u00b7 Codex not reported while offline \u00b7 2h 14m";
/** The two narrow-window frames and the panel have no usage line in the design. */
const NO_USAGE_LINE = new Set(["flow-narrow-panel", "flow-narrow-window"]);

describe("Flow frames (Crew Designs)", () => {
  it("builds every frame of the slice", () => {
    expect(new Set(Object.keys(SHOWS))).toEqual(new Set(FLOW_FRAMES));
  });

  it.each(FLOW_FRAMES)("#/preview/crew/%s draws its state and makes no request", async (name) => {
    const { container, fetchSpy } = await openFrame(name);
    for (const text of SHOWS[name] ?? []) expect(container.textContent).toContain(text);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it.each(FLOW_FRAMES)("#/preview/crew/%s: every control is disabled and says why", async (name) => {
    const { container } = await openFrame(name);
    const buttons = container.querySelectorAll<HTMLButtonElement>("button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const control of buttons) {
      expect(control).toBeDisabled();
      expect(control).toHaveAttribute("aria-description", UNAVAILABLE_REASON);
    }
    expect(container.querySelectorAll("a[href], input, select, textarea")).toHaveLength(0);
  });

  it.each(FLOW_FRAMES)("#/preview/crew/%s: no dollar figure and the per-engine usage line", async (name) => {
    const { container } = await openFrame(name);
    expect(container.textContent).not.toMatch(/\$\d/);
    const usage = container.querySelector(".cx-cwtabs > .cx-end");
    if (NO_USAGE_LINE.has(name)) expect(usage).toBeNull();
    else expect(usage?.textContent).toBe(name === "flow-offline" ? OFFLINE_USAGE_LINE : USAGE_LINE);
  });

  it("tells frames that share the lane chrome apart by their own body", async () => {
    const task = await openFrame("flow-task");
    for (const text of [...SHOWS["flow-artifact-diff"]!, ...SHOWS["flow-running"]!].filter((t) => !SHOWS["flow-task"]!.includes(t))) {
      if (["Diff · Build → Checks", "Open the diff", "Findings so far", "Open the run", "Stop this lane"].includes(text)) expect(task.container.textContent).not.toContain(text);
    }
  });

  it("draws the narrow variants at the designed widths", async () => {
    const panel = await openFrame("flow-narrow-panel");
    expect(panel.container.querySelector<HTMLElement>(".cx-frame.cx-panel")?.style.width).toBe("320px");
    panel.unmount();
    const narrow = await openFrame("flow-narrow-window");
    expect(narrow.container.querySelector<HTMLElement>(".cx-frame")?.style.width).toBe("640px");
  });
});
