import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import type { PreviewTab } from "../../../api/types";
import { useUi } from "../../../product/state/ui";
import { BrowserPanel } from "./BrowserPanel";
import { PreviewFloat } from "../preview/PreviewFloat";

const STAGE = { width: 800, height: 600 };

/** A socket that hands out frames only when the test asks for one. */
class FakeSocket {
  static last: FakeSocket | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor(readonly url: string) {
    FakeSocket.last = this;
  }
  close() {}
}

const previewTab = (over: Partial<PreviewTab> = {}): PreviewTab => ({
  id: "tab-1",
  thread_id: "t",
  url: "http://localhost:3000/",
  control: "none",
  recording: false,
  viewers: 1,
  width: 1280,
  height: 800,
  can_go_back: false,
  can_go_forward: false,
  title: "Local",
  zoom: 1,
  appearance: "system",
  mobile: false,
  private: false,
  profile: null,
  opened_by: "person",
  crashed: false,
  ...over,
});

const domRect = (width: number, height: number): DOMRect =>
  ({ width, height, top: 0, left: 0, right: width, bottom: height, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;

function serve() {
  const calls: { method: string; path: string; body: Record<string, unknown> | undefined }[] = [];
  /** Set by a test that wants the next resize refused, as the service may. */
  const fail = { resize: false };
  let tab = previewTab();
  vi.stubGlobal("fetch", async (input: string, init?: RequestInit) => {
    const method = init?.method ?? "GET";
    const body = typeof init?.body === "string" ? (JSON.parse(init.body) as Record<string, unknown>) : undefined;
    calls.push({ method, path: input, body });
    const json = (value: unknown) => new Response(JSON.stringify(value), { status: 200 });
    if (method === "GET" && input.endsWith("/previews")) return json([tab]);
    if (method === "POST" && input.endsWith("/resize")) {
      if (fail.resize) {
        fail.resize = false;
        return new Response(JSON.stringify({ code: "refused", message: "That size is refused." }), { status: 500 });
      }
      tab = { ...tab, ...(body as { width: number; height: number }) };
      return json(tab);
    }
    if (method === "POST" && input.endsWith("/view")) {
      tab = { ...tab, mobile: Boolean(body?.mobile) };
      return json(tab);
    }
    return json({});
  });
  return { calls, fail };
}

const resizes = (calls: { method: string; path: string; body: Record<string, unknown> | undefined }[]) =>
  calls.filter((call) => call.method === "POST" && call.path.endsWith("/resize")).map((call) => call.body);

async function sendFrame() {
  await waitFor(() => expect(FakeSocket.last?.onmessage).toBeTypeOf("function"));
  act(() => FakeSocket.last?.onmessage?.({ data: "frame" }));
}

/** Renders the panel, opens its device toolbar and picks a phone. */
async function openToolbar(user: ReturnType<typeof userEvent.setup>) {
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(domRect(STAGE.width, STAGE.height));
  render(<BrowserPanel threadId="t" />);
  await screen.findByRole("button", { name: "More page actions" });
  await sendFrame();
  await screen.findByRole("img", { name: /^Page at/ });
  await user.click(screen.getByRole("button", { name: "More page actions" }));
  await user.click(await screen.findByRole("menuitem", { name: "Show device toolbar" }));
  await user.selectOptions(screen.getByRole("combobox", { name: "Device" }), "phone");
  await waitFor(() => expectSize(393, 852));
}

/** What the toolbar's two fields read: the page's real size. */
const expectSize = (width: number, height: number) => {
  expect(screen.getByRole("spinbutton", { name: "Width" })).toHaveValue(String(width));
  expect(screen.getByRole("spinbutton", { name: "Height" })).toHaveValue(String(height));
};

/** jsdom has no pointer capture, so the test provides it and watches the calls. */
function stubPointerCapture() {
  const taken = vi.fn();
  const given = vi.fn();
  Object.assign(Element.prototype, { setPointerCapture: taken, releasePointerCapture: given });
  return { taken, given };
}

afterEach(() => {
  vi.restoreAllMocks();
  FakeSocket.last = null;
  Reflect.deleteProperty(Element.prototype, "setPointerCapture");
  Reflect.deleteProperty(Element.prototype, "releasePointerCapture");
});

it("resizes the page once from a dragged edge, then shows it as custom", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  const capture = stubPointerCapture();
  await openToolbar(user);
  const before = resizes(calls).length;

  const handle = screen.getByRole("button", { name: /Resize the page from the right edge/ });
  fireEvent.pointerDown(handle, { pointerId: 1, button: 0, clientX: 500, clientY: 300 });
  expect(capture.taken).toHaveBeenCalledWith(1);
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 520, clientY: 300 });
  // The page follows the pointer while the button is held.
  expectSize(450, 852);
  fireEvent.pointerMove(handle, { pointerId: 1, clientX: 540, clientY: 300 });
  expectSize(507, 852);
  expect(resizes(calls)).toHaveLength(before);
  fireEvent.pointerUp(handle, { pointerId: 1, clientX: 540, clientY: 300 });
  expect(capture.given).toHaveBeenCalledWith(1);

  await waitFor(() => expect(resizes(calls)).toHaveLength(before + 1));
  expect(resizes(calls).at(-1)).toEqual({ width: 507, height: 852 });
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Device" })).toHaveValue("custom"));
  expect(screen.getByRole("option", { name: "Custom" })).toBeInTheDocument();
});

it("commits a run of arrow keys as one resize", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const before = resizes(calls).length;

  const handle = screen.getByRole("button", { name: /Resize the page from the bottom-right corner/ });
  handle.focus();
  fireEvent.keyDown(handle, { key: "ArrowRight" });
  fireEvent.keyDown(handle, { key: "ArrowRight" });
  fireEvent.keyDown(handle, { key: "ArrowDown", shiftKey: true });
  expectSize(413, 902);

  await waitFor(() => expect(resizes(calls)).toHaveLength(before + 1));
  expect(resizes(calls).at(-1)).toEqual({ width: 413, height: 902 });
});

it("leaves the page alone when a drag never moved", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const before = resizes(calls).length;

  const handle = screen.getByRole("button", { name: /Resize the page from the left edge/ });
  fireEvent.pointerDown(handle, { pointerId: 2, button: 0, clientX: 100, clientY: 300 });
  fireEvent.pointerUp(handle, { pointerId: 2, clientX: 100, clientY: 300 });
  expectSize(393, 852);
  expect(resizes(calls)).toHaveLength(before);
});

it("keeps the size it had when the service refuses the resize", async () => {
  const user = userEvent.setup();
  const { fail } = serve();
  await openToolbar(user);
  fail.resize = true;

  const handle = screen.getByRole("button", { name: /Resize the page from the bottom edge/ });
  fireEvent.pointerDown(handle, { pointerId: 3, button: 0, clientX: 500, clientY: 300 });
  fireEvent.pointerMove(handle, { pointerId: 3, clientX: 500, clientY: 330 });
  fireEvent.pointerUp(handle, { pointerId: 3, clientX: 500, clientY: 330 });

  await waitFor(() =>
    expect(useUi.getState().toasts.at(-1)?.text).toContain("Couldn't resize the page. That size is refused."),
  );
  await waitFor(() => expectSize(393, 852));
  expect(screen.getByRole("combobox", { name: "Device" })).toHaveValue("phone");
});

it("draws the floating page as it is, with nothing to drag", async () => {
  const { calls } = serve();
  vi.stubGlobal("WebSocket", FakeSocket);
  useUi.getState().setPreviewFloat({ threadId: "t", tabId: "tab-1" });
  render(<PreviewFloat threadId="t" />);
  await waitFor(() => expect(document.querySelector(".preview-float")).not.toBeNull());
  await sendFrame();

  await screen.findByRole("img", { name: /^Page at/ });
  expect(screen.queryByRole("button", { name: /Resize the page/ })).toBeNull();
  expect(document.querySelector(".browser-box")).toBeNull();
  expect(calls.filter((call) => call.method !== "GET")).toHaveLength(0);
});

it("keeps a sized page in its own box after the toolbar is hidden, with its handles and a pill that brings the toolbar back", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const sent = calls.filter((call) => call.method === "POST").length;
  await user.click(screen.getByRole("button", { name: "More page actions" }));
  await user.click(await screen.findByRole("menuitem", { name: "Hide device toolbar" }));

  expect(screen.queryByRole("toolbar", { name: "Device toolbar" })).toBeNull();
  // The page still has the size the preset gave it, so it keeps that shape.
  expect(document.querySelector(".browser-box")).not.toBeNull();
  expect(screen.getAllByRole("button", { name: /Resize the page/ })).toHaveLength(5);
  expect(calls.filter((call) => call.method === "POST")).toHaveLength(sent);
  await user.click(screen.getByRole("button", { name: /393 by 852.*Show device toolbar/ }));
  expect(screen.getByRole("toolbar", { name: "Device toolbar" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Show device toolbar/ })).toBeNull();
});

it("resizes once from a typed width on Enter, then reads Custom", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const before = resizes(calls).length;
  const width = screen.getByRole("spinbutton", { name: "Width" });
  await user.clear(width);
  await user.type(width, "500");
  expect(resizes(calls)).toHaveLength(before);
  await user.keyboard("{Enter}");
  await waitFor(() => expect(resizes(calls)).toHaveLength(before + 1));
  expect(resizes(calls).at(-1)).toEqual({ width: 500, height: 852 });
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Device" })).toHaveValue("custom"));
  expectSize(500, 852);
});

it("resizes when the field is left, and steps by 1 or 10 with the arrow keys", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const before = resizes(calls).length;
  const height = screen.getByRole("spinbutton", { name: "Height" });
  await user.click(height);
  await user.keyboard("{ArrowUp}{ArrowUp}{Shift>}{ArrowDown}{/Shift}");
  expect(height).toHaveValue("844");
  expect(resizes(calls)).toHaveLength(before);
  await user.tab();
  await waitFor(() => expect(resizes(calls)).toHaveLength(before + 1));
  expect(resizes(calls).at(-1)).toEqual({ width: 393, height: 844 });
});

it("turns a size out of range red, sends nothing, and puts the old number back on Escape or on leaving", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const before = resizes(calls).length;
  const width = screen.getByRole("spinbutton", { name: "Width" });
  await user.clear(width);
  await user.type(width, "80");
  expect(width).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByRole("alert")).toHaveTextContent("Sizes run from 100 to 4096 pixels.");
  await user.keyboard("{Enter}");
  expect(width).toHaveValue("393");
  expect(screen.queryByRole("alert")).toBeNull();
  await user.clear(width);
  await user.type(width, "5000");
  await user.keyboard("{Escape}");
  expect(width).toHaveValue("393");
  await user.clear(width);
  await user.type(width, "99");
  await user.tab();
  expect(width).toHaveValue("393");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(resizes(calls)).toHaveLength(before);
});

it("keeps the ratio of the moment the lock went on, swaps it on Rotate, and forgets it when the toolbar closes", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  await openToolbar(user);
  const lock = screen.getByRole("button", { name: "Lock aspect ratio" });
  expect(lock).toHaveAttribute("aria-pressed", "false");
  await user.click(lock);
  expect(lock).toHaveAttribute("aria-pressed", "true");
  const width = screen.getByRole("spinbutton", { name: "Width" });
  await user.clear(width);
  await user.type(width, "500{Enter}");
  await waitFor(() => expect(resizes(calls).at(-1)).toEqual({ width: 500, height: 1084 }));
  await waitFor(() => expectSize(500, 1084));
  await user.click(screen.getByRole("button", { name: "Rotate" }));
  await waitFor(() => expectSize(1084, 500));
  const height = screen.getByRole("spinbutton", { name: "Height" });
  await user.clear(height);
  await user.type(height, "1000{Enter}");
  await waitFor(() => expect(resizes(calls).at(-1)).toEqual({ width: 2168, height: 1000 }));
  await user.click(screen.getByRole("button", { name: "More page actions" }));
  await user.click(await screen.findByRole("menuitem", { name: "Hide device toolbar" }));
  await user.click(screen.getByRole("button", { name: /Show device toolbar/ }));
  expect(screen.getByRole("button", { name: "Lock aspect ratio" })).toHaveAttribute("aria-pressed", "false");
});

it("keeps the ratio for a dragged edge while the lock is on", async () => {
  const user = userEvent.setup();
  const { calls } = serve();
  stubPointerCapture();
  await openToolbar(user);
  await user.click(screen.getByRole("button", { name: "Lock aspect ratio" }));
  const handle = screen.getByRole("button", { name: /Resize the page from the right edge/ });
  fireEvent.pointerDown(handle, { pointerId: 9, button: 0, clientX: 500, clientY: 300 });
  fireEvent.pointerMove(handle, { pointerId: 9, clientX: 540, clientY: 300 });
  expectSize(507, 1099);
  fireEvent.pointerUp(handle, { pointerId: 9, clientX: 540, clientY: 300 });
  await waitFor(() => expect(resizes(calls).at(-1)).toEqual({ width: 507, height: 1099 }));
});

it("leaves the fields off while the page follows the panel", async () => {
  const user = userEvent.setup();
  serve();
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(domRect(STAGE.width, STAGE.height));
  render(<BrowserPanel threadId="t" />);
  await screen.findByRole("button", { name: "More page actions" });
  await user.click(screen.getByRole("button", { name: "More page actions" }));
  await user.click(await screen.findByRole("menuitem", { name: "Show device toolbar" }));
  expect(screen.getByRole("spinbutton", { name: "Width" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Lock aspect ratio" })).toBeDisabled();
});
