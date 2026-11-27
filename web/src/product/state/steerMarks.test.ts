import { beforeEach, describe, expect, it, vi } from "vitest";
import { applySteerEvents, steerMarksOf, steerWithMark, useSteerMarks, withSteerMarks } from "./steerMarks";
import type { MessageItem } from "../timeline/types";
import type { WireThreadEvent } from "../../api/types";
import { api } from "../../api/client";
import type { SteerResult } from "../../generated/SteerResult";

const custom = (seq: number, name: string, value: object, run_id: string | null = null): WireThreadEvent =>
  ({ seq, ts: "2026-01-01T00:00:00Z", run_id, event: { type: "CUSTOM", name, value } }) as unknown as WireThreadEvent;
const run = (seq: number, type: "RUN_STARTED"): WireThreadEvent =>
  ({ seq, ts: "2026-01-01T00:00:00Z", run_id: "r2", event: { type } }) as unknown as WireThreadEvent;
const logged = (messageId: string): MessageItem => ({
  kind: "message", key: `msg:r1:${messageId}`, seq: 3, ts: "", runId: "r1", role: "user", text: "Also check the signup redirect", done: true,
});

describe("steer marks", () => {
  beforeEach(() => {
    useSteerMarks.setState({ marks: {} });
    vi.restoreAllMocks();
  });

  it("starts pending and follows the service's answer", () => {
    const store = useSteerMarks.getState();
    store.add("t1", "m1", "Use the same rule");
    expect(steerMarksOf("t1")).toEqual([{ id: "m1", text: "Use the same rule", state: "pending" }]);
    store.settle("t1", "m1", "queued");
    expect(steerMarksOf("t1")[0]?.state).toBe("queued");
  });

  it("updates by id from a steer.state event, in any order", () => {
    const store = useSteerMarks.getState();
    store.add("t1", "a", "first");
    store.add("t1", "b", "second");
    applySteerEvents("t1", [custom(5, "steer.state", { message_id: "b", state: "failed" }), custom(6, "steer.state", { message_id: "a", state: "delivered" })]);
    expect(steerMarksOf("t1").map((mark) => [mark.id, mark.state])).toEqual([["a", "delivered"], ["b", "failed"]]);
  });

  it("marks a refused steer failed even without a state event, and ignores unknown ids", () => {
    useSteerMarks.getState().add("t1", "a", "first");
    applySteerEvents("t1", [custom(5, "steer.failed", { message_id: "a", prompt: "first" }), custom(6, "steer.state", { message_id: "zzz", state: "failed" })]);
    expect(steerMarksOf("t1")).toHaveLength(1);
    expect(steerMarksOf("t1")[0]?.state).toBe("failed");
  });

  it("drops a queued or failed mark once the next turn starts", () => {
    const store = useSteerMarks.getState();
    store.add("t1", "q", "held"); store.settle("t1", "q", "queued");
    store.add("t1", "d", "done"); store.settle("t1", "d", "delivered");
    applySteerEvents("t1", [run(9, "RUN_STARTED")]);
    expect(steerMarksOf("t1").map((mark) => mark.id)).toEqual(["d"]);
  });

  it("shows a mark under its logged message, and a local bubble until the log has it", () => {
    const marks = [{ id: "m1", text: "Also check the signup redirect", state: "delivered" as const }, { id: "m2", text: "Skip admin", state: "pending" as const }];
    const items = withSteerMarks([logged("m1")], marks);
    expect(items).toHaveLength(2);
    expect((items[0] as MessageItem).steer).toBe("delivered");
    expect(items[1]).toMatchObject({ kind: "message", role: "user", text: "Skip admin", steer: "pending", key: "steer:m2" });
  });

  it("changes nothing with no marks", () => {
    const items = [logged("m1")];
    expect(withSteerMarks(items, [])).toBe(items);
  });

  it("settles a waiting mark from the service's own answer", async () => {
    vi.spyOn(api, "steer").mockResolvedValueOnce({ message_id: "m1", state: "queued", queued_message_id: "q1" });
    await expect(steerWithMark("t1", "hold this")).resolves.toMatchObject({ state: "queued" });
    expect(steerMarksOf("t1")).toEqual([{ id: expect.any(String), text: "hold this", state: "queued" }]);
  });

  it("keeps a mark the log already settled when the answer arrives late", async () => {
    // A refused steer is marked failed while the request is still in flight; the answer then says
    // delivered, because that is where the service had it when the request arrived.
    vi.spyOn(api, "steer").mockImplementationOnce(async (threadId: string, _prompt: string, messageId?: string) => {
      applySteerEvents(threadId, [
        custom(7, "steer.failed", { message_id: messageId, prompt: "refused" }),
        custom(8, "steer.state", { message_id: messageId, state: "failed" }),
      ]);
      return { message_id: messageId ?? "m1", state: "delivered" } satisfies SteerResult;
    });
    await steerWithMark("t1", "refused");
    expect(steerMarksOf("t1")).toEqual([{ id: expect.any(String), text: "refused", state: "failed" }]);
  });
});
