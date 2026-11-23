/** The two shapes the service client and the contract share, kept apart so neither imports the other. */

import type { TerminalAttentionOpen } from "../../../../generated/TerminalAttentionOpen";

export type { TerminalAttentionOpen };

/** The AG-UI `CUSTOM` event name the service uses on the thread's event log. */
export const ATTENTION_EVENT = "orbyn.terminal_attention";

/** The service caps a thread's open items at 8; a window accepts no more, whatever arrives. */
export const MAX_OPEN_ITEMS = 8;

/** One thread-log event as the stream delivers it. `event` is untrusted until `parseAttention` accepts it. */
export interface AttentionDelivery {
  seq: number;
  ts: string;
  thread_id: string;
  event: unknown;
  /**
   * `open` for a value read from `GET /api/threads/{id}/terminal-attention`: it is shown like a log item, but it is not new, so
   * it raises no toast and no notification. Absent means a log event.
   */
  source?: "log" | "open";
}

export interface AttentionApi {
  /** Events for every thread the person can open. Returns the function that stops them. Reads only. */
  subscribe(onEvent: (delivery: AttentionDelivery) => void): () => void;
}

