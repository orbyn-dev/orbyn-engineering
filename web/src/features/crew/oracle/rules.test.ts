import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

/** The Designer's final rules for the independent oracle (U5), shown as drawn. Values marked Proposed are Engineering's to set. */
describe("Independent oracle rules (Crew Designs v22)", () => {
  it("the gate fails when the key kills no mutant", async () => {
    const settings = await openFrame("oracle-settings");
    expect(settings.container.textContent).toContain("A key that kills no mutants isn’t an oracle, so it fails the gate and the task doesn’t pass on it.");
    settings.unmount();
    const failed = await openFrame("oracle-gate-failed");
    expect(failed.container.textContent).toContain("0 of 18 mutants killed");
  });

  it("the key is strengthened for two rounds, then Crew asks the person", async () => {
    const settings = await openFrame("oracle-settings");
    expect(settings.container.textContent).toContain("2 rounds · Proposed");
    expect(settings.container.textContent).toContain("After the last round Crew asks you.");
    settings.unmount();
    const needsYou = await openFrame("oracle-needs-you");
    expect(needsYou.container.textContent).toContain("The gate failed after 2 rounds. Crew doesn’t go on without your say.");
    expect(needsYou.container.textContent).toContain("Nothing was merged.");
  });

  it("a key from the builder's own engine is labelled and asked about each time", async () => {
    const { container } = await openFrame("oracle-no-second-engine");
    expect(container.textContent).toContain("Same engine as builder: not independent");
    expect(container.textContent).toContain("Crew asks each time.");
  });

  it("a language with no mutation tool is Not measured, not a pass", async () => {
    const { container } = await openFrame("oracle-not-measured");
    expect(container.textContent).toContain("The key is independent; it is not validated.");
  });

  it("every cost is a count of tokens or mutants, never a dollar figure", async () => {
    for (const name of ["oracle-report", "oracle-key", "oracle-dashboard"]) {
      const { container, unmount } = await openFrame(name);
      expect(container.textContent, name).not.toMatch(/\$/);
      unmount();
    }
  });
});
