import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, it, vi, expect } from "vitest";
import { MOTION } from "../../lib/motion";
import { Loading, Splash } from "./states";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("draws the static skeleton only after the wait, with the label read at once", () => {
  const { container } = render(<Loading label="Loading usage" rows={3} />);
  expect(screen.getByRole("status")).toHaveTextContent("Loading usage");
  expect(container.querySelectorAll(".skeleton")).toHaveLength(0);
  act(() => vi.advanceTimersByTime(MOTION.wait));
  expect(container.querySelectorAll(".skeleton")).toHaveLength(3);
});

it("draws the 64 px splash mark only after the wait, with the label read at once", () => {
  const { container } = render(<Splash label="Connecting to Orbyn" />);
  expect(screen.getByRole("status")).toHaveTextContent("Connecting to Orbyn");
  expect(container.querySelector("svg.logo")).toBeNull();
  act(() => vi.advanceTimersByTime(MOTION.wait));
  expect(container.querySelector("svg.logo")).toHaveAttribute("data-logo", "64");
});
