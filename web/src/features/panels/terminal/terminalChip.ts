import { useUi } from "../../../product/state/ui";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { charactersOver, chipId, promptWithChips, type TerminalChip } from "../../../product/composer/composerContext";

/** Adds terminal output to the thread's message as a chip, unless it would not fit in one message. */
export function addTerminalChip(threadId: string, fields: Omit<TerminalChip, "id" | "kind">, tooBig: string): boolean {
  const chip: TerminalChip = { id: chipId(), kind: "terminal", ...fields };
  const chips = useComposerChips.getState().chips[threadId] ?? [];
  const draft = useUi.getState().drafts[threadId] ?? "";
  if (charactersOver(promptWithChips(draft, [...chips, chip])) > 0) {
    useUi.getState().pushToast(tooBig);
    return false;
  }
  useComposerChips.getState().add(threadId, chip);
  return true;
}
