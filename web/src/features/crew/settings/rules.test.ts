import { within } from "@testing-library/react";
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

  it("draws the Cursor row as crew-cursor-reviewer-only: a small Map it and the Reviewer only chip whose title is the reason", async () => {
    const { container } = await openFrame("settings-crew");
    const row = [...container.querySelectorAll<HTMLElement>(".cx-setrow")].find((entry) => entry.textContent?.startsWith("Cursor"));
    expect(row).toBeDefined();
    expect(row!.textContent).toContain("Builder · not mapped");
    expect(row!.textContent).not.toContain("Role limit");
    const mapIt = within(row!).getByRole("button", { name: "Map it" });
    expect(mapIt).toHaveClass("cx-sm");
    const chip = within(row!).getByText("Reviewer only");
    expect(chip.closest("[title]")).toHaveAttribute("title", "Not yet confined for writing work");
    expect(within(row!).queryByText("Not yet confined for writing work")).toBeNull();
  });
});
