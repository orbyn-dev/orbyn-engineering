import { useMemo } from "react";
import { create } from "zustand";
import { ATTENTION_EVENT, type AttentionEntry, ATTENTION_KINDS, CLEAR_REASONS, LIMITS, type AttentionDelivery, type TerminalAttentionEvent } from "./contract";
import { setWaitingThreads } from "../../../../product/state/waitingThreads";
import { cleanExcerpt, oneLine } from "./text";

export type { AttentionEntry };

export interface AttentionState {
  entries: ReadonlyMap<string, AttentionEntry>;
  /** The newest clear's seq per id, so a waiting that arrives late cannot bring a cleared item back. */
  cleared: ReadonlyMap<string, number>;
}

export const emptyAttention = (): AttentionState => ({ entries: new Map(), cleared: new Map() });

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const isId = (value: unknown): value is string => typeof value === "string" && value !== "" && value.length <= LIMITS.idChars;

/** Accepts the event's value only when it is exactly what the doc says; anything else is ignored, never repaired. */
export function parseAttention(value: unknown): TerminalAttentionEvent | null {
  if (!isRecord(value) || value["v"] !== 1 || !isId(value["id"])) return null;
  const id = value["id"];
  if (value["op"] === "clear") {
    const reason = CLEAR_REASONS.find((one) => one === value["reason"]);
    return reason ? { op: "clear", id, reason } : null;
  }
  if (value["op"] !== "waiting") return null;
  const kind = ATTENTION_KINDS.find((one) => one === value["kind"]);
  const since = typeof value["since"] === "string" ? Date.parse(value["since"]) : Number.NaN;
  const lines = value["excerpt"];
  if (!kind || Number.isNaN(since) || !Array.isArray(lines) || !lines.every((line) => typeof line === "string")) return null;
  const { terminal_id: terminal, terminal_title: title, engine } = value;
  if (!isId(terminal) || typeof title !== "string" || typeof engine !== "string") return null;
  return {
    op: "waiting", id, terminal_id: terminal, terminal_title: oneLine(title, LIMITS.titleChars), engine: oneLine(engine, LIMITS.engineChars),
    kind, since, excerpt: cleanExcerpt(lines as string[], kind === "secret"),
  };
}

function capped(entries: Map<string, AttentionEntry>): Map<string, AttentionEntry> {
  if (entries.size <= LIMITS.entries) return entries;
  // Cleared and dismissed ones go first, then the oldest by seq.
  const order = [...entries.values()].sort((a, b) => Number(b.cleared !== null || b.dismissed) - Number(a.cleared !== null || a.dismissed) || a.seq - b.seq);
  for (const gone of order.slice(0, entries.size - LIMITS.entries)) entries.delete(gone.id);
  return entries;
}

/** The pure step: the next state, and the entry when this event made a new waiting item (what a notification is raised for). */
export function applyAttention(state: AttentionState, delivery: AttentionDelivery): { state: AttentionState; fresh: AttentionEntry | null } {
  const event = delivery.event;
  if (!isRecord(event) || event["type"] !== "CUSTOM" || event["name"] !== ATTENTION_EVENT) return { state, fresh: null };
  const parsed = parseAttention(event["value"]);
  if (!parsed) return { state, fresh: null };
  const before = state.entries.get(parsed.id);
  const at = Number.isNaN(Date.parse(delivery.ts)) ? Date.now() : Date.parse(delivery.ts);
  if (parsed.op === "clear") {
    const cleared = new Map(state.cleared);
    cleared.set(parsed.id, Math.max(cleared.get(parsed.id) ?? 0, delivery.seq));
    if (cleared.size > LIMITS.entries * 4) cleared.delete(cleared.keys().next().value as string);
    if (!before || delivery.seq <= before.seq) return { state: { entries: state.entries, cleared }, fresh: null };
    const entries = new Map(state.entries);
    entries.set(parsed.id, { ...before, seq: delivery.seq, cleared: { reason: parsed.reason, at } });
    return { state: { entries, cleared }, fresh: null };
  }
  if (before && delivery.seq <= before.seq) return { state, fresh: null };
  const clearedAt = state.cleared.get(parsed.id);
  if (clearedAt !== undefined && delivery.seq <= clearedAt) return { state, fresh: null };
  const entry: AttentionEntry = {
    id: parsed.id, threadId: delivery.thread_id, terminalId: parsed.terminal_id, terminalTitle: parsed.terminal_title, engine: parsed.engine,
    kind: parsed.kind, since: parsed.since, toldAt: before?.toldAt ?? at, excerpt: parsed.excerpt, seq: delivery.seq,
    dismissed: before?.dismissed ?? false, cleared: null,
  };
  const entries = new Map(state.entries);
  entries.set(parsed.id, entry);
  // A value read from the open-items route (seq 0) was already waiting before the window opened: it shows, but is not new, so it
  // raises no toast or notification.
  return { state: { entries: capped(entries), cleared: state.cleared }, fresh: before || delivery.seq <= 0 || delivery.source === "open" ? null : entry };
}

interface AttentionStore extends AttentionState {
  /** Applies one delivery; returns the entry when it is a new waiting item. */
  receive(delivery: AttentionDelivery): AttentionEntry | null;
  dismiss(id: string): void;
  reset(): void;
}

export const useAttention = create<AttentionStore>((set, get) => ({
  ...emptyAttention(),
  receive(delivery) {
    const next = applyAttention(get(), delivery);
    if (next.state.entries !== get().entries || next.state.cleared !== get().cleared) set({ entries: next.state.entries, cleared: next.state.cleared });
    return next.fresh;
  },
  dismiss(id) {
    const entry = get().entries.get(id);
    if (!entry || entry.dismissed) return;
    set({ entries: new Map(get().entries).set(id, { ...entry, dismissed: true }) });
  },
  reset: () => set(emptyAttention()),
}));

const isWaiting = (entry: AttentionEntry): boolean => entry.cleared === null && !entry.dismissed;

/** The terminals of one thread that wait for the person now: not cleared, not dismissed on this window. */
export function waitingTerminalIds(entries: ReadonlyMap<string, AttentionEntry>, threadId: string): ReadonlySet<string> {
  const ids = new Set<string>();
  for (const entry of entries.values()) if (entry.threadId === threadId && isWaiting(entry)) ids.add(entry.terminalId);
  return ids;
}

/** The waiting terminals of the thread (the terminal tabs' dot). */
export function useWaitingTerminals(threadId: string): ReadonlySet<string> {
  const entries = useAttention((state) => state.entries);
  return useMemo(() => waitingTerminalIds(entries, threadId), [entries, threadId]);
}

// The sidebar reads which threads wait through a neutral store, so it need not import this feature.
useAttention.subscribe((state, before) => {
  if (state.entries === before.entries) return;
  const ids = new Set<string>();
  for (const entry of state.entries.values()) if (isWaiting(entry)) ids.add(entry.threadId);
  setWaitingThreads(ids);
});
