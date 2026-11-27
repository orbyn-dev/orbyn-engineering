import { Trash2 } from "lucide-react";
import { useState, type ReactNode } from "react";
import { canSendPersonKey } from "../../../api/personKey";
import type { TerminalInfo } from "../../../api/types";
import type { MenuItem } from "../../../ui/Menu";
import { clearTerminalBlocks } from "./blocks/blocksClient";
import { ClearBlocksDialog } from "./ClearBlocksDialog";

/** The words a clear leaves (frame block-clear-empty): what was dropped, then a running command that stayed. */
function clearedToast(cleared: number, keptRunning: number): string {
  if (cleared === 0) return "There were no blocks to clear.";
  const blocks = `Cleared ${cleared} ${cleared === 1 ? "block" : "blocks"}.`;
  return keptRunning === 0 ? blocks : `${blocks} ${keptRunning} running ${keptRunning === 1 ? "command" : "commands"} kept.`;
}

export interface ClearAllBlocks {
  /** The tab menu's items for these panes: one Clear all blocks…, or none where this page cannot send the person key. */
  itemsFor(panes: TerminalInfo[]): MenuItem[];
  /** How many times each terminal's blocks were cleared, so its block list reads again from the start. */
  clears: Readonly<Record<string, number>>;
  /** The confirmation to draw while one is open. */
  dialog: ReactNode;
}

/**
 * Clear all blocks for a terminal tab (parity G17; frames block-clear-menu, block-clear-confirm and block-clear-empty). The route is
 * Person-class and per terminal, so a split tab sends one write for each pane; a split tab's one press still makes one toast.
 */
export function useClearAllBlocks({
  threadId,
  fail,
  pushToast,
}: {
  threadId: string;
  /** Says a clear that failed, once. */
  fail(failure: unknown): void;
  pushToast(text: string): void;
}): ClearAllBlocks {
  const [clearing, setClearing] = useState<TerminalInfo[] | null>(null);
  const [clears, setClears] = useState<Readonly<Record<string, number>>>({});
  const clear = async (panes: TerminalInfo[]) => {
    const answers = await Promise.allSettled(panes.map((pane) => clearTerminalBlocks({ threadId, terminalId: pane.id })));
    const clearedIds: string[] = [];
    let cleared = 0;
    let kept = 0;
    for (const [index, pane] of panes.entries()) {
      const answer = answers[index];
      if (!answer || answer.status !== "fulfilled") continue;
      clearedIds.push(pane.id);
      cleared += answer.value.cleared;
      kept += answer.value.kept_running;
    }
    if (clearedIds.length > 0) {
      setClears((old) => {
        const next = { ...old };
        for (const id of clearedIds) next[id] = (next[id] ?? 0) + 1;
        return next;
      });
    }
    const failed = answers.find((answer): answer is PromiseRejectedResult => answer.status === "rejected");
    if (failed) fail(failed.reason);
    else pushToast(clearedToast(cleared, kept));
  };
  return {
    itemsFor: (panes) => (canSendPersonKey() ? [{ label: "Clear all blocks…", icon: <Trash2 size={14} aria-hidden />, onSelect: () => setClearing(panes) }] : []),
    clears,
    dialog: clearing ? <ClearBlocksDialog onConfirm={() => clear(clearing)} onClose={() => setClearing(null)} /> : null,
  };
}
