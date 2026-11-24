import { api } from "../../../../api/client";
import { followThread, type StreamHandlers } from "../../../../api/sse";
import { onThreadNotice } from "../../../../api/shell";
import { onTerminalRegistryChange, openTerminalThreads } from "../terminalControls";
import { ATTENTION_EVENT, MAX_OPEN_ITEMS, type AttentionApi, type AttentionDelivery, type TerminalAttentionOpen } from "./types";

export { MAX_OPEN_ITEMS };

/** Threads followed at once; past this the one followed longest ago is let go (a later notice follows it again). */
export const MAX_FOLLOWED = 32;

/** Open-item reads in flight at once: enough for a few terminals without a read storm when the window opens. */
const READ_BATCH = 4;

export interface AttentionSources {
  /** The shell stream's notice that a thread's log has something to read, with its `seq` and no payload. */
  onNotice(listener: (threadId: string, seq: number) => void): () => void;
  /** Follows one thread's event log. */
  follow(threadId: string, handlers: StreamHandlers): () => void;
  /**
   * The items waiting now in one thread (`GET /api/threads/{id}/terminal-attention`). Read once when the window learns of a
   * thread's open terminal and once before following a thread from a notice; the log follow continues even when it fails.
   */
  openItems?(threadId: string, signal?: AbortSignal): Promise<TerminalAttentionOpen>;
  /** The threads with a terminal open in this window, read once when the attention client starts. */
  terminalThreads?(): readonly string[];
  /** Calls back when a terminal opens or closes in this window, so a thread it brings is read once too. */
  onTerminalChange?(listener: () => void): () => void;
}

const live: AttentionSources = {
  onNotice: onThreadNotice,
  follow: followThread,
  openItems: (threadId, signal) => api.terminalAttention(threadId, signal),
  terminalThreads: openTerminalThreads,
  onTerminalChange: onTerminalRegistryChange,
};

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null;

/** A value read from the open-items route, fed through the same store path as a log event. It is not new: `seq` 0 and `source` open. */
function openDelivery(threadId: string, value: unknown): AttentionDelivery | null {
  if (!isRecord(value) || value["op"] !== "waiting") return null;
  return {
    seq: 0,
    ts: "",
    thread_id: threadId,
    source: "open",
    event: { type: "CUSTOM", name: ATTENTION_EVENT, value },
  };
}

/**
 * The service client: threads that never announced anything are not followed. A notice starts following that thread's log from just
 * before the announced `seq`, so the event arrives through the ordinary log stream (ordering and replay come with it) and a later
 * clear arrives the same way. Before following, and once when the window opens for each thread with an open terminal, it reads the
 * thread's items waiting now so an item that began before the window did is drawn too. Reads only. What an event means, and what is
 * ignored, is decided by the store; here a frame that is not a log event with a number `seq` is dropped, never thrown on.
 */
export function createServiceAttentionApi(sources: AttentionSources = live): AttentionApi {
  const terminalThreads = sources.terminalThreads ?? openTerminalThreads;
  const onTerminalChange = sources.onTerminalChange ?? onTerminalRegistryChange;
  return {
    subscribe(onEvent) {
      const followed = new Map<string, () => void>();
      const pendingFollows = new Map<string, Promise<void>>();
      const reads = new Map<string, Promise<void>>();
      const controller = new AbortController();
      const queue: { run(): void; cancel(): void }[] = [];
      let active = 0;
      let stopped = false;

      const read = async (threadId: string): Promise<void> => {
        if (stopped || !sources.openItems) return;
        let answer: unknown;
        try {
          answer = await sources.openItems(threadId, controller.signal);
        } catch {
          // A read that fails or 404s draws nothing, says nothing, and the log follow still happens.
          return;
        }
        if (stopped || !isRecord(answer) || !Array.isArray(answer["items"])) return;
        for (const value of answer["items"].slice(0, MAX_OPEN_ITEMS)) {
          const delivery = openDelivery(threadId, value);
          if (delivery) onEvent(delivery);
        }
      };

      const pump = () => {
        if (stopped) {
          for (const next of queue.splice(0)) next.cancel();
          return;
        }
        while (active < READ_BATCH) {
          const next = queue.shift();
          if (!next) return;
          active += 1;
          next.run();
        }
      };

      const readOnce = (threadId: string): Promise<void> => {
        const known = reads.get(threadId);
        if (known) return known;
        if (threadId === "" || reads.size >= MAX_FOLLOWED || !sources.openItems) return Promise.resolve();
        const task = new Promise<void>((resolve) => {
          queue.push({
            run: () =>
              void read(threadId).finally(() => {
                active -= 1;
                resolve();
                pump();
              }),
            cancel: resolve,
          });
        });
        reads.set(threadId, task);
        pump();
        return task;
      };

      const follow = (threadId: string, from: number) => {
        if (stopped) return;
        let last = from;
        followed.set(threadId, sources.follow(threadId, {
          lastSeq: () => last,
          onStatus: () => undefined,
          onEvents(events) {
            for (const wire of events as readonly unknown[]) {
              if (!isRecord(wire) || typeof wire["seq"] !== "number" || !Number.isFinite(wire["seq"])) continue;
              last = Math.max(last, wire["seq"]);
              const delivery: AttentionDelivery = {
                seq: wire["seq"], ts: typeof wire["ts"] === "string" ? wire["ts"] : "", thread_id: threadId, event: wire["event"],
              };
              onEvent(delivery);
            }
          },
        }));
        if (followed.size > MAX_FOLLOWED) {
          const [oldest, stop] = followed.entries().next().value as [string, () => void];
          followed.delete(oldest);
          stop();
        }
      };

      const beginFollow = (threadId: string, seq: number) => {
        if (stopped || followed.has(threadId) || pendingFollows.has(threadId)) return;
        if (!sources.openItems) {
          follow(threadId, seq - 1);
          return;
        }
        const start = readOnce(threadId).then(() => {
          pendingFollows.delete(threadId);
          if (!stopped && !followed.has(threadId)) follow(threadId, seq - 1);
        });
        pendingFollows.set(threadId, start);
      };

      // A terminal open when the window starts, or opened later, is read once; no polling.
      const stopTerminals = onTerminalChange(() => {
        for (const threadId of terminalThreads()) void readOnce(threadId);
      });
      for (const threadId of terminalThreads()) void readOnce(threadId);

      const stopNotice = sources.onNotice((threadId, seq) => {
        if (threadId === "" || !Number.isInteger(seq) || seq < 1) return;
        beginFollow(threadId, seq);
      });
      return () => {
        stopped = true;
        controller.abort();
        stopNotice();
        stopTerminals?.();
        for (const stop of followed.values()) stop();
        followed.clear();
        pendingFollows.clear();
        reads.clear();
        pump();
      };
    },
  };
}
