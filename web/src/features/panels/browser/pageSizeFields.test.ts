import { describe, expect, it } from "vitest";
import { SIZE_RANGE_SENTENCE, lockedPage, parseSize } from "./viewportResize";

describe("parseSize", () => {
  it("takes whole pixels from 100 to 4096", () => {
    expect(parseSize("393")).toBe(393);
    expect(parseSize(" 100 ")).toBe(100);
    expect(parseSize("4096")).toBe(4096);
  });
  it("refuses a size outside the range, a fraction, and anything that is not a number", () => {
    for (const text of ["80", "99", "4097", "0", "-300", "300.5", "", "  ", "12e2", "0x40", "abc", "300px"]) expect(parseSize(text)).toBeNull();
  });
  it("names the range in one sentence", () => {
    expect(SIZE_RANGE_SENTENCE).toBe("Sizes run from 100 to 4096 pixels.");
  });
});

describe("lockedPage", () => {
  const ratio = { width: 393, height: 852 };
  it("keeps the ratio of the moment it was locked when the width changes", () => {
    expect(lockedPage({ width: 393, height: 852 }, { width: 500, height: 852 }, ratio)).toEqual({ width: 500, height: 1084 });
  });
  it("follows the height when only the height changed", () => {
    expect(lockedPage({ width: 393, height: 852 }, { width: 393, height: 1000 }, ratio)).toEqual({ width: 461, height: 1000 });
  });
  it("follows the width when both changed, as a corner does", () => {
    expect(lockedPage({ width: 393, height: 852 }, { width: 600, height: 900 }, ratio)).toEqual({ width: 600, height: 1301 });
  });
  it("stays within what the service takes", () => {
    expect(lockedPage({ width: 393, height: 852 }, { width: 2000, height: 852 }, ratio)).toEqual({ width: 2000, height: 4096 });
  });
});
