/**
 * CLI-agent attention (parity G14 extended, W10), built hidden. The service detects a waiting engine console (on by default,
 * `terminal_attention_enabled`, 20 s quiet time) and sends the event written in docs/cli-agent-attention.md; the web is not wired to it. `attentionApi()` answers `null` in a production build, so nothing subscribes,
 * no Needs you item is drawn and no notification is raised. The mock lives in web/mock/attention.ts and is installed only by the
 * dev-only preview page (`#/preview/attention`).
 */

/** The AG-UI `CUSTOM` event name the service uses on the thread's event log. */
export const ATTENTION_EVENT = "orbyn.terminal_attention";

export const ATTENTION_KINDS = ["question", "yes_no", "secret"] as const;
export type AttentionKind = (typeof ATTENTION_KINDS)[number];

export const CLEAR_REASONS = ["input", "output", "closed"] as const;
export type ClearReason = (typeof CLEAR_REASONS)[number];

export interface AttentionWaiting {
  op: "waiting";
  /** Service-made, stable for one waiting episode. Opaque: a Map key only. */
  id: string;
  terminal_id: string;
  terminal_title: string;
  /** Engine family id, such as `claude` or `codex`. */
  engine: string;
  kind: AttentionKind;
  /** When the terminal last printed: RFC 3339, UTC, as on the wire. The entry keeps it as epoch milliseconds. */
  since: string;
  /** The last lines, cleaned and capped. For a secret, only the prompt's label. Text, never markup. */
  excerpt: string[];
}

export interface AttentionClear {
  op: "clear";
  id: string;
  reason: ClearReason;
}

export type TerminalAttentionEvent = AttentionWaiting | AttentionClear;

/** One thread-log event as the stream delivers it. `event` is untrusted until `parseAttention` accepts it. */
export interface AttentionDelivery {
  seq: number;
  ts: string;
  thread_id: string;
  event: unknown;
}

/** Limits, in step with docs/cli-agent-attention.md. */
export const LIMITS = { idChars: 64, titleChars: 80, engineChars: 40, lines: 6, lineChars: 200, excerptChars: 1000, labelChars: 80, entries: 64 } as const;

/** One waiting episode of one terminal, as the web keeps it. */
export interface AttentionEntry {
  id: string;
  threadId: string;
  terminalId: string;
  terminalTitle: string;
  engine: string;
  kind: AttentionKind;
  since: number;
  /** When the web was told (the event's time). */
  toldAt: number;
  excerpt: string[];
  /** The newest event applied to this id; an older one changes nothing. */
  seq: number;
  /** Dismissed on this window only; changes nothing on the service. */
  dismissed: boolean;
  cleared: { reason: ClearReason; at: number } | null;
}

export interface AttentionApi {
  /** Events for every thread the person can open. Returns the function that stops them. Reads only. */
  subscribe(onEvent: (delivery: AttentionDelivery) => void): () => void;
}

let current: AttentionApi | null = null;

export const attentionApi = (): AttentionApi | null => current;

/** Tests and the dev-only preview install a client here; `null` puts back the production default, which sends nothing. */
export function setAttentionApi(next: AttentionApi | null): void {
  current = next;
}
