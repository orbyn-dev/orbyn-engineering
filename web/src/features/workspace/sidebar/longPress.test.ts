import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createLongPress, LONG_PRESS_MS } from "./longPress";
import { DRAG_DISTANCE } from "./pressGesture";

const touch = (x = 40, y = 60, pointerId = 1) => ({ pointerType: "touch", pointerId, clientX: x, clientY: y });

describe("createLongPress", () => {
  const fire = vi.fn();
  beforeEach(() => {
    vi.useFakeTimers();
    fire.mockClear();
  });
  afterEach(() => vi.useRealTimers());

  it("opens after the hold, at the touch point, once", () => {
    const press = createLongPress(fire);
    press.down(touch());
    vi.advanceTimersByTime(LONG_PRESS_MS - 1);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2);
    expect(fire).toHaveBeenCalledWith({ clientX: 40, clientY: 60 });
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it("never starts for a mouse or a short tap", () => {
    const press = createLongPress(fire);
    press.down({ ...touch(), pointerType: "mouse" });
    vi.advanceTimersByTime(2000);
    press.down(touch());
    press.end();
    vi.advanceTimersByTime(2000);
    expect(fire).not.toHaveBeenCalled();
  });

  it("is cancelled by the movement that starts a drag, so 6 to 10 px then a hold opens nothing", () => {
    const press = createLongPress(fire);
    for (const distance of [DRAG_DISTANCE, 8, 10]) {
      press.down(touch(40, 60));
      press.move(touch(40, 60 + distance));
      vi.advanceTimersByTime(LONG_PRESS_MS * 3);
      press.end();
    }
    expect(fire).not.toHaveBeenCalled();
    // Under the threshold the finger is still holding.
    press.down(touch(40, 60));
    press.move(touch(40, 60 + DRAG_DISTANCE - 1));
    vi.advanceTimersByTime(LONG_PRESS_MS + 1);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it("ignores another pointer's movement", () => {
    const press = createLongPress(fire);
    press.down(touch(40, 60, 1));
    press.move(touch(40, 200, 2));
    vi.advanceTimersByTime(LONG_PRESS_MS + 1);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it("swallows the click that ends a long press once, and lets the next one through", () => {
    const press = createLongPress(fire);
    press.down(touch());
    vi.advanceTimersByTime(LONG_PRESS_MS + 1);
    press.end();
    expect(press.consumeClick()).toBe(true);
    expect(press.consumeClick()).toBe(false);
  });

  it("clears its state after the press so the keyboard's context menu key is never skipped", () => {
    const press = createLongPress(fire);
    press.down(touch());
    vi.advanceTimersByTime(LONG_PRESS_MS + 1);
    press.end();
    // No click came (a long press can end without one): the state clears on its own.
    vi.advanceTimersByTime(1);
    expect(press.contextMenu()).toBe("open");
  });

  it("opens a touch contextmenu once: Android's own event before the timer, or after it", () => {
    const early = createLongPress(fire);
    early.down(touch());
    expect(early.contextMenu()).toBe("open");
    vi.advanceTimersByTime(LONG_PRESS_MS * 2);
    expect(fire).not.toHaveBeenCalled();
    expect(early.contextMenu()).toBe("skip");
    const late = createLongPress(fire);
    late.down(touch());
    vi.advanceTimersByTime(LONG_PRESS_MS + 1);
    expect(late.contextMenu()).toBe("skip");
  });

  it("stops its timers when disposed", () => {
    const press = createLongPress(fire);
    press.down(touch());
    press.dispose();
    vi.advanceTimersByTime(2000);
    expect(fire).not.toHaveBeenCalled();
  });
});
