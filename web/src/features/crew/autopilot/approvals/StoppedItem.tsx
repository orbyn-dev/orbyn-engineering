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
 * A background run that reached its budget, as a Review queue item: Stopped with the reason Budget,
 * not Failed, with what it finished and what it didn't. The row has one action, Open; the budget is set when the crew is created.
 */
export function StoppedBudgetItem({ run, onOpen }: { run: StoppedRun; onOpen?(): void }) {
  return (
    <article className="ap-box ap-box-am" aria-label={`${run.title} · Stopped · Budget`}>
      <div className="ap-chips"><Chip tone="plain">{RISK_LABEL[run.risk].replace(" risk", "")}</Chip><Chip tone="plain">Planned</Chip></div>
      <h4>{run.title} · Stopped · Budget</h4>
      <p className="ap-muted">{run.meta}</p>
      <p className="ap-muted">Checks {run.checks} · Not verified: {run.notVerified} · {run.budget}</p>
      <div className="ap-acts">
        {onOpen ? <Button small onClick={onOpen}>Open what it finished</Button> : null}
      </div>
      <p><b>Stopped · Budget</b> It reached its budget. What it finished is merged or ready; the rest is listed.</p>
    </article>
  );
}
