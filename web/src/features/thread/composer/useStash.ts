import { useState, type RefObject } from "react";
import type { Thread } from "../../../api/types";
import { useUi, type StashedPrompt } from "../../../product/state/ui";

/** Parks the draft for later and brings a parked one back. */
export function useStash({
  thread,
  draft,
  stashes,
  field,
  setDraft,
}: {
  thread: Thread;
  draft: string;
  stashes: StashedPrompt[];
  field: RefObject<HTMLTextAreaElement | null>;
  setDraft(threadId: string, text: string): void;
}) {
  const [stashOpen, setStashOpen] = useState(false);
  const stash = () => {
    const stored = useUi.getState().stashes;
    const list = stored[thread.id] ?? [];
    const { setPrefs: save } = useUi.getState();
    if (draft.trim()) {
      const entry = { id: crypto.randomUUID(), text: draft, at: new Date().toISOString() };
      save({ stashes: { ...stored, [thread.id]: [entry, ...list] } });
      setDraft(thread.id, "");
      useUi.getState().pushToast("Prompt stashed");
    } else if (list.length === 1 && list[0]) {
      restoreStash(list[0].id);
    } else if (list.length > 1) {
      setStashOpen(true);
    }
  };

  const dropStash = (id: string) => {
    const stored = useUi.getState().stashes;
    useUi.getState().setPrefs({
      stashes: { ...stored, [thread.id]: (stored[thread.id] ?? []).filter((entry) => entry.id !== id) },
    });
  };

  const restoreStash = (id: string) => {
    const entry = stashes.find((candidate) => candidate.id === id);
    if (!entry) return;
    setDraft(thread.id, entry.text);
    dropStash(id);
    setStashOpen(false);
    field.current?.focus();
  };
  return { stashOpen, setStashOpen, stash, dropStash, restoreStash };
}
