import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

/** The Designer's rules for escapes (U2) that a drawn frame still carries. The settings rules moved to the live
 * screen's own test (./live/settings.test.tsx) when `escape-settings` began reading the thresholds; the v20 line
 * "You can remove it later with Not an escape" was retracted by Crew Designs v79 (there is no undo in v1) and is
 * asserted nowhere. The mark dialog's own rules are asserted here once `escape-report` is redrawn to v79. */
describe("Escape rules (Crew Designs v20)", () => {
  it("a drawn frame never shows a cost figure", async () => {
    const { container } = await openFrame("escape-report");
    expect(container.textContent).toContain("Mark as an escape");
    expect(container.textContent).not.toMatch(/\$\d/);
  });
});
