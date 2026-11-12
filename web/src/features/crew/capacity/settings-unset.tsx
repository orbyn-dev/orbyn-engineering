import { Frame } from "../shared/Frame";
import { Btn, Ic } from "../shared/parts";

/** Crew Designs v21, frame `capacity-settings-unset`. */
export function CapacitySettingsUnset() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Review capacity
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Agents can merge faster than a person can read. Say how many changes you can review, and Crew slows the work it starts on its own before the queue gets ahead of you.
        </p>
        <div className="cx-cwbn" style={{ margin: "0" }}>
          <Ic n="info" sm />
          <span><b>No limit is set.</b>{" 3 High and Medium changes are waiting for your review now. Crew keeps starting work whatever the queue holds."}</span>
          <span className="cx-acts"><Btn className="cx-btn">Set a limit</Btn></span>
        </div>
        <div>
          <div className="cx-setrow">
            <div>
              <b>How many changes you can review</b>
              <small>
                Not set. Until you set it, Crew never pauses a start because of your review queue. Crew counts the High and Medium changes that merged and are waiting for you; Low-risk changes don’t count.
              </small>
            </div>
            <span className="cx-sv">Not set</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>When the queue passes it</b>
              <small>
                Crew doesn’t begin new Low-priority tasks it started itself. Tasks you start, High-priority work and everything already running carry on.
              </small>
            </div>
            <span className="cx-sv cx-ro">Needs a limit</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Low-risk changes</b>
              <small>Review every low-risk batch, or review a sample sized to the batch. The changes not sampled are listed in the digest.</small>
            </div>
            <span className="cx-sv">Review every batch · default</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Review coverage</b>
              <small>
                When Crew merges it records which lines an agent wrote as a git note on the merge commit. The map counts the lines no person has reviewed.
              </small>
            </div>
            <span className="cx-sv cx-ro">Recorded at merge</span>
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
