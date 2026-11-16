import { Frame } from "../shared/Frame";
import { Ic } from "../shared/parts";

/** Crew Designs v20, frame `escape-settings`. */
export function EscapeSettings() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>Crew › Escapes<span className="cx-cwpv">Preview</span></h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Every revert, and every bug found after a merge that you don’t revert, can become a permanent check. Crew classifies it, writes the check, and counts the escape against the engine, model and area that produced it. The check is a normal task: it goes through review, the merge queue and your Autopilot mode like any change.
        </p>
        <div>
          <div className="cx-setrow">
            <div>
              <b>Create the check</b>
              <small>
                When a merge is reverted or marked as an escape, Crew proposes a check. This sets whether it opens the task itself. “Open it by itself” is a choice here and is off until you turn it on, under Full as well.
              </small>
            </div>
            <span className="cx-sv">Ask me each time · default</span>
          </div>
          <div className="cx-setrow">
            <div><b>Kind of gap</b><small>Crew suggests a test gap or a tooling gap. You can change it on the escape.</small></div>
            <span className="cx-sv cx-ro">Suggested by Crew</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Proof before a check is added</b>
              <small>A check is added when it fails on the bad commit and passes without it. If it doesn’t, Crew asks you.</small>
            </div>
            <span className="cx-sv cx-ro">Required</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Counted against</b>
              <small>Each escape counts against the engine, model and area that wrote the merge. Autopilot by area and the Route card read the counts.</small>
            </div>
            <span className="cx-sv cx-ro">Engine · model · area</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Router tie-break</b>
              <small>Escape counts decide between equal engines when both have at least 10 merges in the area and one rate is at least double the other.</small>
            </div>
            <span className="cx-sv cx-ro">10 merges · 2× · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div><b>Window</b><small>How far back the log and the counts go.</small></div>
            <span className="cx-sv cx-ro">60 days · Proposed</span>
          </div>
        </div>
        <div className="cx-fbx">
          <h6>What counts</h6>
          <div className="cx-fkv">
            <span>A revert</span>
            <b>A merge that Crew or you reverted, unless you mark it Not an escape</b>
            <span>An escape not reverted</span>
            <b>A bug that reached main and stays: found by a person, a later check or a report</b>
            <span>Cost</span>
            <b>Not part of the record</b>
          </div>
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
