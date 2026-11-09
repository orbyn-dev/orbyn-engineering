import { describe, expect, it } from "vitest";
import { chooseSide, placeFloating } from "./ModelPicker";

describe("picker placement", () => {
  const anchor = { left: 800, right: 880, top: 400, bottom: 428 } as DOMRect;
  const size = { width: 362, height: 249 };

  it("opens below when it fits, keeping inside the window", () => {
    expect(placeFloating(anchor, size, { width: 1000, height: 900 })).toMatchObject({ left: 632, top: 432 });
  });

  it("flips above, then beside when neither fits", () => {
    expect(placeFloating(anchor, size, { width: 1400, height: 600 })).toMatchObject({ left: 800, top: 147 });
    const short = { ...anchor, top: 284, bottom: 312 } as DOMRect;
    expect(placeFloating(short, { width: 362, height: 340 }, { width: 1051, height: 534 }).left).toBe(434);
  });

  it("stays on the side it opened on when the content changes height", () => {
    const short = { ...anchor, top: 284, bottom: 312 } as DOMRect;
    const window = { width: 1051, height: 534 };
    const side = chooseSide(short, 348, window);
    expect(side).toBe("beside");
    const tall = placeFloating(short, { width: 362, height: 348 }, window, side);
    const small = placeFloating(short, { width: 362, height: 120 }, window, side);
    expect(small.left).toBe(tall.left);
  });
});
