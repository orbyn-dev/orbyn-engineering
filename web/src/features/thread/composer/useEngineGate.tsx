import { useEffect, useRef, useState } from "react";
import type { EngineStatus, Thread } from "../../../api/types";
import { refusalModeName } from "../../../product/composer/access";
import { toFailure } from "../../../lib/useAsync";
import { cursorThreadApi, displayText, ModeRefusedCard, NoticeCard, runRefusal } from "../../cursorthread";

/**
 * What a thread on an engine that never asks before acting adds to the composer, all read from the service's answers:
 * the first-run notice card (409 `engine_notice_required`, once per project) and the refused-mode card (409 `engine_mode_refused`).
 * When both apply the notice card is drawn first and the refusal card returns once the notice is answered. Nothing here names an engine.
 */
export function useEngineGate(thread: Thread, engine: EngineStatus | null, setError: (message: string | null) => void) {
  const [noticeOpen, setNoticeOpen] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [acknowledging, setAcknowledging] = useState(false);
  const [refused, setRefused] = useState(false);
  const [overriding, setOverriding] = useState(false);
  const writing = useRef(false);
  const engineName = engine?.name ?? thread.engine;
  const savedMode = thread.lifecycle?.permission_mode ?? "supervised";

  // A different engine ends the path: nothing is owed for the engine the thread left.
  useEffect(() => {
    setNoticeOpen(false);
    setChoosing(false);
    setRefused(false);
  }, [thread.engine]);

  /** A failed run start that this gate owns: true when it drew a card or a line, false when the composer's own error applies. */
  const onSendFailure = (failure: unknown): boolean => {
    const found = runRefusal(failure);
    if (!found) return false;
    if (found.kind === "notice") setNoticeOpen(true);
    else setRefused(true);
    return true;
  };

  /** One click, one write: the acknowledgement, then the message goes out. A second click while it runs does nothing. */
  const acknowledge = async (resend: () => void) => {
    if (writing.current) return;
    writing.current = true;
    setAcknowledging(true);
    try {
      await cursorThreadApi().acknowledge(thread.project_id, thread.engine);
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

  /**
   * One click, one write: the one-turn override. `run` sends this message with no approvals and stores nothing, so the thread's
   * saved mode stays. It is never remembered; a second click while it runs does nothing.
   */
  const override = async (run: () => Promise<boolean>) => {
    if (writing.current) return;
    writing.current = true;
    setOverriding(true);
    try {
      if (await run()) setRefused(false);
    } finally {
      writing.current = false;
      setOverriding(false);
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

  const cards = (resend: () => void, runWithout: () => Promise<boolean>) => (
    <>
      {refused && !choosing && !noticeOpen ? (
        <ModeRefusedCard engineName={engineName} modeName={refusalModeName(savedMode)} busy={overriding} onRun={() => void override(runWithout)} onChoose={choose} />
      ) : null}
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
    /** A refusal card is waiting: Send stays dimmed and the message stays in the box. */
    refused,
    clear: () => setRefused(false),
  };
}
