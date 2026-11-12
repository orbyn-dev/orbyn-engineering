import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

/** The Designer's final rules for the reviewer backtest (U4), shown as drawn. Values marked Proposed are Engineering's to set. */
describe("Reviewer backtest rules (Crew Designs v23)", () => {
  it("a backtest starts only when the person chooses it, and not at all without a budget", async () => {
    const { container } = await openFrame("backtest-settings-unset");
    expect(container.textContent).toContain("Crew starts it when you choose Run a backtest.");
    expect(container.textContent).toContain("No run budget is set, so a backtest can’t start.");
  });

  it("each engine has its own budget in its own unit and the amounts are never added up", async () => {
    const settings = await openFrame("backtest-settings");
    expect(settings.container.textContent).toContain("each engine has a budget in its own unit, never added up: Claude Code 1.5M tokens, Codex 1M tokens and OpenCode 500k tokens.");
    settings.unmount();
    const estimate = await openFrame("backtest-estimate");
    expect(estimate.container.textContent).toContain("Each engine counts in its own unit and the amounts are never added up.");
  });

  it("the history is the last 180 days and only pull requests 30 days old or more", async () => {
    const { container } = await openFrame("backtest-settings-unset");
    expect(container.textContent).toContain("Last 180 days · Proposed");
    expect(container.textContent).toContain("Merged PRs from the last 180 days that are 30 days old or more");
  });

  it("outcomes come from reverts, fixes and the escape log only, and Not an escape counts as none", async () => {
    const { container } = await openFrame("backtest-settings-unset");
    expect(container.textContent).toContain("A PR escaped if it was reverted, or you marked its merge as an escape in the escape log.");
    expect(container.textContent).toContain("A merge you marked Not an escape counts as no escape.");
  });

  it("a rate shows from 5 escapes and from 10 findings, and below that the counts stand alone", async () => {
    const report = await openFrame("backtest-report");
    expect(report.container.textContent).toContain("A rate shows from 5 escapes.");
    expect(report.container.textContent).toContain("A rate shows from 10 findings.");
    report.unmount();
    const low = await openFrame("backtest-low-history");
    expect(low.container.textContent).toContain("A recall rate shows from 5 escapes and a precision rate from 10 findings, so the table shows the counts it rests on.");
  });

  it("an unmatched finding is not called a false alarm", async () => {
    const { container } = await openFrame("backtest-unmatched");
    expect(container.textContent).toContain("Unmatched isn’t the same as wrong");
    expect(container.textContent).toContain("so it doesn’t call a finding a false alarm");
  });

  it("the lane picker's backtest counts are information and do not change what Auto picks", async () => {
    const { container } = await openFrame("backtest-lane-picker");
    expect(container.textContent).toContain("It is information: it doesn’t change which engine Auto picks.");
  });
});
