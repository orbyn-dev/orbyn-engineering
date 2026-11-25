import type { FlowGate } from "../../../../generated/FlowGate";
import type { FlowTaskDetail } from "../../../../generated/FlowTaskDetail";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { Button } from "../../../../ui/controls";
import { Ic, type FlowIconName } from "../icons";
import { decidedBy, gateBlocks, gateDecisionWord, gateStageWord, taskFinal, when } from "./model";
import { useFlowTask } from "./useFlow";
// The gates frame's own card rules: the shared design's card classes and the Flow frame CSS the sample draws them with.
import "../../shared/design.css";
import "../flow.css";
import "./flow-live.css";

/** The design's mark for who decided a gate: the machine, a confirmed finding or the person. */
const DECIDER_ICON: Record<string, FlowIconName> = { code: "cpu", confirmed_finding: "bug", person: "user" };

/**
 * The gates view of one Flow task (Crew Designs, frame `flow-gates`): one card per gate the per-task detail read serves,
 * titled by its stage and decision, saying who decided it, the reviewer lane when it names one, the decision's own time and,
 * only once the task is over, the reason the service then serves. The frame's "Gate states" legend names states this read
 * does not carry (every gate it serves is already decided), so it is left out; its static rules paragraph is the design's
 * copy, not a read, and is left out too. Read only: nothing is written.
 */
export function TaskGates({ id, name, onBack }: { id: string; name?: string; onBack(): void }) {
  const read = useFlowTask(id);
  const missing = read.error?.status === 404;
  return (
    <section className="fv-body fv-zoom" aria-label="Gates">
      <div className="fv-crumb">
        <Button small onClick={onBack}>Task</Button>
        <h2 tabIndex={-1} data-page-title>Gates</h2>
        {name ? <span className="fv-muted">{name}</span> : null}
      </div>
      {read.status === "loading" ? <Loading label="Reading the task’s gates" rows={3} /> : null}
      {read.status === "error" && !read.data ? (
        missing ? (
          <>
            <Empty title="This service has no gates read" />
            <p className="fv-muted">The gate decisions stay Planned.</p>
          </>
        ) : (
          <ErrorState title="Can’t read this task’s gates" failure={read.error} onRetry={() => void read.reload()} />
        )
      ) : null}
      {read.data ? <GatesBody detail={read.data} /> : null}
    </section>
  );
}

/** The frame's cards, one per gate the read serves. A reason is drawn only when the service serves one and the task is over. */
function GatesBody({ detail }: { detail: FlowTaskDetail }) {
  const final = taskFinal(detail.row.state);
  return (
    <>
      {detail.truncated ? <p className="fv-note" role="note">A list of this task was cut at its cap; the newest are shown.</p> : null}
      {detail.gates.length === 0 ? (
        <p className="fv-muted">No gate decisions yet.</p>
      ) : (
        <div className="fv-gates-cards">
          {detail.gates.map((gate, at) => (
            <GateCard key={`${gate.stage}:${gate.at}:${at}`} gate={gate} final={final} />
          ))}
        </div>
      )}
    </>
  );
}

/** One served gate as the frame draws it: its mark, its stage and decision, who decided it, its lane, its reason and its time. */
function GateCard({ gate, final }: { gate: FlowGate; final: boolean }) {
  return (
    <div className={gateBlocks(gate.decision) ? "cx-fbx cx-red" : "cx-fbx"}>
      <div className="fv-gate-head">
        <span className="cx-fgt cx-done"><Ic n={DECIDER_ICON[gate.decided_by] ?? "gate"} sm /></span>
        <h6>{gateStageWord(gate.stage)} → {gateDecisionWord(gate.decision)}</h6>
      </div>
      {final && gate.reason ? <p className="fv-gate-reason">{gate.reason}</p> : null}
      <div className="cx-fkv">
        <span>Decided by</span>
        <b>{decidedBy(gate)}</b>
        {gate.lane ? (
          <>
            <span>Lane</span>
            <b>{gate.lane}</b>
          </>
        ) : null}
        <span>At</span>
        <b>{when(gate.at)}</b>
      </div>
    </div>
  );
}
