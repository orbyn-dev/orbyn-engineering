import { expect, it, vi } from "vitest";
import { setViewportWidth } from "./test-support";

/**
 * The layout hooks read `window.matchMedia` while a component mounts, so the stand-in jsdom gets
 * has to outlive whatever a test file's own teardown does. It did not: `vi.unstubAllGlobals()`
 * put `undefined` back, and an effect that flushed during that teardown failed with
 * "window.matchMedia is not a function" (issue #1065).
 */
it("keeps the browser stand-ins when a test file unstubs every global", () => {
  vi.unstubAllGlobals();
  expect(typeof window.matchMedia).toBe("function");
  expect(window.matchMedia("(max-width: 599px)").matches).toBe(false);
  expect(window.matchMedia("(prefers-reduced-motion: reduce)").matches).toBe(false);
  expect(typeof window.ResizeObserver).toBe("function");
});

it("gives the plain stand-in back after a test stubs its own, as a file's afterEach does", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: true, addEventListener: () => undefined }));
  expect(window.matchMedia("(min-width: 1160px)").matches).toBe(true);
  vi.unstubAllGlobals();
  expect(window.matchMedia("(min-width: 1160px)").matches).toBe(false);
});

it("still lets a test answer the width queries as a window that wide would", () => {
  setViewportWidth(390);
  expect(window.matchMedia("(max-width: 599px)").matches).toBe(true);
  expect(window.matchMedia("(min-width: 1160px)").matches).toBe(false);
});
