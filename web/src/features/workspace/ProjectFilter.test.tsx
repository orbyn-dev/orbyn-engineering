import { describe, expect, it } from "vitest";
import { popupPlace, POPUP_WIDTH } from "./ProjectFilter";

describe("popupPlace", () => {
  it("opens under the trigger, left edges aligned", () => {
    expect(popupPlace({ left: 173, bottom: 112 }, 1280)).toEqual({ left: 173, top: 116 });
  });
  it("keeps the popup inside the window, never flush at x=0", () => {
    expect(popupPlace({ left: 1200, bottom: 100 }, 1280).left).toBe(1280 - POPUP_WIDTH - 8);
    expect(popupPlace({ left: -50, bottom: 100 }, 1280).left).toBe(8);
  });
});
