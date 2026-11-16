import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AutopilotLogRow } from "../../../../generated/AutopilotLogRow";
import { DecisionDrawer } from "./DecisionDrawer";
import { relative, rowOf } from "./ledger";
import type { Mode } from "./model";
import { Tripwire } from "./Tripwire";

const log = (over: Partial<AutopilotLogRow>): AutopilotLogRow => ({
  id: 1, task_id: "task-1", crew_id: "crew-1", kind: "allowed", at: "2026-11-07T14:12:00", paths: [], needs_second_engine: false, body: {}, mode: "full", ...over,
});
const RULES = [{ id: "N4", rule: "writing or deleting outside the worktree" }];

function drawer(row: AutopilotLogRow, mode: Mode = "full") {
  const mapped = rowOf(row, RULES, ["/work/orbyn"]);
  if (!mapped) throw new Error("not a request row");
  render(<DecisionDrawer row={mapped} mode={mode} act={{ approveMyself: () => undefined, close: () => undefined }} />);
  return screen.getByRole("complementary");
}

describe("the drawer says only what the row says", () => {
  it("for an always-allowed row asks nobody and claims no summary and no second key", () => {
    const d = drawer(log({ tier: "always_allowed", action_kind: "read", target: "auth/redirect.ts" }));
    expect(d).toHaveTextContent("Not askedThis kind of action is always allowed.");
    expect(d).not.toHaveTextContent("Medium risk");
    expect(d).not.toHaveTextContent("structured summary");
    expect(d).not.toHaveTextContent("both keys");
    expect(within(d).queryByText(/Second key/)).toBeNull();
  });

  it("for a low-tier row, where the lead decided, mentions the summary but not two keys", () => {
    const d = drawer(log({ tier: "low", action_kind: "write", target: "src/a.ts", lead: { approve: true, reason: "In scope." } }));
    expect(d).toHaveTextContent("The lead said yesIn scope.");
    expect(d).toHaveTextContent("The lead saw a structured summary of the action");
    expect(d).not.toHaveTextContent("both keys");
    expect(d).toHaveTextContent("Low risk");
  });

  it("for a medium-tier row says two keys were needed, and shows both answers", () => {
    const d = drawer(log({ tier: "medium", action_kind: "install", target: "zod", lead: { approve: true, reason: "Pinned." }, second: { engine: "codex", approve: true, reason: "Fine." } }));
    expect(d).toHaveTextContent("Medium risk needs both keys.");
    expect(d).toHaveTextContent("Second key: yescodex, a different vendor from the lead.");
  });

  it.each([["full", "Denied under Full, not asked", "Autopilot Full doesn’t stop to ask you."], ["on", "Denied under On, not asked", "The worker is told to take another approach"], ["off", "Denied under Off, not asked", "The worker is told to take another approach"]] as const)("reads the row's own mode when a second key is missing: %s", (mode, title, text) => {
    const d = drawer(log({ kind: "denied", tier: "medium", mode, needs_second_engine: true, action_kind: "install", target: "zod", lead: { approve: true, reason: "Pinned." } }), mode);
    expect(d).toHaveTextContent(title);
    expect(d).toHaveTextContent(text);
    if (mode !== "full") expect(d).not.toHaveTextContent("Autopilot Full");
  });

  it("names no mode when the row has none", () => {
    const d = drawer(log({ kind: "denied", mode: null, needs_second_engine: true, tier: "medium" }));
    expect(d).toHaveTextContent("Denied, not asked");
    expect(d).not.toHaveTextContent("Under Full");
  });

  it("lists a disagreement once, not again as the denial it caused", () => {
    const twin = log({ kind: "denied", tier: "medium", lead: { approve: true, reason: "Yes." }, second: { engine: "codex", approve: false, reason: "No." } });
    expect(rowOf(twin, RULES)).toBeNull();
    expect(rowOf({ ...twin, kind: "disagreement" }, RULES)?.result).toBe("keys_disagree");
  });

  it("leaves out the merge rows and trip wires the ledger also holds", () => {
    for (const kind of ["merged", "merge_queued", "tripwire", "reverted"]) expect(rowOf(log({ kind }), RULES)).toBeNull();
  });
});

describe("paths", () => {
  it("are shown relative to the repo where its root is known", () => {
    expect(relative("/work/orbyn/src/a.ts", ["/work/orbyn", "/work"])).toBe("src/a.ts");
    expect(relative("./src/a.ts", [])).toBe("src/a.ts");
    expect(relative("/elsewhere/a.ts", ["/work/orbyn"])).toBe("/elsewhere/a.ts");
    expect(rowOf(log({ action_kind: "read", target: "/work/orbyn/src/a.ts" }), [], ["/work/orbyn"])?.title).toBe("Read src/a.ts");
  });
});

describe("the trip-wire limits come from the server", () => {
  const stop = { taskId: "t", taskTitle: "t", engine: null, reason: "too_many_denials" as const, denialCount: 5, inARun: false, denials: [], injected: null };

  it("shows the server's numbers", () => {
    render(<Tripwire stop={stop} limits={{ inARow: 5, inARun: 30 }} />);
    expect(screen.getByText(/stops the task on 5 denials in a row/)).toBeVisible();
    expect(screen.getByText(/30 denials in one run/)).toBeVisible();
    expect(screen.getByText(/was denied 5 times in a row/)).toBeVisible();
  });

  it("shows the drawn text without numbers when the server gave none", () => {
    render(<Tripwire stop={stop} />);
    const box = screen.getByText("Why Crew stops").closest("section") as HTMLElement;
    expect(box).toHaveTextContent("The app stops the task on a run of denials in a row");
    expect(box.textContent).not.toMatch(/\d/);
  });
});
