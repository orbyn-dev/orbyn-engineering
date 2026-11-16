import { describe, expect, it } from "vitest";
import type { RightTab } from "../../../product/state/ui";
import { dropGap, landing, moveIndex, slotShift } from "./tabDrag";

describe("where a dragged tab lands", () => {
  it("counts the slot centres the pointer has passed, and lands one earlier when it moved right of itself", () => {
    const centers = [16, 50, 84, 118];
    expect(dropGap(centers, 5)).toBe(0);
    expect(dropGap(centers, 60)).toBe(2);
    expect(dropGap(centers, 400)).toBe(4);
    expect(landing(1, 0)).toBe(0);
    expect(landing(1, 1)).toBe(1);
    expect(landing(1, 2)).toBe(1);
    expect(landing(1, 4)).toBe(3);
  });

  it("slides only the slots between the old place and the new one, one pitch toward the gap", () => {
    expect([0, 1, 2, 3].map((at) => slotShift(at, 0, 2))).toEqual([0, -34, -34, 0]);
    expect([0, 1, 2, 3].map((at) => slotShift(at, 3, 1))).toEqual([0, 34, 34, 0]);
    expect([0, 1, 2, 3].map((at) => slotShift(at, 2, 2))).toEqual([0, 0, 0, 0]);
  });

  it("gives the store the index in the whole row, past tabs that are closed or folded away", () => {
    const order: RightTab[] = ["diff", "browser", "activity", "side", "tools", "file:%5B%22t%22%2C%22a.ts%22%5D"];
    // Tools is closed and the file is folded: only diff, browser, activity and side are shown.
    const shown: RightTab[] = ["diff", "browser", "activity", "side"];
    expect(moveIndex(order, shown, "side", 0)).toBe(0);
    expect(moveIndex(order, shown, "diff", 3)).toBe(3);
    expect(moveIndex(order, shown, "browser", 2)).toBe(2);
    expect(moveIndex(order, shown, "activity", 1)).toBe(1);
  });
});
