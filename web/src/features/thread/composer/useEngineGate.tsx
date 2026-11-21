import { useEffect, useRef, useState } from "react";
import type { EngineStatus, Thread } from "../../../api/types";
import { refusalSentence, refusedModes } from "../../../product/composer/access";
import { toFailure } from "../../../lib/useAsync";
import { cursorThreadApi, displayText, NoticeCard, RefusedLine, runRefusal } from "../../cursorthread";

/**
 * What a thread on an engine that never asks before acting adds to the composer, all read from the service's answers:
 * the first-run notice card (409 `engine_notice_required`) and the refused-mode line (409 `engine_mode_refused`, or a press on a
 * dimmed access row). Nothing here names an engine.
 */
export function useEngineGate(thread: Thread, engine: EngineStatus | null, setError: (message: string | null) => void) {
  const [noticeOpen, setNoticeOpen] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [acknowledging, setAcknowledging] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const writing = useRef(false);
  const engineName = engine?.name ?? thread.engine;
  const modes = refusedModes(engine);
  const sentence = refusalSentence(engineName, modes);

  // A different engine ends the path: nothing is owed for the engine the thread left.
  useEffect(() => {
    setNoticeOpen(false);
    setChoosing(false);
    setRefusal(null);
  }, [thread.engine]);

  /** A failed run start that this gate owns: true when it drew a card or a line, false when the composer's own error applies. */
  const onSendFailure = (failure: unknown): boolean => {
    const refused = runRefusal(failure);
    if (!refused) return false;
    if (refused.kind === "notice") setNoticeOpen(true);
    else setRefusal(refused.message ?? sentence);
    return true;
  };

  /** One click, one write: the acknowledgement, then the message goes out. A second click while it runs does nothing. */
  const acknowledge = async (resend: () => void) => {
    if (writing.current) return;
    writing.current = true;
    setAcknowledging(true);
    try {
      await cursorThreadApi().acknowledge(thread.engine);
      setNoticeOpen(false);
      resend();
    } catch (failure) {
      const reason = failure instanceof Error ? displayText(failure.message) : null;
      setError(`Couldn't save that. ${reason ?? toFailure(failure).message} Your message is kept.`);
    } finally {
      writing.current = false;
      setAcknowledging(false);
    }
  };

  /** Opens the existing model picker; the card steps aside while it is open and returns if it closes with the engine still chosen. */
  const choose = (from: HTMLElement) => {
    const trigger = from.closest("form")?.querySelector<HTMLButtonElement>("[data-model-trigger]");
    if (!trigger || trigger.disabled) return;
    setChoosing(true);
    let seenOpen = false;
    const watch = new MutationObserver(() => {
      const open = trigger.getAttribute("aria-expanded") === "true";
      if (open) seenOpen = true;
      else if (seenOpen) {
        watch.disconnect();
        setChoosing(false);
      }
    });
    watch.observe(trigger, { attributes: true, attributeFilter: ["aria-expanded"] });
    trigger.click();
  };

  const cards = (resend: () => void) => (
    <>
      {refusal ? <RefusedLine text={refusal} onDismiss={() => setRefusal(null)} /> : null}
      {noticeOpen && !choosing ? (
        <NoticeCard engineName={engineName} busy={acknowledging} onContinue={() => void acknowledge(resend)} onChoose={choose} />
      ) : null}
    </>
  );

  return {
    /** The notice is waiting: Send stays dimmed and the message stays in the box. */
    noticeOpen,
    onSendFailure,
    cards,
    clear: () => setRefusal(null),
    /** A press on a dimmed access row: nothing changes, and the line says why. */
    refused: { modes, sentence, onPick: () => setRefusal(sentence) },
  };
}
