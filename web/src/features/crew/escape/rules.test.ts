import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

/** The Designer's final rules for escapes (U2), shown as drawn. Values marked Proposed are Engineering's to set. */
describe("Escape rules (Crew Designs v20)", () => {
  it("Create the check asks each time by default, even under Full, and never opens by itself unless turned on", async () => {
    const { container } = await openFrame("escape-settings");
    const text = container.textContent ?? "";
    expect(text).toContain("Ask me each time · default");
    expect(text).toContain("“Open it by itself” is a choice here and is off until you turn it on, under Full as well.");
  });

  it("a check must fail on the bad commit and pass without it, or Crew asks", async () => {
    const { container } = await openFrame("escape-settings");
    expect(container.textContent).toContain("A check is added when it fails on the bad commit and passes without it. If it doesn’t, Crew asks you.");
  });

  it("your own revert is not an escape: Not an escape removes a counted one", async () => {
    const report = await openFrame("escape-report");
    expect(report.container.textContent).toContain("You can remove it later with Not an escape.");
    report.unmount();
    const settings = await openFrame("escape-settings");
    expect(settings.container.textContent).toContain("unless you mark it Not an escape");
  });

  it("the window and the router tie-break are Proposed, not final", async () => {
    const { container } = await openFrame("escape-settings");
    expect(container.textContent).toContain("10 merges · 2× · Proposed");
    expect(container.textContent).toContain("60 days · Proposed");
    expect(container.textContent).toContain("Proposed: Engineering sets the real values");
  });

  it("cost is not part of the record", async () => {
    const { container } = await openFrame("escape-settings");
    expect(container.textContent).toContain("Not part of the record");
  });
});
