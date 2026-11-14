import { create } from "zustand";
import { ApiFailure } from "../../../api/client";
import type { CrewTaskCancelled } from "../../../generated/CrewTaskCancelled";
import type { CrewTaskRerunDone } from "../../../generated/CrewTaskRerunDone";
import { toFailure } from "../../../lib/useAsync";
import { writesApi } from "./api";

export type WriteKind = "cancel" | "rerun";

export type Op =
  | { kind: WriteKind; phase: "busy" }
  | { kind: "cancel"; phase: "done"; answer: CrewTaskCancelled }
  | { kind: "rerun"; phase: "done"; answer: CrewTaskRerunDone }
  | { kind: WriteKind; phase: "failed"; message: string; status: number; code: string };

interface Writes {
  /** The last write of each task, by task id: on its way, answered, or refused. */
  ops: Record<string, Op>;
  dismiss(id: string): void;
}

export const useWrites = create<Writes>((set) => ({
  ops: {},
  dismiss: (id) =>
    set((state) => {
      const { [id]: _gone, ...rest } = state.ops;
      return { ops: rest };
    }),
}));

/** The two messages the service's cancel sends with a 502, and the code it sends them under (the same code for both). */
const CANCEL_502 = ["The task was not cancelled.", "The task is cancelled, but its worker could not be stopped."];
export const CHECK_THE_TASK = "Check the task’s state before trying again.";

/**
 * The words for a failed write. A cancel's 502 means the worker's engine could not be stopped, and the service says in its own
 * message whether the task was left as it was or is cancelled: that message is shown as it is. A 502 whose code or message is not
 * one of those two (a proxy, another path, no message) says only to check the task: this screen never says "cancelled" or
 * "not cancelled" itself.
 */
export function failureText(kind: WriteKind, status: number, code: string, message: string): string {
  if (kind === "cancel" && status === 502) {
    return code === "engine_unavailable" && CANCEL_502.some((known) => message.startsWith(known)) ? message : CHECK_THE_TASK;
  }
  return message;
}

const put = (id: string, op: Op) => useWrites.setState((state) => ({ ops: { ...state.ops, [id]: op } }));

/** A re-run is running somewhere on this device: the service allows one at a time, so every Re-run waits for it. */
export const rerunBusy = (ops: Record<string, Op>) => Object.values(ops).some((op) => op.kind === "rerun" && op.phase === "busy");

/**
 * Sends one write. It is called only from a person's click on a confirm. A second call for a task whose write is on its way
 * sends nothing, and so does a Re-run while another is running. There is no retry: a failure is shown and stays until dismissed.
 * `onDone` runs after an answer or a refusal (the screens read again), and is a read.
 */
export async function runWrite(kind: WriteKind, id: string, reason: string, onDone?: () => void): Promise<void> {
  const ops = useWrites.getState().ops;
  if (ops[id]?.phase === "busy" || (kind === "rerun" && rerunBusy(ops))) return;
  put(id, { kind, phase: "busy" });
  try {
    if (kind === "cancel") put(id, { kind, phase: "done", answer: await writesApi.cancel(id, reason) });
    else put(id, { kind, phase: "done", answer: await writesApi.rerun(id, reason) });
  } catch (error) {
    const failure = toFailure(error);
    const status = failure instanceof ApiFailure ? failure.status : 0;
    put(id, { kind, phase: "failed", message: failureText(kind, status, failure.code, failure.message), status, code: failure.code });
  }
  onDone?.();
}
