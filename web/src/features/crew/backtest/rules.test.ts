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

describe("the running backtest promises no stop (Crew Designs v95)", () => {
  it("shows progress and says it finishes by itself, with no Stop control", async () => {
    const { container } = await openFrame("backtest-running");
    expect(container.textContent).toContain("Crew shows the report when every lane is done.");
    expect(container.textContent).not.toContain("if you stop it");
    expect(container.textContent).not.toContain("labelled Partial");
    expect(container.textContent).not.toContain("or the part that was replayed");
    expect([...container.querySelectorAll("button")].map((button) => button.textContent?.trim())).not.toContain("Stop the backtest");
  });
});

/** The report serves the run, its lanes and their scores, and nothing else. */
const NARROW = ["backtest-narrow-panel", "backtest-narrow-window"] as const;
const CARDS = "div.cx-t14card";

describe("the narrow reviewer backtest shows only what the report serves (Crew Designs v95)", () => {
  it.each(NARROW)("%s draws one run card and one card per lane", async (name) => {
    const { container } = await openFrame(name);
    const cards = [...container.querySelectorAll(CARDS)];
    expect(cards).toHaveLength(4);
    expect(cards[0]?.textContent).toContain("Partly measured");
    expect(cards[0]?.textContent).toContain("Lanes that scored: 2 of 3");
    for (const lane of ["correctness · claude · sonnet-5-5", "correctness · codex · gpt-5-5", "security · opencode · deepseek-v4"]) {
      expect(cards.filter((card) => card.textContent?.includes(lane))).toHaveLength(1);
    }
  });

  it.each(NARROW)("%s shows each engine's tokens on their own, never summed", async (name) => {
    const { container } = await openFrame(name);
    expect(container.textContent).toContain("Used: claude 1.28M tokens · codex 896k · opencode 396k");
    expect(container.textContent).not.toMatch(/\b(?:Total|Summed|Combined)\b/i);
  });

  it.each(NARROW)("%s names the two outcomes the log can and cannot measure", async (name) => {
    const { container } = await openFrame(name);
    expect(container.textContent).toContain("Partly measured");
    expect(container.textContent?.match(/Not measured/g)).toHaveLength(2);
    expect(container.textContent).toContain("Escapes caught: Not measured · the escape log can’t say");
    expect(container.textContent).toContain("Matched an escape: Not measured · the escape log can’t say");
  });

  it.each(NARROW)("%s leaves out what the report does not serve, and keeps replays per lane Planned", async (name) => {
    const { container } = await openFrame(name);
    const text = container.textContent ?? "";
    expect(text).not.toMatch(/\b64\b/);
    expect(text).not.toContain("pull requests replayed");
    expect(text).not.toContain("180");
    expect(text).not.toContain("#392");
    expect(text).not.toMatch(/\bescaped\b/i);
    expect(text.toLowerCase()).not.toContain("vendor");
    for (const display of ["Claude Code", "Anthropic", "OpenAI", "GPT-5.5", "Sonnet 5.5", "OpenCode", "DeepSeek"]) {
      expect(text).not.toContain(display);
    }
    expect(text).not.toContain("Checks");
    expect(text).toContain("Replays per lane are not served.");
    expect(container.querySelector("[title^='Planned: the report route does not serve how many pull requests each lane replayed']")).not.toBeNull();
  });
});
