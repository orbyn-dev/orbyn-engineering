import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useUi } from "../../product/state/ui";
import { PanelResizer } from "./PanelHost";

beforeEach(() => {
  Object.defineProperty(window, "innerWidth", { configurable: true, writable: true, value: 1400 });
  useUi.setState({ panelWidth: null });
  Object.assign(Element.prototype, { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn(), hasPointerCapture: () => true });
});

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(Element.prototype, "setPointerCapture");
  Reflect.deleteProperty(Element.prototype, "releasePointerCapture");
  Reflect.deleteProperty(Element.prototype, "hasPointerCapture");
  vi.restoreAllMocks();
});

function setup(drawer = false) {
  const view = render(<div data-testid="panel"><PanelResizer drawer={drawer} /></div>);
  const panel = screen.getByTestId("panel");
  vi.spyOn(panel, "getBoundingClientRect").mockImplementation(() => ({ right: 1200, width: Number.parseFloat(panel.style.width) || 500 }) as DOMRect);
  return { ...view, panel, handle: screen.getByRole("separator", { name: "Resize panel" }) };
}

it("prevents selection and saves the bounded width on pointer up", () => {
  const { panel, handle } = setup();
  const down = new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 1, button: 0 });
  fireEvent(handle, down);
  expect(down.defaultPrevented).toBe(true);
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 0 });
  expect(panel.style.width).toBe("840px");
  fireEvent.pointerUp(handle, { pointerId: 1 });
  expect(useUi.getState().panelWidth).toBe(840);
  expect(handle.releasePointerCapture).toHaveBeenCalledWith(1);
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 1000 });
  expect(panel.style.width).toBe("840px");
});

it("ignores non-primary buttons and other pointers", () => {
  const { panel, handle } = setup();
  fireEvent.pointerDown(handle, { pointerId: 1, button: 2 });
  expect(handle.setPointerCapture).not.toHaveBeenCalled();
  fireEvent.pointerDown(handle, { pointerId: 2, button: 0 });
  fireEvent.pointerDown(handle, { pointerId: 3, button: 0 });
  expect(handle.setPointerCapture).toHaveBeenCalledTimes(1);
  fireEvent.pointerMove(handle, { pointerId: 3, clientX: 1000 });
  expect(panel.style.width).toBe("");
  fireEvent.pointerMove(handle, { pointerId: 2, clientX: 1000 });
  expect(panel.style.width).toBe("320px");
});

it.each(["pointerCancel", "lostPointerCapture"] as const)("ends a drag on %s", (eventName) => {
  const { panel, handle } = setup();
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0 });
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 600 });
  fireEvent[eventName](handle, { pointerId: 1 });
  expect(useUi.getState().panelWidth).toBe(600);
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 1000 });
  expect(panel.style.width).toBe("600px");
});

it("releases capture when unmounted and keeps the double-click reset to 380 px", () => {
  const { unmount, handle } = setup();
  useUi.setState({ panelWidth: 600 });
  fireEvent.doubleClick(handle);
  expect(useUi.getState().panelWidth).toBe(380);
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0 });
  unmount();
  expect(handle.releasePointerCapture).toHaveBeenCalledWith(1);
});

it.each([false, true])("restores only a drag-owned width when the divider unmounts (parent changed: %s)", (parentChanged) => {
  const { rerender, panel, handle } = setup();
  panel.style.width = "450px";
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0 });
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 600 });
  if (parentChanged) panel.style.width = "";
  rerender(<div data-testid="panel" />);
  expect(panel.style.width).toBe(parentChanged ? "" : "450px");
});

it("does not save a drag into a different thread layout", () => {
  const { panel, handle } = setup();
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0 });
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 600 });
  act(() => useUi.setState({ layoutKey: "another-thread" }));
  fireEvent.pointerUp(handle, { pointerId: 1 });
  expect(useUi.getState().panelWidth).toBeNull();
  // The destination has no saved width, so the panel shows the default it renders with.
  expect(panel.style.width).toBe("380px");
});

it("leaves a drawer's width to its stylesheet when the destination has no saved width", () => {
  const { panel, handle } = setup(true);
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0 });
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 600 });
  act(() => useUi.setState({ layoutKey: "another-thread" }));
  fireEvent.pointerUp(handle, { pointerId: 1 });
  expect(panel.style.width).toBe("");
});

it("restores the destination width if the pointer ends before layout cleanup", () => {
  const { panel, handle } = setup();
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0 });
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 600 });
  act(() => {
    useUi.setState({ layoutKey: "another-thread", panelWidth: 640 });
    fireEvent.pointerUp(handle, { pointerId: 1 });
  });
  expect(useUi.getState().panelWidth).toBe(640);
  expect(panel.style.width).toBe("640px");
});
