import { create } from "zustand";
import type { WireThreadEvent } from "../../api/types";
import type { SteerDelivery } from "../../generated/SteerDelivery";
import type { SteerResult } from "../../generated/SteerResult";
import { api } from "../../api/client";
import type { MessageItem, ThreadItem } from "../timeline/types";

/** Where a mid-turn message stands. `pending` is the web's own, until the service answers. */
export type SteerMarkState = SteerDelivery | "pending";

export interface SteerMark {
  id: string;
  text: string;
  state: SteerMarkState;
}

interface SteerMarkStore {
  /** Messages sent into a running turn, by thread, in send order. Kept for this session only. */
  marks: Record<string, SteerMark[]>;
  add(threadId: string, id: string, text: string): void;
  settle(threadId: string, id: string, state: SteerDelivery): void;
  /** Forgets the marks a new turn has taken over: held and refused ones are in the queue or the log. */
  clearHeld(threadId: string): void;
}

const NONE: SteerMark[] = [];

export const useSteerMarks = create<SteerMarkStore>((set) => ({
  marks: {},
  add: (threadId, id, text) =>
    set((state) => ({
      marks: { ...state.marks, [threadId]: [...(state.marks[threadId] ?? []).filter((mark) => mark.id !== id), { id, text, state: "pending" }] },
    })),
  settle: (threadId, id, next) =>
    set((state) => {
      const list = state.marks[threadId];
      if (!list?.some((mark) => mark.id === id && mark.state !== next)) return state;
      return { marks: { ...state.marks, [threadId]: list.map((mark) => (mark.id === id ? { ...mark, state: next } : mark)) } };
    }),
  clearHeld: (threadId) =>
    set((state) => {
      const list = state.marks[threadId];
      if (!list?.some((mark) => mark.state === "queued" || mark.state === "failed")) return state;
      return { marks: { ...state.marks, [threadId]: list.filter((mark) => mark.state !== "queued" && mark.state !== "failed") } };
    }),
}));

export const steerMarksOf = (threadId: string): SteerMark[] => useSteerMarks.getState().marks[threadId] ?? NONE;

const isDelivery = (value: unknown): value is SteerDelivery => value === "delivered" || value === "queued" || value === "failed";

/** Reads the steer news out of fresh log events: `steer.state` by id, and a refusal as failed. */
export function applySteerEvents(threadId: string, events: readonly WireThreadEvent[]): void {
  const store = useSteerMarks.getState();
  for (const wire of events) {
    const event = wire.event;
    if (event.type === "RUN_STARTED") {
      store.clearHeld(threadId);
      continue;
    }
    if (event.type !== "CUSTOM" || (event.name !== "steer.state" && event.name !== "steer.failed")) continue;
    const body = event.value && typeof event.value === "object" && !Array.isArray(event.value) ? event.value : null;
    const id = body?.message_id;
    if (typeof id !== "string") continue;
    const state = event.name === "steer.failed" ? "failed" : body?.state;
    if (isDelivery(state)) store.settle(threadId, id, state);
  }
}

/** The mark's message in the log, which carries the steer's own id as its message id. */
const loggedAt = (items: readonly ThreadItem[], id: string): number =>
  items.findIndex((item) => item.kind === "message" && item.role === "user" && item.key.endsWith(`:${id}`));

/** Puts each mark under its message, or shows the text itself until the log has it. */
export function withSteerMarks(items: readonly ThreadItem[], marks: readonly SteerMark[]): readonly ThreadItem[] {
  if (marks.length === 0) return items;
  const out = [...items];
  for (const mark of marks) {
    const at = loggedAt(out, mark.id);
    const item = out[at];
    if (item?.kind === "message") {
      out[at] = { ...item, steer: mark.state };
      continue;
    }
    const local: MessageItem = { kind: "message", key: `steer:${mark.id}`, seq: 0, ts: "", runId: null, role: "user", text: mark.text, done: true, steer: mark.state };
    out.push(local);
  }
  return out;
}

/** Sends a message into the running turn under a fresh id and keeps its mark current. */
export async function steerWithMark(threadId: string, prompt: string): Promise<SteerResult> {
  const id = crypto.randomUUID();
  useSteerMarks.getState().add(threadId, id, prompt);
  try {
    const result = await api.steer(threadId, prompt, id);
    // The answer is where the steer stood when the service took it, and its log can carry the
    // message further before this reply lands: a refused steer is marked failed while the
    // request is still in flight. So the answer settles a mark still waiting on it, and never
    // pulls one the log has already settled back to delivered.
    if (steerMarksOf(threadId).some((mark) => mark.id === id && mark.state === "pending")) {
      useSteerMarks.getState().settle(threadId, id, result.state);
    }
    return result;
  } catch (failure) {
    // The service never took it, so no mark is left behind; the caller keeps the text.
    useSteerMarks.setState((state) => ({ marks: { ...state.marks, [threadId]: (state.marks[threadId] ?? []).filter((mark) => mark.id !== id) } }));
    throw failure;
  }
}
