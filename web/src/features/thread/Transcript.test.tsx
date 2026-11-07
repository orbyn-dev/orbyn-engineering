import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUi } from "../../state/ui";
import { Transcript } from "./Transcript";
import { ThreadFind } from "./ThreadFind";
import { emptyLog, type MessageItem, type ThreadItem } from "./projection";
import { useThreadLogs } from "./threadLogs";

/**
 * jsdom has no layout, so this gives the transcript a small one: every `.item` is HEIGHT tall
 * unless a test says otherwise, rows stack with the 16px gap under a 24px top padding, and the
 * scroller is VIEW tall with a settable scrollTop. It is enough to check what is mounted and
 * where the view sits, which is what windowing decides.
 */
const VIEW = 600;
const HEIGHT = 100;
const heights = new Map<string, number>();
let callbacks: ResizeObserverCallback[] = [];

const stackHeight = (children: Element[]): number =>
  children.reduce((sum, child) => sum + blockHeight(child) + 16, 0);
function blockHeight(child: Element): number {
  const key = (child as HTMLElement).dataset?.entry;
  if (key) return heights.get(key) ?? HEIGHT;
  return Number.parseFloat((child as HTMLElement).style?.height || "0") || 0;
}

function layout() {
  const proto = HTMLElement.prototype;
  const scrollTops = new WeakMap<Element, number>();
  Object.defineProperty(proto, "scrollTop", {
    configurable: true,
    get(this: HTMLElement) {
      return scrollTops.get(this) ?? 0;
    },
    set(this: HTMLElement, value: number) {
      const max = Math.max(0, (this as HTMLElement).scrollHeight - VIEW);
      scrollTops.set(this, Math.min(max, Math.max(0, value)));
      this.dispatchEvent(new Event("scroll"));
    },
  });
  Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => VIEW });
  Object.defineProperty(proto, "scrollHeight", {
    configurable: true,
    get(this: HTMLElement) {
      if (!this.classList.contains("transcript")) return 0;
      const inner = this.querySelector(".transcript-inner");
      return 24 + stackHeight([...(inner?.children ?? [])]) - 16 + 24;
    },
  });
  proto.getBoundingClientRect = function (this: HTMLElement) {
    const make = (top: number, height: number) => ({ top, bottom: top + height, height, left: 0, right: 800, width: 800, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
    if (this.classList.contains("transcript")) return make(0, VIEW);
    const scroller = this.closest(".transcript") as HTMLElement | null;
    const parent = this.parentElement;
    if (!scroller || !parent) return make(0, 0);
    const before = [...parent.children].slice(0, [...parent.children].indexOf(this));
    return make(24 + stackHeight(before) - scroller.scrollTop, blockHeight(this));
  };
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: ResizeObserverCallback) {
        callbacks.push(callback);
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = function (this: HTMLElement, options?: boolean | ScrollIntoViewOptions) {
    const scroller = this.closest(".transcript") as HTMLElement | null;
    const block = typeof options === "object" ? options.block : "start";
    if (!scroller) return;
    const lead = block === "center" ? VIEW / 2 - HEIGHT / 2 : 0;
    scroller.scrollTop = this.getBoundingClientRect().top + scroller.scrollTop - lead;
  };
}

const message = (key: string, role: "user" | "assistant", text: string, runId: string, done = true): MessageItem => ({
  kind: "message",
  key: `msg:${key}`,
  seq: 0,
  ts: "2026-01-01T00:00:00Z",
  runId,
  role,
  text,
  done,
});

/** Turn n: your prompt and the answer, as two rows. */
function turns(count: number): ThreadItem[] {
  return Array.from({ length: count }, (_, n) => [
    message(`u${n}`, "user", `Question ${n}`, `r${n}`),
    message(`a${n}`, "assistant", `Answer ${n}`, `r${n}`),
  ]).flat();
}

const load = (threadId: string, items: ThreadItem[]) =>
  useThreadLogs.setState((state) => ({ logs: { ...state.logs, [threadId]: { ...emptyLog, items } } }));

const scroller = () => screen.getByRole("log", { name: "Thread messages" });
const mountedRows = () => document.querySelectorAll(".transcript .item").length;
const mountedKeys = () => [...document.querySelectorAll<HTMLElement>(".transcript .item")].map((row) => row.dataset.entry);
const scrollTo = (top: number) => {
  act(() => {
    scroller().scrollTop = top;
  });
};
const resize = () => act(() => callbacks.forEach((callback) => callback([], {} as ResizeObserver)));

beforeEach(() => {
  heights.clear();
  callbacks = [];
  layout();
  // The windowed rows need an animation frame for find; run it at once.
  vi.stubGlobal("requestAnimationFrame", (run: FrameRequestCallback) => setTimeout(() => run(0), 0));
  vi.stubGlobal("cancelAnimationFrame", (id: number) => clearTimeout(id));
});
afterEach(() => {
  vi.unstubAllGlobals();
  useThreadLogs.setState({ logs: {}, streams: {}, prompts: {} });
  useUi.setState({ messageReveal: null });
});

describe("Transcript windowing", () => {
  it("mounts only the rows near the view and keeps the full scroll height", () => {
    load("t-window", turns(500)); // 1,000 rows
    render(<Transcript threadId="t-window" active={false} />);
    expect(mountedRows()).toBeGreaterThan(0);
    expect(mountedRows()).toBeLessThan(40);
    // Mounted rows are 100px; the rest stand in at their estimates (56 for a prompt, 39 for a reply).
    const mounted = new Set(mountedKeys());
    let expected = 24 + 24 - 16;
    for (let n = 0; n < 500; n++) {
      expected += (mounted.has(`msg:u${n}`) ? 100 : 56) + 16;
      expected += (mounted.has(`msg:a${n}`) ? 100 : 39) + 16;
    }
    // Rows that were measured and then left the window keep their measured height, so the
    // height is at least the estimate-only total and a little more, never the 1,000 rows' worth.
    expect(scroller().scrollHeight).toBeGreaterThanOrEqual(expected);
    expect(scroller().scrollHeight).toBeLessThan(expected + 3000);
  });

  it("opens at the end of the thread", () => {
    load("t-end", turns(500));
    render(<Transcript threadId="t-end" active={false} />);
    expect(scroller().scrollTop).toBe(scroller().scrollHeight - VIEW);
    expect(mountedKeys()).toContain("msg:a499");
    expect(screen.getByText("Answer 499")).toBeInTheDocument();
    expect(screen.queryByText("Question 0")).not.toBeInTheDocument();
  });

  it("mounts the rows of the new place after a long scroll and drops the old ones", () => {
    load("t-far", turns(500));
    render(<Transcript threadId="t-far" active={false} />);
    scrollTo(0);
    expect(screen.getByText("Question 0")).toBeInTheDocument();
    expect(screen.queryByText("Answer 499")).not.toBeInTheDocument();
    expect(mountedRows()).toBeLessThan(40);
  });

  it("follows a growing last row while pinned, and lets go once you scroll up", () => {
    load("t-stream", turns(100));
    const { rerender } = render(<Transcript threadId="t-stream" active />);
    const end = () => scroller().scrollHeight - VIEW;
    expect(scroller().scrollTop).toBe(end());
    heights.set("msg:a99", 400); // streaming text wraps onto more lines
    resize();
    expect(scroller().scrollTop).toBe(end());
    scrollTo(2000);
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeInTheDocument();
    const reading = scroller().scrollTop;
    heights.set("msg:a99", 900);
    resize();
    rerender(<Transcript threadId="t-stream" active />);
    expect(scroller().scrollTop).toBe(reading);
  });

  it("Jump to latest returns to the end from a far scroll", async () => {
    load("t-jump", turns(300));
    render(<Transcript threadId="t-jump" active={false} />);
    scrollTo(500);
    await userEvent.click(screen.getByRole("button", { name: "Jump to latest" }));
    expect(scroller().scrollTop).toBe(scroller().scrollHeight - VIEW);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).not.toBeInTheDocument();
    expect(screen.getByText("Answer 299")).toBeInTheDocument();
  });

  it("keeps the row you are reading in place when rows above it turn out taller", () => {
    load("t-above", turns(300));
    render(<Transcript threadId="t-above" active={false} />);
    scrollTo(6000);
    const reading = mountedKeys().find((key) => document.querySelector<HTMLElement>(`[data-entry="${key}"]`)!.getBoundingClientRect().top >= 0)!;
    const before = document.querySelector<HTMLElement>(`[data-entry="${reading}"]`)!.getBoundingClientRect().top;
    // Rows above the view get a taller measurement: an image or a code block finished loading.
    for (const key of mountedKeys()) {
      const row = document.querySelector<HTMLElement>(`[data-entry="${key}"]`)!;
      if (row.getBoundingClientRect().top < before - 300) heights.set(key!, 260);
    }
    resize();
    const after = document.querySelector<HTMLElement>(`[data-entry="${reading}"]`)!.getBoundingClientRect().top;
    expect(after).toBeCloseTo(before, 0);
  });

  it("comes back to the same place after switching threads", () => {
    load("t-back", turns(300));
    const first = render(<Transcript threadId="t-back" active={false} />);
    scrollTo(8000);
    const top = mountedKeys().find((key) => document.querySelector<HTMLElement>(`[data-entry="${key}"]`)!.getBoundingClientRect().top >= 0)!;
    const topAt = document.querySelector<HTMLElement>(`[data-entry="${top}"]`)!.getBoundingClientRect().top;
    first.unmount();
    render(<Transcript threadId="t-back" active={false} />);
    const again = document.querySelector<HTMLElement>(`[data-entry="${top}"]`);
    expect(again).not.toBeNull();
    expect(again!.getBoundingClientRect().top).toBeCloseTo(topAt, 0);
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeInTheDocument();
  });

  it("brings a message far outside the window into view for a deep link", async () => {
    load("t-link", turns(500));
    render(<Transcript threadId="t-link" active={false} />);
    expect(screen.queryByText("Answer 7")).not.toBeInTheDocument();
    act(() => useUi.getState().revealMessage("t-link", "a7"));
    expect(await screen.findByText("Answer 7")).toBeInTheDocument();
    const row = document.querySelector<HTMLElement>('[data-entry="msg:a7"]')!;
    expect(row.getBoundingClientRect().top).toBeGreaterThanOrEqual(0);
    expect(row.getBoundingClientRect().top).toBeLessThan(VIEW);
    expect(row).toHaveAttribute("data-revealed", "true");
    expect(useUi.getState().messageReveal).toBeNull();
  });

  it("keeps the row with focus mounted while the view is far away", () => {
    load("t-focus", turns(300));
    render(<Transcript threadId="t-focus" active={false} />);
    scrollTo(0);
    const row = document.querySelector<HTMLElement>('[data-entry="msg:u0"]')!;
    row.tabIndex = 0;
    act(() => row.focus());
    scrollTo(30000);
    expect(mountedKeys()).toContain("msg:u0");
    expect(document.activeElement).toBe(row);
  });

  it("keeps both ends of a selection mounted so copy does not lose them", () => {
    load("t-select", turns(300));
    render(<Transcript threadId="t-select" active={false} />);
    scrollTo(0);
    const range = document.createRange();
    const start = screen.getByText("Question 0");
    range.setStart(start.firstChild!, 0);
    range.setEnd(start.firstChild!, 4);
    const selection = document.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    act(() => {
      document.dispatchEvent(new Event("selectionchange"));
    });
    scrollTo(30000);
    expect(mountedKeys()).toContain("msg:u0");
    expect(document.getSelection()?.toString()).toBe("Ques");
  });

  it("keeps its log role and label, and its prompts landmark", () => {
    load("t-a11y", turns(50));
    render(<Transcript threadId="t-a11y" active={false} />);
    expect(scroller()).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("navigation", { name: "Prompts in this thread" })).toBeInTheDocument();
    // Spacers carry no content for assistive technology.
    for (const gap of document.querySelectorAll(".transcript-inner > div[aria-hidden]")) expect(gap).toBeEmptyDOMElement();
  });

  it("lets the prompts strip reach a prompt that is not mounted", async () => {
    load("t-mini", turns(400));
    render(<Transcript threadId="t-mini" active={false} />);
    const ticks = screen.getAllByRole("button", { name: /^Go to prompt: Question/ });
    expect(ticks.length).toBe(400);
    expect(screen.queryByText("Question 3")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Go to prompt: Question 3" }));
    await waitFor(() => expect(document.querySelector('[data-entry="msg:u3"]')).not.toBeNull());
    const top = document.querySelector<HTMLElement>('[data-entry="msg:u3"]')!.getBoundingClientRect().top;
    expect(Math.abs(top)).toBeLessThan(HEIGHT);
  });
});

describe("Find in thread", () => {
  const find = async (threadId: string, text: string) => {
    render(
      <>
        <Transcript threadId={threadId} active={false} />
        <ThreadFind threadId={threadId} onClose={() => {}} />
      </>,
    );
    fireEvent.change(screen.getByRole("textbox", { name: "Find in thread" }), { target: { value: text } });
  };

  it("counts matches in rows that are not mounted and scrolls to each one", async () => {
    const items = turns(500);
    items[2] = message("a1", "assistant", "Answer 1 mentions zebra", "r1");
    items[998] = message("a499", "assistant", "Answer 499 also zebra", "r499");
    load("t-find", items);
    await find("t-find", "zebra");
    // Both matches count although only the end of the thread is mounted.
    expect(await screen.findByText("1/2")).toBeInTheDocument();
    expect(await screen.findByText("Answer 1 mentions zebra")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Next match" }));
    expect(await screen.findByText("2/2")).toBeInTheDocument();
    expect(await screen.findByText("Answer 499 also zebra")).toBeInTheDocument();
    expect(screen.queryByText("Answer 1 mentions zebra")).not.toBeInTheDocument();
  });

  it("reports no matches for text that is nowhere in the thread", async () => {
    load("t-none", turns(50));
    await find("t-none", "walrus");
    expect(await screen.findByText("0/0")).toBeInTheDocument();
  });
});
