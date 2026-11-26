import { useNow } from "../../../../lib/useNow";
import { useAttention, type AttentionEntry } from "./store";
import { engineName } from "./text";

/**
 * The silence strip under the terminal drawer's tab row (Engineer Designs attention-badge): the engine console on screen stopped
 * printing and waits for the person. Copy and treatment come from the frame: an amber dot, "Waiting for you" in the amber text,
 * then the engine and how long it has been silent. Answer here is not built (attention-answer, Planned), so the strip has no action.
 * It leaves with the waiting item, when the event clears or the item is dismissed on this window.
 */
export function SilenceStrip({ threadId, terminalIds }: { threadId: string; terminalIds: readonly string[] }) {
  const entries = useAttention((state) => state.entries);
  const waiting = waitingOf(entries, threadId, terminalIds);
  // The seconds tick while the strip shows, as the drawer's other clocks do.
  const now = useNow(waiting !== undefined);
  if (!waiting) return null;
  const silent = Math.max(0, Math.round((now - waiting.since) / 1000));
  return (
    <div className="term-wait">
      <span className="status-dot term-wait-dot" aria-hidden />
      <span>
        <span className="term-wait-label">Waiting for you</span> · {engineName(waiting.engine)} has been silent for {silent} s
      </span>
    </div>
  );
}

/** The first terminal on screen that waits now, not cleared and not dismissed here; the same rule as the tab and row dots. */
function waitingOf(entries: ReadonlyMap<string, AttentionEntry>, threadId: string, terminalIds: readonly string[]): AttentionEntry | undefined {
  const byTerminal = new Map<string, AttentionEntry>();
  for (const entry of entries.values()) {
    if (entry.threadId !== threadId || entry.cleared !== null || entry.dismissed) continue;
    byTerminal.set(entry.terminalId, entry);
  }
  for (const terminalId of terminalIds) {
    const entry = byTerminal.get(terminalId);
    if (entry) return entry;
  }
  return undefined;
}
