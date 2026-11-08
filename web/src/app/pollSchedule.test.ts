import { expect, it } from "vitest";
import { FOCUS_FULL_AFTER_MS, POLL_MS, quickFocus, quickTick, SLOW_POLL_EVERY } from "./pollSchedule";

it("reads the Git-backed status once in many ticks, never on the ticks between", () => {
  const full = Array.from({ length: SLOW_POLL_EVERY * 2 }, (_, at) => at + 1).filter((tick) => !quickTick(tick));
  expect(full).toEqual([SLOW_POLL_EVERY, SLOW_POLL_EVERY * 2]);
  // Every few minutes, not every few seconds.
  expect(SLOW_POLL_EVERY * POLL_MS).toBeGreaterThanOrEqual(120_000);
});

it("reads it on focus unless it was read within the last minute", () => {
  expect(quickFocus(1_000_000 + 5_000, 1_000_000)).toBe(true);
  expect(quickFocus(1_000_000 + FOCUS_FULL_AFTER_MS, 1_000_000)).toBe(false);
  expect(quickFocus(1_000_000, 0)).toBe(false);
});
