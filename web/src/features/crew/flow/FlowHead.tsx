import { Btn, Ic } from "../shared/parts";

const AFTER_THE_BETA = "After the beta: no route yet. The beta has a Cancel on each task, when it is wired.";

/** The Crew page title as the Flow frames draw it: Pause crew and Stop crew are After the beta, since no Flow route pauses or stops a crew. */
export function FlowHead({ sub, acts }: { sub: string; acts?: boolean }) {
  return (
    <div className="cx-ph">
      <h3>
        Crew<span className="cx-cwpv">Preview</span>
      </h3>
      <span className="cx-sub" style={{ whiteSpace: "normal" }}>{sub}</span>
      {acts ? (
        <div className="cx-acts">
          <Btn className="cx-ghost">
            <Ic n="pause" sm />
            Pause crew
            <span className="cx-cwpv" title={AFTER_THE_BETA}>After the beta</span>
          </Btn>
          <Btn className="cx-ghost">
            <Ic n="stop" sm />
            Stop crew
            <span className="cx-cwpv" title={AFTER_THE_BETA}>After the beta</span>
          </Btn>
        </div>
      ) : null}
    </div>
  );
}
