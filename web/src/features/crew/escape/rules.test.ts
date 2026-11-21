import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

/** The Designer's rules for escapes (U2) that a drawn frame still carries. The settings rules moved to the live screen's
 * own test (./live/settings.test.tsx) when `escape-settings` began reading the thresholds. The mark dialog, the dashboard
 * and the counts cards have no read behind them (the report and counts routes and the Review queue are not built), so they
 * stay drawn samples and their rules are asserted here; the v20 line "You can remove it later with Not an escape" was
 * retracted by Crew Designs v79 (there is no undo in v1) and is gone. */
describe("Escape rules (Crew Designs v79)", () => {
  it("a drawn frame never shows a cost figure", async () => {
    const { container } = await openFrame("escape-report");
    expect(container.textContent).toContain("Mark as an escape");
    expect(container.textContent).not.toMatch(/\$\d/);
  });

  it("the mark is the confirm, final in v1, and its Planned fields say so", async () => {
    const { container } = await openFrame("escape-report");
    const text = container.textContent ?? "";
    expect(text).toContain("Your mark is the confirm");
    expect(text).toContain("There is no undo in v1.");
    expect(text).not.toContain("Not an escape");
    expect(text).toContain("How it was found");
    expect(text).toContain("Model: Planned");
  });

  it("the report and the counts read Not measured with Planned, and invent no rate", async () => {
    const dashboard = await openFrame("escape-dashboard");
    expect(dashboard.container.textContent).toContain("No rate is shown in v1");
    expect(dashboard.container.textContent).toContain("Not measured");
    dashboard.unmount();
    const counts = await openFrame("escape-route-counts");
    expect(counts.container.textContent).toContain("Escape counts in web/app · last 60 days");
    expect(counts.container.textContent).toContain("Not measured");
    expect(counts.container.textContent).not.toMatch(/\d+%/);
    counts.unmount();
    const shifted = await openFrame("escape-route-shifted");
    expect(shifted.container.textContent).toContain("Both engines suit the task equally");
    expect(shifted.container.textContent).not.toMatch(/\d+%/);
  });
});
