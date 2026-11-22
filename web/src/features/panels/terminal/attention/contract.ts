/**
 * CLI-agent attention (parity G14 extended, W10). The service detects an engine console that waits for the person and writes a `CUSTOM`
 * event on the thread's log (docs/cli-agent-attention.md). In the app `attentionApi()` is the service client (`client.ts`), which
 * reads the open items once for the threads with a terminal open in this window and once before following a thread after the shell
 * stream announced it. The mock lives in web/mock/attention.ts and is installed only by the dev-only preview page
 * (`#/preview/attention`) and by tests.
 */

import { createServiceAttentionApi } from "./client";
import { ATTENTION_EVENT, MAX_OPEN_ITEMS, type AttentionApi } from "./types";

export type { AttentionApi, AttentionDelivery } from "./types";
export { ATTENTION_EVENT };

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
  /** When the terminal last printed (epoch milliseconds, parsed from the RFC 3339 string on the wire). */
  since: number;
  /** The last lines, cleaned and capped. For a secret, only the prompt's label. Text, never markup. */
  excerpt: string[];
}

export interface AttentionClear {
  op: "clear";
  id: string;
  reason: ClearReason;
}

export type TerminalAttentionEvent = AttentionWaiting | AttentionClear;

/** Limits, in step with docs/cli-agent-attention.md. */
export const LIMITS = { idChars: 64, titleChars: 80, engineChars: 40, lines: 6, lineChars: 200, excerptChars: 1000, labelChars: 80, entries: 64, openItems: MAX_OPEN_ITEMS } as const;

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

let current: AttentionApi | null = null;
let service: AttentionApi | null = null;

/** The installed test or preview client, else the service client. */
export function attentionApi(): AttentionApi {
  service ??= createServiceAttentionApi();
  return current ?? service;
}

/** Tests and the dev-only preview install a client here; `null` puts back the service client. */
export function setAttentionApi(next: AttentionApi | null): void {
  current = next;
}
