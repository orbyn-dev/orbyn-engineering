import type { ClipboardEvent, KeyboardEvent, RefObject } from "react";
import type { EngineStatus, Thread } from "../../../api/types";
import type { composerMarkup } from "./composerMarkup";

/** The text area over its styled mirror. */
export function ComposerField({
  field,
  mirror,
  thread,
  engine,
  draft,
  styled,
  layout,
  compacting,
  serviceDown,
  pendingHere,
  setDraft,
  setCaret,
  setHighlight,
  setMenuDismissed,
  onKeyDown,
  onPaste,
}: {
  field: RefObject<HTMLTextAreaElement | null>;
  mirror: RefObject<HTMLDivElement | null>;
  thread: Thread;
  engine: EngineStatus | null;
  draft: string;
  styled: ReturnType<typeof composerMarkup> | null;
  layout: string;
  compacting: boolean;
  serviceDown: boolean;
  pendingHere: readonly unknown[];
  setDraft(threadId: string, text: string): void;
  setCaret(caret: number): void;
  setHighlight(index: number): void;
  setMenuDismissed(dismissed: boolean): void;
  onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>): void;
  onPaste(event: ClipboardEvent<HTMLTextAreaElement>): void;
}) {
  return (
    <div className="composer-field">
    {styled ? (
      <div ref={mirror} className="composer-mirror" aria-hidden>
        {styled.map((span, at) => (
          <span key={at} data-md={span.kind === "text" ? undefined : span.kind}>
            {span.text}
          </span>
        ))}
        {"\n"}
      </div>
    ) : null}
    <textarea
      ref={field}
      data-styled={styled ? true : undefined}
      onScroll={(event) => {
        if (mirror.current) mirror.current.scrollTop = event.currentTarget.scrollTop;
      }}
      rows={1}
      value={draft}
      onChange={(event) => {
        setDraft(thread.id, event.target.value);
        setCaret(event.target.selectionStart);
        setHighlight(0);
        setMenuDismissed(false);
      }}
      onSelect={(event) => setCaret(event.currentTarget.selectionStart)}
      onKeyDown={onKeyDown}
      onPaste={onPaste}
      dir="auto"
      aria-label={`Message ${thread.title}`}
      aria-describedby="composer-help"
      placeholder={
        compacting
          ? "Waiting for compact to finish"
          : serviceDown
          ? "Your draft is kept. Sending waits for the Orbyn service."
          : pendingHere.length > 0
          ? engine?.capabilities.decline_feedback
            ? "Type a reply to decline with feedback"
            : "Send a message to decline and redirect the agent"
          : "Ask anything. @ files, $ skills, / commands"
      }
      data-autofocus={layout === "phone" ? undefined : true}
    />
    </div>
  );
}
