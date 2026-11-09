import type { KeyboardEvent, MutableRefObject } from "react";
import type { Thread } from "../../../api/types";
import { exitCodeBlock, indentCodeLines, openFenceAt } from "./composerCodeBlock";
import { continueListLine, indentListLine, recallStep, type RecallPosition } from "./composerInput";
import type { Suggestion } from "./MentionMenu";

/** What a keystroke in the field needs from the composer that owns it. */
export interface KeyContext {
  thread: Thread;
  draft: string;
  menuOpen: boolean;
  matches: Suggestion[];
  active: number;
  history: string[];
  recall: RecallPosition | null;
  planControls: boolean;
  planMode: boolean;
  busy: boolean;
  sendShortcut: "enter" | "mod-enter-multiline" | "mod-enter";
  shiftPaste: MutableRefObject<boolean>;
  caretAfterEdit: MutableRefObject<number | null>;
  selectionEndAfterEdit: MutableRefObject<number | null>;
  setHighlight(index: number): void;
  setMenuDismissed(dismissed: boolean): void;
  setRecall(position: RecallPosition | null): void;
  setDraft(threadId: string, text: string): void;
  pick(suggestion: Suggestion): void;
  setPlanMode(on: boolean): Promise<void>;
  send(followUp?: "default" | "opposite"): Promise<boolean>;
}

/** Arrows, Tab and Enter in the field: menu, history recall, code blocks, lists, send. */
export function handleComposerKey(
  event: KeyboardEvent<HTMLTextAreaElement>,
  {
    thread,
    draft,
    menuOpen,
    matches,
    active,
    history,
    recall,
    planControls,
    planMode,
    busy,
    sendShortcut,
    shiftPaste,
    caretAfterEdit,
    selectionEndAfterEdit,
    setHighlight,
    setMenuDismissed,
    setRecall,
    setDraft,
    pick,
    setPlanMode,
    send,
  }: KeyContext,
) {
  if (menuOpen) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setHighlight((active + step + matches.length) % matches.length);
      return;
    }
    if ((event.key === "Enter" && !event.shiftKey) || event.key === "Tab") {
      event.preventDefault();
      const suggestion = matches[active];
      if (suggestion) pick(suggestion);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setMenuDismissed(true);
      return;
    }
  }
  if (event.nativeEvent.isComposing) return;
  shiftPaste.current = event.shiftKey && event.key.toLowerCase() === "v";
  const element = event.currentTarget;
  if ((event.key === "ArrowUp" || event.key === "ArrowDown") && !event.metaKey && !event.ctrlKey && !event.altKey) {
    const edge = event.key === "ArrowUp" ? element.selectionStart === 0 : element.selectionEnd === draft.length;
    const step = edge ? recallStep(history, recall, draft, event.key === "ArrowUp" ? -1 : 1) : null;
    if (step) {
      event.preventDefault();
      setRecall(step.position);
      setDraft(thread.id, step.text);
    }
    return;
  }
  if (event.key === "Tab" && !menuOpen) {
    const code = indentCodeLines(draft, element.selectionStart, element.selectionEnd, event.shiftKey);
    if (code) {
      event.preventDefault();
      setDraft(thread.id, code.text);
      caretAfterEdit.current = code.start;
      // A multi-line selection stays selected, so Tab and Shift+Tab can repeat over it.
      selectionEndAfterEdit.current = code.end;
      return;
    }
    const next = indentListLine(draft, element.selectionStart, event.shiftKey);
    if (next) {
      event.preventDefault();
      setDraft(thread.id, next.text);
      caretAfterEdit.current = next.caret;
      return;
    }
    if (event.shiftKey && planControls && !busy && thread.lifecycle) {
      event.preventDefault();
      void setPlanMode(!planMode);
      return;
    }
  }
  if (event.key !== "Enter") return;
  // Left to the shell, whose rule sends and opens a new thread.
  if (event.altKey && (event.metaKey || event.ctrlKey)) return;
  // A fenced block keeps its own lines; only Mod+Enter leaves it to send.
  if (!event.shiftKey && !event.metaKey && !event.ctrlKey && openFenceAt(draft, element.selectionStart)) {
    const exit = exitCodeBlock(draft, element.selectionStart);
    if (exit) {
      event.preventDefault();
      setDraft(thread.id, exit.text);
      caretAfterEdit.current = exit.caret;
      return;
    }
    event.preventDefault();
    const at = element.selectionStart;
    setDraft(thread.id, `${draft.slice(0, at)}\n${draft.slice(element.selectionEnd)}`);
    caretAfterEdit.current = at + 1;
    return;
  }
  if (event.shiftKey && !event.metaKey && !event.ctrlKey) {
    const next = continueListLine(draft, element.selectionStart);
    if (next) {
      event.preventDefault();
      setDraft(thread.id, next.text);
      caretAfterEdit.current = next.caret;
    }
    return;
  }
  const mod = event.metaKey || event.ctrlKey;
  const multiline = draft.includes("\n");
  const enterSends = sendShortcut === "enter" || (sendShortcut === "mod-enter-multiline" && !multiline);
  if (!enterSends && !mod) return;
  event.preventDefault();
  void send(busy && (enterSends ? mod : event.shiftKey) ? "opposite" : "default");
}
