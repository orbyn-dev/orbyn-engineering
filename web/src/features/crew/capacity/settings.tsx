import { Frame } from "../shared/Frame";
import { Ic } from "../shared/parts";

/** Crew Designs v21, frame `capacity-settings`. */
export function CapacitySettings() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Review capacity
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Agents can merge faster than a person can read. Say how many changes you can review, and Crew slows the work it starts on its own before the queue gets ahead of you. It never stops a merge, and nothing here changes what you can review.
        </p>
        <div>
          <div className="cx-setrow">
            <div>
              <b>How many changes you can review</b>
              <small>
                Crew counts the High and Medium changes that merged and are waiting for you. Low-risk changes are batched, have no deadline and don’t count. Set the number you are comfortable having waiting.
              </small>
            </div>
            <span className="cx-sv">8 changes · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>When the queue passes it</b>
              <small>
                Crew doesn’t begin new Low-priority tasks it started itself: background crews, cleanups and checks from escapes. Tasks you start, High-priority work and everything already running carry on. Paused starts begin by themselves when the queue is back at the limit or under it.
              </small>
            </div>
            <span className="cx-sv cx-ro">Pause new Low-priority starts</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Low-risk changes</b>
              <small>
                Review every low-risk batch (the default), or review a sample. A sample is sized to the batch (see the plan below). The changes not sampled are listed in the digest, and you can add any of them to your queue.
              </small>
            </div>
            <span className="cx-sv">Review a sample · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Review coverage</b>
              <small>
                When Crew merges it records which lines an agent wrote as a git note on the merge commit, in a form tools that use Agent Trace can read. The map counts the lines no person has reviewed.
              </small>
            </div>
            <span className="cx-sv cx-ro">Recorded at merge</span>
          </div>
        </div>
        <div className="cx-fbx">
          <h6>The sampling plan</h6>
          <p>One in four, rounded up, of the low-risk merges in a batch. Which ones is decided after they merge and is not given to the worker.</p>
          <div className="cx-t15plan">
            <div className="cx-h">Changes in the batch</div>
            <div className="cx-h">Sampled for review</div>
            <div className="cx-h">Not sampled</div>
            <div>1 to 4</div>
            <div>1</div>
            <div>0 to 3</div>
            <div>5 to 8</div>
            <div>2</div>
            <div>3 to 6</div>
            <div>9 to 12</div>
            <div>3</div>
            <div>6 to 9</div>
            <div>13 to 16</div>
            <div>4</div>
            <div>9 to 12</div>
          </div>
        </div>
        <div className="cx-fbx">
          <h6>What the limit does and doesn’t do</h6>
          <div className="cx-fkv">
            <span>Does</span>
            <b>Pauses new Low-priority starts while the queue is over the limit, and says so on the Crew page</b>
            <span>Doesn’t</span>
            <b>Stop running work, stop a merge, or pause a task you started or any High-priority task</b>
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
