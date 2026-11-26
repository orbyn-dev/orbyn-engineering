import { describe, expect, it } from "vitest";
import { draftTitle } from "./draftHelpers";

describe("the draft's title seed", () => {
  it("normalizes the typed goal and keeps it at 50 characters", () => {
    expect(draftTitle("  Add   the login\ntimeout  ")).toBe("Add the login timeout");
    expect(draftTitle("Short goal")).toBe("Short goal");
    expect(draftTitle("x".repeat(60))).toBe(`${"x".repeat(50)}...`);
    expect(Array.from(draftTitle("🎉".repeat(60)))).toHaveLength(53);
  });
});
