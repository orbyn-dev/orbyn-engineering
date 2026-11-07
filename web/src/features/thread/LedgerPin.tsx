import { Button } from "../../ui/controls";
import { useUi } from "../../state/ui";
import { useWorkspace } from "../../state/workspace";
import { rowWhere } from "./threadSearch";
import { transcriptFor } from "./transcriptRegistry";

/**
 * The Ledger layout pins what the thread is waiting on above the transcript, so the
 * decision stays in view however far the person has scrolled. It only points at the
 * decision: approving and declining stay on the card itself, where the detail is, and
 * Show focuses the card so the next Tab reaches the choices.
 */
export function LedgerPin({ threadId }: { threadId: string }) {
  const layout = useUi((state) => state.workspaceLayout);
  const approvals = useWorkspace((state) => state.approvals.data);
  const pending = (approvals ?? []).filter(
    (approval) =>
      approval.thread_id === threadId && approval.state === "pending",
  );
  const first = pending[0];
  if (layout !== "ledger" || !first) return null;
  const show = async () => {
    const find = () => document.querySelector<HTMLElement>(".transcript .approval[data-pending]");
    let card = find();
    if (!card) {
      // The card is out of the window: bring its row in first.
      const transcript = transcriptFor(threadId);
      const at = transcript ? rowWhere(transcript.entries(), (item) => item.kind === "approval" && item.approval.state === "pending") : -1;
      if (transcript && at >= 0) await transcript.reveal(at, "center");
      card = find();
    }
    if (!card) return;
    const still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    card.scrollIntoView?.({ block: "center", behavior: still ? "auto" : "smooth" });
    // Focus the card, not a button: a second key press must never decide the request.
    card.focus({ preventScroll: true });
  };
  return (
    <div className="ledger-pin" role="status">
      <span className="status-dot" aria-hidden />
      <span className="ledger-pin-label">Needs you</span>
      <span className="ledger-pin-text">
        {pending.length > 1 ? `${pending.length} decisions waiting. ` : null}
        {first.summary}
      </span>
      <Button small tone="quiet" onClick={() => void show()}>
        Show
      </Button>
    </div>
  );
}
