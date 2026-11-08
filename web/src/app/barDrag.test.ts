import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { attachBarDrag, DOUBLE_CLICK_MS, doubleClickTracker } from "./barDrag";

let clock = 0;
let stop = () => {};
const invoke = vi.fn();
const down = (target: Element, x = 100, y = 20, button = 0) =>
  target.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true, clientX: x, clientY: y, button }));
const up = (target: Element, x = 100, y = 20, button = 0) =>
  target.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, cancelable: true, clientX: x, clientY: y, button }));
/** A full click: press and release. */
const click = (target: Element, x = 100, y = 20) => { down(target, x, y); up(target, x, y); };

beforeEach(() => {
  clock = 1000; invoke.mockReset();
  document.body.innerHTML = `<header><div class="layer"></div><button class="toggle">t</button><button class="other">o</button></header>`;
  stop = attachBarDrag(document.querySelector(".layer")!, invoke, () => clock);
});
afterEach(() => stop());
const layer = () => document.querySelector(".layer")!;
const button = (name: string) => document.querySelector(name)!;
const zooms = () => invoke.mock.calls.filter(([command]) => command === "plugin:window|internal_toggle_maximize").length;

it("drags on a press of the empty bar and zooms when the second press at the same spot is released", () => {
  click(layer());
  expect(invoke).toHaveBeenLastCalledWith("plugin:window|start_dragging");
  clock += 200; down(layer(), 102, 21);
  expect(zooms()).toBe(0);
  up(layer(), 102, 21);
  expect(zooms()).toBe(1);
});

it("does not zoom when the pointer moves between the second press and its release, or the release is elsewhere", () => {
  click(layer()); clock += 200; down(layer());
  up(layer(), 160, 20);
  expect(zooms()).toBe(0);
  clock += 1000; click(layer()); clock += 100; down(layer());
  up(button(".toggle"), 100, 20);
  expect(zooms()).toBe(1);
});

it("never zooms for two quick clicks at different spots", () => {
  click(layer(), 100, 20); clock += 200; click(layer(), 300, 20);
  expect(zooms()).toBe(0);
});

it("never zooms when a button is clicked before or after a press of the empty bar", () => {
  click(button(".toggle")); clock += 200; click(layer());
  click(button(".other")); clock += 200; click(layer());
  clock += 2 * DOUBLE_CLICK_MS;
  click(layer()); clock += 100; click(button(".other")); clock += 100; click(layer());
  expect(zooms()).toBe(0);
  expect(invoke).not.toHaveBeenCalledWith("plugin:window|internal_toggle_maximize");
});

it("does not zoom for presses further apart in time than a double click", () => {
  click(layer()); clock += DOUBLE_CLICK_MS + 1; click(layer());
  expect(zooms()).toBe(0);
});

it("a third quick press starts a new pair instead of zooming again", () => {
  click(layer()); clock += 100; click(layer()); clock += 100; click(layer());
  expect(zooms()).toBe(1);
});

it("ignores other mouse buttons", () => {
  down(layer(), 100, 20, 2); clock += 100; down(layer(), 100, 20, 2);
  expect(invoke).not.toHaveBeenCalled();
});

it("tracks time and distance on their own", () => {
  const tracker = doubleClickTracker(() => clock);
  expect(tracker.click(0, 0)).toBe(false);
  clock += 50;
  expect(tracker.click(10, 0)).toBe(false);
  clock += 50;
  expect(tracker.click(10, 1)).toBe(true);
});
