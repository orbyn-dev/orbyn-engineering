import { describe, expect, it } from "vitest";
import { openFrame } from "../shared/test-support";

/** The Designer's final rules for review capacity (U3), shown as drawn. Values marked Proposed are Engineering's to set. */
describe("Review capacity rules (Crew Designs v77)", () => {
  it("the limit counts High and Medium changes only, and 8 is an example limit", async () => {
    const { container } = await openFrame("capacity-settings");
    const text = container.textContent ?? "";
    expect(text).toContain("8 changes");
    expect(text).not.toContain("Proposed");
    expect(text).toContain("Crew counts the High and Medium changes that merged and are waiting for you. Low-risk changes are batched, have no deadline and don’t count.");
  });

  it("sampling is off by default and sized one in four, rounded up", async () => {
    const unset = await openFrame("capacity-settings-unset");
    expect(unset.container.textContent).toContain("Review every batch · default");
    expect(unset.container.textContent).toContain("Not set");
    unset.unmount();
    const set = await openFrame("capacity-settings");
    expect(set.container.textContent).toContain("One in four, rounded up, of the low-risk merges in a batch.");
  });

  it("the sample is chosen after the merge and is not given to the worker", async () => {
    const { container } = await openFrame("capacity-settings");
    expect(container.textContent).toContain("Which ones is decided after they merge and is not given to the worker.");
  });

  it("starting a paused task is the person's decision and is recorded", async () => {
    const { container } = await openFrame("capacity-paused-card");
    expect(container.textContent).toContain("Starting it now is your decision and is recorded in the ledger.");
    expect(container.textContent).toContain("your limit is 8");
  });

  it("the limit never stops a merge or running work", async () => {
    const { container } = await openFrame("capacity-over");
    expect(container.textContent).toContain("Running work carries on, and tasks you start and High-priority work are not paused.");
  });

  it("a failed coverage read draws one dash tile, not one per area", async () => {
    const { container } = await openFrame("capacity-coverage-error");
    expect(container.querySelectorAll(".cx-t15tile")).toHaveLength(1);
    expect(container.querySelector(".cx-t15tile")?.textContent).toContain("—");
  });

  it("a map of a few merges draws the gaps banner and no invented “rests on” line", async () => {
    const { container } = await openFrame("capacity-coverage-low");
    expect(container.textContent).toContain("2 of 5 merges aren’t in the map.");
    expect(container.textContent).not.toMatch(/rests on|\bthin\b/i);
  });
});
