import { afterEach, describe, expect, it, vi } from "vitest";
import type { StreamHandlers } from "../../../../api/sse";
import type { WireThreadEvent } from "../../../../api/types";
import { attentionApi, setAttentionApi, type AttentionDelivery } from "./contract";
import { createServiceAttentionApi, MAX_FOLLOWED } from "./client";

afterEach(() => setAttentionApi(null));

interface Follow { threadId: string; handlers: StreamHandlers; stop: ReturnType<typeof vi.fn> }

function setup() {
  let notice: (threadId: string, seq: number) => void = () => undefined;
  const stopNotice = vi.fn();
  const follows: Follow[] = [];
  const api = createServiceAttentionApi({
    onNotice(listener) { notice = listener; return stopNotice; },
    follow(threadId, handlers) { const stop = vi.fn(); follows.push({ threadId, handlers, stop }); return stop; },
  });
  const heard: AttentionDelivery[] = [];
  const stop = api.subscribe((delivery) => heard.push(delivery));
  return { notice: (threadId: string, seq: number) => notice(threadId, seq), stopNotice, follows, heard, stop };
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
});
