import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

describe("Settings › Crew rows (Crew Designs v23)", () => {
  it("Review capacity and Reviewer backtest are rows that start unset, and each says what it opens", async () => {
    const { container } = await openFrame("settings-crew");
    const rows = [...container.querySelectorAll(".cx-setrow")].map((row) => row.textContent ?? "");
    const capacity = rows.find((row) => row.startsWith("Review capacity"));
    const backtest = rows.find((row) => row.startsWith("Reviewer backtest"));
    expect(capacity).toContain("Open Review capacity.");
    expect(capacity?.endsWith("Not set")).toBe(true);
    expect(backtest).toContain("Open Reviewer backtest.");
    expect(backtest).toContain("Needs a run budget for each engine.");
    expect(backtest?.endsWith("Budget not set")).toBe(true);
  });

  it("the default budget is per engine, each in its own unit", async () => {
    const { container } = await openFrame("settings-crew");
    expect(container.textContent).toContain("Claude Code 1M tokens · Codex 800k tokens");
  });
});
