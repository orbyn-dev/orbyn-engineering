import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSwipeReveal, SWIPE_DISTANCE } from "./swipeReveal";

const touch = (x: number, y = 60, pointerId = 1) => ({ pointerType: "touch", pointerId, clientX: x, clientY: y });

describe("createSwipeReveal", () => {
  const change = vi.fn();
  beforeEach(() => {
    vi.useFakeTimers();
    change.mockClear();
  });
  afterEach(() => vi.useRealTimers());

  it("reveals the strip on a swipe left and puts it away on a swipe right", () => {
    const swipe = createSwipeReveal(change);
    swipe.down(touch(200));
    swipe.move(touch(200 - SWIPE_DISTANCE));
    expect(change).toHaveBeenLastCalledWith(true);
    swipe.end();
    swipe.down(touch(100));
    swipe.move(touch(100 + SWIPE_DISTANCE));
    expect(change).toHaveBeenLastCalledWith(false);
    expect(change.mock.calls).toEqual([[true], [false]]);
  });

  it("ignores a short move, a mostly vertical move (a scroll) and a mouse", () => {
    const swipe = createSwipeReveal(change);
    swipe.down(touch(200));
    swipe.move(touch(200 - SWIPE_DISTANCE + 1));
    swipe.end();
    swipe.down(touch(200));
    swipe.move(touch(200 - SWIPE_DISTANCE, 60 + SWIPE_DISTANCE));
    swipe.end();
    swipe.down({ ...touch(200), pointerType: "mouse" });
    swipe.move({ ...touch(100), pointerType: "mouse" });
    expect(change).not.toHaveBeenCalled();
  });

  it("follows the finger that went down, and a swipe right on a closed strip does nothing", () => {
    const swipe = createSwipeReveal(change);
    swipe.down(touch(200, 60, 1));
    swipe.move(touch(100, 60, 2));
    expect(change).not.toHaveBeenCalled();
    swipe.end();
    swipe.down(touch(100));
    swipe.move(touch(100 + SWIPE_DISTANCE));
    expect(change).not.toHaveBeenCalled();
  });

  it("makes the click that ends a swipe no click on the row, once", () => {
    const swipe = createSwipeReveal(change);
    swipe.down(touch(200));
    swipe.move(touch(100));
    swipe.end();
    expect(swipe.consumeClick()).toBe(true);
    expect(swipe.consumeClick()).toBe(false);
    swipe.down(touch(200));
    swipe.end();
    expect(swipe.consumeClick()).toBe(false);
  });

  it("closes on demand, and only says so when it was open", () => {
    const swipe = createSwipeReveal(change);
    swipe.close();
    expect(change).not.toHaveBeenCalled();
    swipe.down(touch(200));
    swipe.move(touch(100));
    swipe.close();
    expect(change).toHaveBeenLastCalledWith(false);
    expect(swipe.isOpen()).toBe(false);
  });
});
