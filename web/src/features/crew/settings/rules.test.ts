import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

describe("Settings › Crew rows (Crew Designs v23)", () => {
  it("draws the four entry rows of Crew Designs v87, each with its Open button and no stored value", async () => {
    const { container } = await openFrame("settings-crew");
    const rows = [...container.querySelectorAll(".cx-setrow")].map((row) => row.textContent ?? "");
    const row = (title: string) => rows.find((text) => text.startsWith(title)) ?? "";
    expect(row("Review capacity").endsWith("Open Review capacity")).toBe(true);
    // Only the Escapes row is a plain "Open"; its accessible name keeps the page it opens.
    expect(row("Escapes").endsWith("Open Escapes")).toBe(false);
    expect(row("Escapes").endsWith("Open")).toBe(true);
    expect(container.querySelector('button[aria-label="Open Escapes"]')).not.toBeNull();
    expect(row("Escapes")).not.toContain("Defaults");
    expect(row("Independent oracle")).toContain("Planned");
    expect(row("Independent oracle")).not.toContain("Another engine · gate on");
    expect(row("Reviewer backtest")).toContain("The live runner is not approved, so a backtest can’t start yet.");
    expect(row("Reviewer backtest")).not.toContain("Budget not set");
    expect(row("Reviewer backtest").endsWith("Open Reviewer backtest")).toBe(true);
  });

  it("the default budget is per engine, each in its own unit", async () => {
    const { container } = await openFrame("settings-crew");
    expect(container.textContent).toContain("Claude Code 1M tokens · Codex 800k tokens");
  });

  it("has no default budget row for background crews: a background crew's budget is set when it is created", async () => {
    const { container } = await openFrame("settings-crew");
    expect(container.textContent).not.toContain("Default budget for background crews");
    expect(container.textContent).not.toContain("Claude Code 300k tokens · Codex 250k tokens");
  });
});
