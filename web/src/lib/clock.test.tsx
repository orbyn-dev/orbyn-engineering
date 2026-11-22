import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CLOCK_TICK_MS, useClock } from "./clock";

const Age = ({ at }: { at: number }) => <p>{Math.floor((useClock() - at) / 60_000)}</p>;

const setVisibility = (state: "visible" | "hidden") => {
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
  document.dispatchEvent(new Event("visibilitychange"));
};

beforeEach(() => {
  vi.useFakeTimers();
  setVisibility("visible");
});
afterEach(() => {
  vi.useRealTimers();
  Reflect.deleteProperty(document, "visibilityState");
});

it("recomputes every 30 seconds while visible", () => {
  render(<Age at={Date.now()} />);
  expect(screen.getByText("0")).toBeInTheDocument();
  act(() => { vi.advanceTimersByTime(CLOCK_TICK_MS * 2); });
  expect(screen.getByText("1")).toBeInTheDocument();
});

it("pauses while hidden and catches up on return", () => {
  render(<Age at={Date.now()} />);
  setVisibility("hidden");
  expect(vi.getTimerCount()).toBe(0);
  act(() => { vi.advanceTimersByTime(10 * 60_000); });
  expect(screen.getByText("0")).toBeInTheDocument();
  act(() => { setVisibility("visible"); });
  expect(screen.getByText("10")).toBeInTheDocument();
  expect(vi.getTimerCount()).toBe(1);
});

it("runs one timer for any number of screens and clears it on the last unmount", () => {
  const a = render(<Age at={Date.now()} />);
  const b = render(<Age at={Date.now()} />);
  const c = render(<Age at={Date.now()} />);
  expect(vi.getTimerCount()).toBe(1);
  a.unmount();
  b.unmount();
  expect(vi.getTimerCount()).toBe(1);
  c.unmount();
  expect(vi.getTimerCount()).toBe(0);
});
