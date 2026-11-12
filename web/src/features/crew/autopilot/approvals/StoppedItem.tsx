import { Button } from "../../../../ui/controls";
import { Chip, RISK_LABEL } from "./parts";
import type { Risk } from "./model";

export interface StoppedRun {
  title: string;
  risk: Risk;
  /** "Background crew · Fri 6 Nov · Claude Code · Sonnet 5.5 · crew/ci-fix · 2 of 3 changes finished" */
  meta: string;
  checks: string;
  notVerified: number;
  budget: string;
}

/**
 * A background run that reached its budget, as a Review queue item: Stopped with the reason "budget",
 * not Failed, with what it finished and what it didn't. Amber: it needs you to raise the budget or let it be.
 */
export function StoppedBudgetItem({ run, onOpen, onRaise }: { run: StoppedRun; onOpen?(): void; onRaise?(): void }) {
  return (
    <article className="ap-box ap-box-am" aria-label={`${run.title} · Stopped · budget`}>
      <div className="ap-chips"><Chip tone="plain">{RISK_LABEL[run.risk].replace(" risk", "")}</Chip></div>
      <h4>{run.title} · Stopped · budget</h4>
      <p className="ap-muted">{run.meta}</p>
      <p className="ap-muted">Checks {run.checks} · Not verified: {run.notVerified} · {run.budget}</p>
      <div className="ap-acts">
        {onOpen ? <Button small onClick={onOpen}>Open what it finished</Button> : null}
        {onRaise ? <Button small tone="primary" onClick={onRaise}>Raise the budget and continue</Button> : null}
      </div>
      <p><b>Stopped · budget</b> It reached its budget. What it finished is merged or ready; the rest is listed.</p>
    </article>
  );
}
