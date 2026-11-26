import { describe, expect, it } from "vitest";
import { branchNameSeed } from "./draftHelpers";

describe("the draft's branch-name seed", () => {
  it("normalizes the typed goal and cuts it at 50 characters plus an ellipsis", () => {
    expect(branchNameSeed("  Add   the login\ntimeout  ")).toBe("Add the login timeout");
    expect(branchNameSeed("Short goal")).toBe("Short goal");
    expect(branchNameSeed("x".repeat(50))).toBe("x".repeat(50));
    expect(branchNameSeed("x".repeat(60))).toBe(`${"x".repeat(50)}...`);
    expect(Array.from(branchNameSeed("🎉".repeat(60)))).toHaveLength(53);
  });
});
