import { describe, expect, it } from "vitest";
import { addRecent, stepZoom, pageSize } from "./BrowserPanel";

describe("pageSize", () => {
  it("keeps a page within what the service takes", () => {
    expect(pageSize(640.4, 50)).toEqual({ width: 640, height: 100 });
    expect(pageSize(9000, 480)).toEqual({ width: 4096, height: 480 });
  });
});

describe("browser helpers", () => {
  it("steps zoom through its stops from anywhere", () => {
    expect(stepZoom(1, true)).toBe(1.25);
    expect(stepZoom(1, false)).toBe(0.75);
    expect(stepZoom(1.1, true)).toBe(1.25);
    expect(stepZoom(2, true)).toBe(2);
    expect(stepZoom(0.5, false)).toBe(0.5);
  });

  it("keeps recent pages newest first and each once", () => {
    expect(addRecent(["a", "b"], "b")).toEqual(["b", "a"]);
    expect(addRecent(Array.from({ length: 60 }, (_, at) => `u${at}`), "new")).toHaveLength(50);
  });
});
