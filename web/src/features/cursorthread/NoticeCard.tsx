import { Info } from "lucide-react";
import { unmanagedNoticeText } from "../../product/engines/unmanaged";
import { Button } from "../../ui/controls";

/** The notice's words, as the lead ruled them. Cursor keeps its own sentence; every other engine says Orbyn cannot ask. */
export const noticeText = (engineId: string, engineName: string) => unmanagedNoticeText(engineId, engineName);

/**
 * The inline card above the composer, drawn before an engine that never asks runs for the first time. Continue is the one
 * write (the notice's acknowledgement); Choose another engine is not a cancel and writes nothing. Drawing it stores nothing.
 */
export function NoticeCard({ engineId, engineName, busy, onContinue, onChoose }: { engineId: string; engineName: string; busy: boolean; onContinue(): void; onChoose(button: HTMLElement): void }) {
  return (
    <div className="ct-notice" role="group" aria-label={`Before the first ${engineName} run`}>
      <Info size={16} aria-hidden />
      <div className="ct-notice-body">
        <span>{noticeText(engineId, engineName)}</span>
        <span className="ct-notice-actions">
          <Button small tone="primary" busy={busy} onClick={onContinue}>Continue</Button>
          <Button small tone="quiet" disabled={busy} onClick={(event) => onChoose(event.currentTarget)}>Choose another engine</Button>
        </span>
      </div>
    </div>
  );
}
