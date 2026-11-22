import { afterEach, describe, expect, it, vi } from "vitest";
import type { StreamHandlers } from "../../../../api/sse";
import type { WireThreadEvent } from "../../../../api/types";
import { attentionApi, setAttentionApi, type AttentionDelivery } from "./contract";
import { registerTerminal, terminalKey, type TerminalControls } from "../terminalControls";
import { createServiceAttentionApi, MAX_FOLLOWED, MAX_OPEN_ITEMS, type AttentionSources } from "./client";

afterEach(() => setAttentionApi(null));

interface Follow { threadId: string; handlers: StreamHandlers; stop: ReturnType<typeof vi.fn> }

const waiting = (over: Record<string, unknown> = {}) => ({
  v: 1, op: "waiting", id: "a1", terminal_id: "tm1", terminal_title: "dev", engine: "claude", kind: "yes_no",
  since: "2026-11-22T14:12:00Z", excerpt: ["Overwrite auth/redirect.ts? (y/N)"], ...over,
});
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
const terminal = (): TerminalControls => ({
  clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined,
});

function setup(options: {
  openItems?: (threadId: string, signal?: AbortSignal) => Promise<unknown>;
  terminalThreads?: () => readonly string[];
  onFollow?: (threadId: string) => void;
} = {}) {
  let notice: (threadId: string, seq: number) => void = () => undefined;
  let terminalsChanged: () => void = () => undefined;
  const stopNotice = vi.fn();
  const follows: Follow[] = [];
  const api = createServiceAttentionApi({
    onNotice(listener) { notice = listener; return stopNotice; },
    follow(threadId, handlers) { options.onFollow?.(threadId); const stop = vi.fn(); follows.push({ threadId, handlers, stop }); return stop; },
    ...(options.openItems ? { openItems: options.openItems as NonNullable<AttentionSources["openItems"]> } : {}),
    ...(options.terminalThreads ? { terminalThreads: options.terminalThreads } : {}),
    onTerminalChange(listener) { terminalsChanged = listener; return () => undefined; },
  });
  const heard: AttentionDelivery[] = [];
  const stop = api.subscribe((delivery) => heard.push(delivery));
  return { notice: (threadId: string, seq: number) => notice(threadId, seq), terminalsChanged: () => terminalsChanged(), stopNotice, follows, heard, stop };
}
const wire = (seq: number, event: unknown, over: Partial<WireThreadEvent> = {}) =>
  ({ seq, thread_id: "t1", run_id: null, ts: "2026-11-22T14:12:03Z", event, ...over }) as WireThreadEvent;

describe("the service client", () => {
  it("is the production default, and follows nothing until the service says a thread announced something", () => {
    expect(attentionApi()).not.toBeNull();
    const { follows } = setup();
    expect(follows).toHaveLength(0);
  });

  it("follows a thread from just before the announced seq, so the event itself arrives", () => {
    const { notice, follows } = setup();
    notice("t1", 412);
    expect(follows).toHaveLength(1);
    expect(follows[0]?.threadId).toBe("t1");
    expect(follows[0]?.handlers.lastSeq()).toBe(411);
  });

  it("passes each logged event on with its thread, and resumes after the newest one", () => {
    const { notice, follows, heard } = setup();
    notice("t1", 5);
    const custom = { type: "CUSTOM", name: "orbyn.terminal_attention", value: { v: 1, op: "clear", id: "a1", reason: "input" } };
    follows[0]?.handlers.onEvents([wire(5, custom), wire(6, { type: "TEXT_MESSAGE_CONTENT" })]);
    expect(heard.map((one) => [one.thread_id, one.seq, one.ts])).toEqual([["t1", 5, "2026-11-22T14:12:03Z"], ["t1", 6, "2026-11-22T14:12:03Z"]]);
    expect(heard[0]?.event).toEqual(custom);
    expect(follows[0]?.handlers.lastSeq()).toBe(6);
  });

  it("follows a thread once however many notices come", () => {
    const { notice, follows } = setup();
    notice("t1", 5);
    notice("t1", 9);
    notice("t2", 1);
    expect(follows.map((one) => one.threadId)).toEqual(["t1", "t2"]);
  });

  it("ignores a notice or an event it cannot read, and never throws", () => {
    const { notice, follows, heard } = setup();
    expect(() => {
      notice("", 3);
      notice("t1", Number.NaN);
      notice("t1", -1);
      notice("t1", 2);
      follows[0]?.handlers.onEvents([null, 7, "x", { seq: "no" }, wire(3, undefined), wire(4, "text")] as unknown as WireThreadEvent[]);
    }).not.toThrow();
    expect(follows).toHaveLength(1);
    expect(heard.map((one) => one.seq)).toEqual([3, 4]);
  });

  it("keeps a few threads at most and stops the oldest first", () => {
    const { notice, follows } = setup();
    for (let index = 0; index <= MAX_FOLLOWED; index += 1) notice(`t${index}`, 1);
    expect(follows).toHaveLength(MAX_FOLLOWED + 1);
    expect(follows[0]?.stop).toHaveBeenCalledTimes(1);
    expect(follows[1]?.stop).not.toHaveBeenCalled();
  });

  it("stops every followed thread and the notice listener when unsubscribed", () => {
    const { notice, follows, stop, stopNotice } = setup();
    notice("t1", 1);
    notice("t2", 1);
    stop();
    expect(stopNotice).toHaveBeenCalledTimes(1);
    expect(follows.every((one) => one.stop.mock.calls.length === 1)).toBe(true);
  });

  it("reads the threads with an open terminal once when the client starts, without following anything", async () => {
    const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => ({ items: [waiting()] }));
    const { heard, follows } = setup({ openItems, terminalThreads: () => ["t1"] });
    await settle();
    expect(openItems).toHaveBeenCalledTimes(1);
    expect(openItems.mock.calls[0]?.[0]).toBe("t1");
    expect(openItems.mock.calls[0]?.[1]).toBeInstanceOf(AbortSignal);
    expect(follows).toHaveLength(0);
    expect(heard).toHaveLength(1);
    expect(heard[0]).toMatchObject({ seq: 0, ts: "", thread_id: "t1", source: "open" });
  });

  it("reads a thread that the window's own terminal registry names", async () => {
    const stop = registerTerminal(terminalKey("opened", undefined), terminal());
    try {
      const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => ({ items: [] }));
      setup({ openItems });
      await settle();
      expect(openItems.mock.calls.map(([threadId]) => threadId)).toContain("opened");
    } finally {
      stop();
    }
  });

  it("reads a thread once when its terminal opens later", async () => {
    let threads: string[] = [];
    const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => ({ items: [waiting()] }));
    const { terminalsChanged, heard } = setup({ openItems, terminalThreads: () => threads });
    await settle();
    expect(openItems).not.toHaveBeenCalled();
    threads = ["t1"];
    terminalsChanged();
    await settle();
    terminalsChanged();
    await settle();
    expect(openItems).toHaveBeenCalledTimes(1);
    expect(heard).toHaveLength(1);
  });

  it("reads no more than MAX_FOLLOWED threads", async () => {
    const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => ({ items: [] }));
    setup({ openItems, terminalThreads: () => Array.from({ length: MAX_FOLLOWED + 2 }, (_, index) => `t${index}`) });
    await settle();
    expect(openItems).toHaveBeenCalledTimes(MAX_FOLLOWED);
  });

  it("reads a thread's open items before following it from the notice", async () => {
    const order: string[] = [];
    const openItems = vi.fn(async () => {
      order.push("read");
      return { items: [waiting()] };
    });
    const { notice, follows, heard } = setup({ openItems, onFollow: () => order.push("follow") });
    notice("t1", 9);
    expect(order).not.toContain("follow");
    await settle();
    expect(order).toEqual(["read", "follow"]);
    expect(heard).toHaveLength(1);
    expect(follows[0]?.handlers.lastSeq()).toBe(8);
  });

  it("says nothing and still follows when the read fails or 404s", async () => {
    const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => {
      throw new Error("http_404");
    });
    const { notice, follows, heard } = setup({ openItems });
    expect(() => notice("t1", 9)).not.toThrow();
    await settle();
    expect(heard).toHaveLength(0);
    expect(follows).toHaveLength(1);
    expect(follows[0]?.handlers.lastSeq()).toBe(8);
  });

  it("accepts at most eight open items and ignores unknown shapes", async () => {
    const items: unknown[] = [
      ...Array.from({ length: MAX_OPEN_ITEMS + 2 }, (_, index) => waiting({ id: `a${index}` })),
      null,
      7,
      "text",
      { v: 1, op: "clear", id: "a1", reason: "input" },
    ];
    const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => ({ items }));
    const { notice, heard } = setup({ openItems });
    expect(() => notice("t1", 1)).not.toThrow();
    await settle();
    expect(heard).toHaveLength(MAX_OPEN_ITEMS);
    expect(heard.map((one) => (one.event as { value: { id: string } }).value.id)).toEqual(
      Array.from({ length: MAX_OPEN_ITEMS }, (_, index) => `a${index}`),
    );
  });

  it("ignores an answer that is not an open-items response", async () => {
    const openItems = vi.fn(async (_threadId: string, _signal?: AbortSignal) => null as never);
    const { notice, heard, follows } = setup({ openItems });
    notice("t1", 1);
    await settle();
    expect(heard).toHaveLength(0);
    expect(follows).toHaveLength(1);
  });

  it("passes a later clear on the log so the item read earlier can be cleared", async () => {
    const { notice, follows, heard } = setup({ openItems: async () => ({ items: [waiting()] }) });
    notice("t1", 5);
    await settle();
    follows[0]?.handlers.onEvents([wire(5, { v: 1, op: "clear", id: "a1", reason: "input" })]);
    expect(heard.map((one) => [one.seq, one.source])).toEqual([[0, "open"], [5, undefined]]);
  });
});
