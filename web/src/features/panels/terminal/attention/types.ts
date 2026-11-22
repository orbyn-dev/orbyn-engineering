/** The two shapes the service client and the contract share, kept apart so neither imports the other. */

/** One thread-log event as the stream delivers it. `event` is untrusted until `parseAttention` accepts it. */
export interface AttentionDelivery {
  seq: number;
  ts: string;
  thread_id: string;
  event: unknown;
}

export interface AttentionApi {
  /** Events for every thread the person can open. Returns the function that stops them. Reads only. */
  subscribe(onEvent: (delivery: AttentionDelivery) => void): () => void;
}

