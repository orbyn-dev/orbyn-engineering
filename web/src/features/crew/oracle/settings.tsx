import { Frame } from "../shared/Frame";
import { Ic } from "../shared/parts";

/** Crew Designs v22, frame `oracle-settings`. */
export function OracleSettings() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Independent oracle
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          A check written by the engine that built the change can pass because both made the same mistake. An independent key is written by a different engine, and a mutation gate shows whether its checks can catch a break at all.
        </p>
        <div>
          <div className="cx-setrow">
            <div>
              <b>Who writes the answer key</b>
              <small>
                An engine other than the one that builds the task, on another vendor when one is signed in. The author never sees the builder’s code, notes, messages or worktree. With no other engine, Crew asks you.
              </small>
            </div>
            <span className="cx-sv">Another engine · Auto</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>What the author is given</b>
              <small>
                The brief and the contract facts. Not given to the author: the diff, the builder’s notes and messages, its worktree. The builder is never given the key, as in every Crew task.
              </small>
            </div>
            <span className="cx-sv cx-ro">Brief and contract facts</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Mutation gate</b>
              <small>
                Crew makes small deliberate breaks in the changed code and runs the key on each. A key that kills no mutants isn’t an oracle, so it fails the gate and the task doesn’t pass on it.
              </small>
            </div>
            <span className="cx-sv">On · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div><b>If the gate fails</b><small>The key goes back to its author to be strengthened. After the last round Crew asks you.</small></div>
            <span className="cx-sv">2 rounds · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Which tasks</b>
              <small>Tasks that change code and have a check to write. Documentation-only tasks have no mutants and are skipped.</small>
            </div>
            <span className="cx-sv">Tasks that change code · Proposed</span>
          </div>
        </div>
        <div className="cx-fbx">
          <h6>What Crew doesn’t claim</h6>
          <p>
            Crew doesn’t claim that an independent key finds more bugs. The benefit is not measured yet, so the Dashboard counts how keys were used and how many escapes followed, and says when the sample is too small to compare.
          </p>
        </div>
        <div className="cx-t13row">
          <span className="cx-t13mk">Sample data</span>
          <span className="cx-t13mk">Proposed: Engineering sets the real values</span>
        </div>
        <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
          <Ic n="info" sm />
          <span>Workers run as you, with your machine’s permissions.</span>
        </p>
      </div>
    </Frame>
  );
}
