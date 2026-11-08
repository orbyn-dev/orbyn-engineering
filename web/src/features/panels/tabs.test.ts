import { expect, it } from "vitest";
import type { RightTab } from "../../state/ui";
import { tabSlots, visibleTabs } from "./tabs";

const ids: RightTab[] = ["diff", "files", "browser", "activity", "side", "tools"];

it("counts slots from the room: all of them when they fit with the plus, else one fewer for the more button", () => {
  // Five tabs and the plus need 6 x 32 + 5 x 2 = 202.
  expect(tabSlots(202, 5)).toBe(5);
  expect(tabSlots(201, 5)).toBe(3);
  expect(tabSlots(0, 5)).toBe(1);
});

it("shows the first tabs that fit, and the open one in the last slot when it is hidden", () => {
  expect(visibleTabs(ids, null, null, 3)).toMatchObject({ shown: ["diff", "files", "browser"], hidden: ["activity", "side", "tools"], holder: null });
  expect(visibleTabs(ids, "side", null, 3)).toMatchObject({ shown: ["diff", "files", "side"], hidden: ["browser", "activity", "tools"], holder: "side" });
});

it("keeps a tab picked from the menu in the last slot until the width changes", () => {
  expect(visibleTabs(ids, "diff", "side", 3).shown).toEqual(["diff", "files", "side"]);
  expect(visibleTabs(ids, "diff", "side", 6).shown).toEqual(ids);
});

it("never changes a slot when a visible tab is activated", () => {
  const seen = ids.map((open) => visibleTabs(ids, open, null, 4).shown.slice(0, 3));
  // Activating the first three leaves their positions alone.
  expect(seen.slice(0, 3).every((slots) => slots.join() === "diff,files,browser")).toBe(true);
});

it("does not take the last slot from the open tab for a tab picked earlier", () => {
  const result = visibleTabs(ids, "browser", "activity", 3);
  expect(result.shown).toEqual(["diff", "files", "browser"]);
  expect(result.holder).toBeNull();
});

it("always shows the open tab, once and within the slots, for every width, open tab and earlier pick", () => {
  for (let slots = 1; slots <= ids.length + 1; slots++) {
    for (const active of ids) {
      for (const promoted of [null, ...ids]) {
        const { shown, hidden } = visibleTabs(ids, active, promoted, slots);
        expect(shown).toContain(active);
        expect(new Set(shown).size).toBe(shown.length);
        expect(shown.length).toBe(Math.min(slots, ids.length));
        expect([...shown, ...hidden].sort()).toEqual([...ids].sort());
      }
    }
  }
});
