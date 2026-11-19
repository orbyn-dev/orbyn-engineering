import { useEffect } from "react";
import { authHeaders } from "../../../api/client";
import { reserveStreamConnection } from "../../../api/lanes";
import { serviceUrl } from "../../../api/origin";
import { dataChanged } from "../../../api/shell";
import { SseParser } from "../../../api/sse";
import type { Row } from "./model";

const RETRY_BASE_MS = 3000;
const RETRY_MAX_MS = 60_000;
/** A stream that closes sooner than this was not healthy, whatever it sent first. */
const HEALTHY_MS = 30_000;

/** Only unfolded worktrees own watches, including their loading/error/empty state. */
export function watchedWorktrees(rows: readonly Row[]): string {
  const pairs = new Map<string, [string, string]>();
  for (const row of rows) {
    if (row.kind !== "worktree" || !row.open || row.missing || !row.worktree) continue;
    const pair: [string, string] = [row.projectId, row.worktree.id];
    pairs.set(JSON.stringify(pair), pair);
  }
  return JSON.stringify([...pairs.values()].slice(0, 32));
}

/** One connection per mounted explorer; blur closes it and releases server leases. */
export function followExplorer(worktrees: string): () => void {
  let controller: AbortController | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  let failures = 0;
  const active = () => !document.hidden && document.hasFocus();
  const disconnect = () => {
    controller?.abort();
    controller = undefined;
    if (retry !== undefined) clearTimeout(retry);
    retry = undefined;
  };
  const connect = async () => {
    if (stopped || !active() || controller || worktrees === "[]") return;
    if (retry !== undefined) clearTimeout(retry);
    retry = undefined;
    const current = new AbortController();
    controller = current;
    let release: (() => void) | undefined;
    let ready = false;
    let refused = false;
    let opened = 0;
    try {
      release = await reserveStreamConnection(current.signal);
      if (current.signal.aborted) return;
      const response = await fetch(serviceUrl(`/api/explorer/events?worktrees=${encodeURIComponent(worktrees)}`), {
        signal: current.signal,
        credentials: "same-origin",
        headers: { ...authHeaders(), Accept: "text/event-stream" },
      });
      if (!response.ok || !response.body) {
        // A 4xx will not heal by waiting; a changed worktree set remounts the effect.
        refused = response.status >= 400 && response.status < 500;
        throw new Error("no explorer stream");
      }
      opened = Date.now();
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      const parser = new SseParser();
      const cancel = () => { void reader.cancel().catch(() => undefined); };
      current.signal.addEventListener("abort", cancel);
      try {
        for (;;) {
          const { value, done } = await reader.read();
          if (done || current.signal.aborted) break;
          for (const data of parser.push(value)) {
            const event: unknown = JSON.parse(data);
            ready = true;
            if (typeof event === "object" && event !== null && "type" in event && event.type === "data_changed"
              && "what" in event && typeof event.what === "string" && event.what.startsWith("explorer:")) {
              dataChanged(event.what);
            }
          }
        }
      } finally {
        current.signal.removeEventListener("abort", cancel);
        await reader.cancel().catch(() => undefined);
        reader.releaseLock();
      }
    } catch {
      // Reconnect the subscription, never poll for file freshness.
    } finally {
      release?.();
      if (controller === current) controller = undefined;
      // Start the backoff over only after a stream that said ready and then stayed up.
      failures = ready && Date.now() - opened >= HEALTHY_MS ? 0 : failures + 1;
      if (!stopped && !refused && !current.signal.aborted && active()) {
        const delay = Math.min(RETRY_BASE_MS * 2 ** Math.max(failures - 1, 0), RETRY_MAX_MS);
        retry = setTimeout(() => void connect(), delay);
      }
    }
  };
  const focus = () => { if (active()) void connect(); else disconnect(); };
  window.addEventListener("focus", focus);
  window.addEventListener("blur", disconnect);
  document.addEventListener("visibilitychange", focus);
  void connect();
  return () => {
    stopped = true;
    disconnect();
    window.removeEventListener("focus", focus);
    window.removeEventListener("blur", disconnect);
    document.removeEventListener("visibilitychange", focus);
  };
}

export function useExplorerWatch(rows: readonly Row[]): void {
  const worktrees = watchedWorktrees(rows);
  useEffect(() => followExplorer(worktrees), [worktrees]);
}
