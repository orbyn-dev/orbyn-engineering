// Settling or archiving a thread that started live Crew work asks first, and cancels each task through the service. The read is
// silent: with Crew Preview off, no desktop shell key, no live tasks or a failed read, the thread settles exactly as it did before.
import { create } from "zustand";
import { canSendPersonKey } from "../../../api/personKey";
import type { Thread } from "../../../api/types";
import { useConnection } from "../../../product/state/connection";
import { leadApi, type LiveTask } from "./api";

export type CloseVerb = "settle" | "archive";

export type CloseOutcome =
  | { kind: "none" }
  | { kind: "declined" }
  | { kind: "closed"; closed: string[]; failed: string[]; tasks: LiveTask[] };

export interface CloseLeadRequest {
  thread: Thread;
  verb: CloseVerb;
  tasks: LiveTask[];
  done(outcome: CloseOutcome): void;
}

interface CloseLeadState {
  /** At most one request: a second one waits in `askCloseLead`'s queue until this one is answered. */
  pending: CloseLeadRequest | null;
  setPending(pending: CloseLeadRequest | null): void;
}

export const useCloseLead = create<CloseLeadState>((set) => ({
  pending: null,
  setPending: (pending) => set({ pending }),
}));

const none = (): CloseOutcome => ({ kind: "none" });

/** Lets one answer finish before the next ask is read, so two dialogs never share the store. */
let tail: Promise<unknown> = Promise.resolve();

async function askNow(thread: Thread, verb: CloseVerb): Promise<CloseOutcome> {
  let tasks: LiveTask[] | null;
  try {
    tasks = await leadApi.live(thread.id);
  } catch {
    // A read that fails must never block the settle or archive the person asked for.
    return none();
  }
  if (!tasks || tasks.length === 0) return none();
  return new Promise<CloseOutcome>((resolve) => {
    useCloseLead.getState().setPending({
      thread,
      verb,
      tasks,
      done: (outcome) => {
        useCloseLead.getState().setPending(null);
        resolve(outcome);
      },
    });
  });
}

/**
 * Asks whether a thread's live tasks should be cancelled before it settles or archives. Resolves `{kind:"none"}` without a
 * read when Crew Preview is off or this page cannot send the person key; otherwise reads the live tasks and, when there are
 * any, sets the pending request and waits for its dialog.
 */
export function askCloseLead(thread: Thread, verb: CloseVerb): Promise<CloseOutcome> {
  if (!useConnection.getState().crewPreview || !canSendPersonKey()) return Promise.resolve(none());
  const asked = tail.then(() => askNow(thread, verb));
  tail = asked.catch(() => undefined);
  return asked;
}
