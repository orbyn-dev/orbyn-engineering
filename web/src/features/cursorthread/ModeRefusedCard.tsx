import { CircleAlert } from "lucide-react";
import { Button } from "../../ui/controls";

/**
 * The inline card above the composer when a run start is refused because the saved mode needs approvals the engine never asks.
 * The words are the frame's, not the service's: the service's sentence is display data and is not drawn. Run this thread without
 * approvals is the one write (a one-turn override, never remembered); Choose another engine writes nothing.
 */
export function ModeRefusedCard({ engineName, modeName, busy, onRun, onChoose }: { engineName: string; modeName: string; busy: boolean; onRun(): void; onChoose(button: HTMLElement): void }) {
  return (
    <div className="ct-notice" role="group" aria-label={`${engineName} can’t start in ${modeName} mode`}>
      <CircleAlert size={16} aria-hidden />
      <div className="ct-notice-body">
        <strong>{engineName} can’t start in {modeName} mode</strong>
        <span>{engineName} does not ask before acting, so it can’t run where approvals are required.</span>
        <span className="ct-quiet">Running without approvals applies to this thread only. Orbyn does not remember it.</span>
        <span className="ct-notice-actions">
          <Button small tone="primary" busy={busy} onClick={onRun}>Run this thread without approvals</Button>
          <Button small tone="quiet" disabled={busy} onClick={(event) => onChoose(event.currentTarget)}>Choose another engine</Button>
        </span>
      </div>
    </div>
  );
}
