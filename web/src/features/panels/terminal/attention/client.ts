import { followThread, type StreamHandlers } from "../../../../api/sse";
import { onThreadNotice } from "../../../../api/shell";
import type { AttentionApi, AttentionDelivery } from "./types";

/** Threads followed at once; past this the one followed longest ago is let go (a later notice follows it again). */
export const MAX_FOLLOWED = 32;

export interface AttentionSources {
  /** The shell stream's notice that a thread's log has something to read, with its `seq` and no payload. */
  onNotice(listener: (threadId: string, seq: number) => void): () => void;
  /** Follows one thread's event log. */
  follow(threadId: string, handlers: StreamHandlers): () => void;
}

const live: AttentionSources = { onNotice: onThreadNotice, follow: followThread };

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null;

/**
 * The service client: threads that never announced anything are not followed. A notice starts following that thread's log from just
 * before the announced `seq`, so the event arrives through the ordinary log stream (ordering and replay come with it) and a later
 * clear arrives the same way. Reads only. What an event means, and what is ignored, is decided by the store; here a frame that is not
 * a log event with a number `seq` is dropped, never thrown on.
 */
export function createServiceAttentionApi(sources: AttentionSources = live): AttentionApi {
  return {
    subscribe(onEvent) {
      const followed = new Map<string, () => void>();
      const follow = (threadId: string, from: number) => {
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
      const stopNotice = sources.onNotice((threadId, seq) => {
        if (threadId === "" || !Number.isInteger(seq) || seq < 1 || followed.has(threadId)) return;
        follow(threadId, seq - 1);
      });
      return () => {
        stopNotice();
        for (const stop of followed.values()) stop();
        followed.clear();
      };
    },
  };
}
