import { Ic } from "../shared/parts";
import { CoverageShell } from "./coverage-shell";
import { Planned, WHY } from "./Planned";

/** Crew Designs v77, frame `capacity-coverage`. */
export function CapacityCoverage() {
  return (
    <CoverageShell project="orbyn-engineering">
      <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>
        As of Sat 7 Nov 2026, 17:00 · 3,660 agent-written lines from 24 merges since Mon 12 Oct 2026
      </span>
      <div className="cx-cwbn" style={{ margin: "0" }}>
        <Ic n="info" sm />
        <span>
          <b>14 of 38 merges aren’t in the map.</b>
          {" They are from before Mon 12 Oct 2026, had no stored hunks or had no note written. Their lines are Not measured, not counted as read or unread."}
        </span>
      </div>
      <div className="cx-t15map">
        <div className="cx-t15tile">
          <b className="cx-a">web/auth</b>
          <b className="cx-n">510 of 1,420</b>
          <small>lines no person has reviewed · 36%</small>
          <div className="cx-sp3"><span><i>Reviewed</i>910</span><span><i>In your queue</i>310</span><span><i>Not in a queue</i>200</span></div>
          <small>9 merges</small>
        </div>
        <div className="cx-t15tile">
          <b className="cx-a">docs/guides</b>
          <b className="cx-n">940 of 1,060</b>
          <small>lines no person has reviewed · 89%</small>
          <div className="cx-sp3"><span><i>Reviewed</i>120</span><span><i>In your queue</i>40</span><span><i>Not in a queue</i>900</span></div>
          <small>9 merges</small>
        </div>
        <div className="cx-t15tile">
          <b className="cx-a">web/app</b>
          <b className="cx-n">830 of 1,180</b>
          <small>lines no person has reviewed · 70%</small>
          <div className="cx-sp3"><span><i>Reviewed</i>350</span><span><i>In your queue</i>80</span><span><i>Not in a queue</i>750</span></div>
          <small>6 merges</small>
        </div>
      </div>
      <h5 className="cx-t10h5">
        By area
        <span className="cx-t10r">
          Open an area for its files
          <Planned why={WHY.files} />
        </span>
      </h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.1fr) minmax(0,1.2fr) minmax(0,.9fr) minmax(0,.9fr) minmax(0,1fr) minmax(0,1.1fr)" }}>
        <div className="cx-h">Area</div>
        <div className="cx-h">Agent-written lines</div>
        <div className="cx-h">Reviewed</div>
        <div className="cx-h">In your queue</div>
        <div className="cx-h">Not in a queue</div>
        <div className="cx-h">No person has read</div>
        <div><b>web/auth</b><small>9 merges</small></div>
        <div><b>1,420</b></div>
        <div><b>910</b></div>
        <div><b>310</b></div>
        <div><b>200</b></div>
        <div><b>510</b><small>36%</small></div>
        <div><b>docs/guides</b><small>9 merges</small></div>
        <div><b>1,060</b></div>
        <div><b>120</b></div>
        <div><b>40</b></div>
        <div><b>900</b></div>
        <div><b>940</b><small>89%</small></div>
        <div><b>web/app</b><small>6 merges</small></div>
        <div><b>1,180</b></div>
        <div><b>350</b></div>
        <div><b>80</b></div>
        <div><b>750</b></div>
        <div><b>830</b><small>70%</small></div>
        <div><b>All areas</b><small>24 merges</small></div>
        <div><b>3,660</b></div>
        <div><b>1,380</b></div>
        <div><b>430</b></div>
        <div><b>1,850</b></div>
        <div><b>2,280</b><small>62%</small></div>
      </div>
      <h5 className="cx-t10h5">By engine</h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1.2fr) minmax(0,1fr) minmax(0,1.1fr)" }}>
        <div className="cx-h">Engine</div>
        <div className="cx-h">Agent-written lines</div>
        <div className="cx-h">Reviewed</div>
        <div className="cx-h">No person has read</div>
        <div><b>claude · Sonnet 5.5</b></div>
        <div><b>2,300</b></div>
        <div><b>880</b></div>
        <div><b>1,420</b><small>62%</small></div>
        <div><b>codex · GPT-5.5</b></div>
        <div><b>1,030</b></div>
        <div><b>400</b></div>
        <div><b>630</b><small>61%</small></div>
        <div><b>opencode · DeepSeek V4</b></div>
        <div><b>330</b></div>
        <div><b>100</b></div>
        <div><b>230</b><small>70%</small></div>
      </div>
      <div className="cx-fbx">
        <h6>How to read this</h6>
        <div className="cx-fkv">
          <span>Agent-written</span>
          <b>Lines the merges wrote, counted per merge from the note Crew records at merge. Not the lines still on main today</b>
          <span>Reviewed</span>
          <b>The merge that wrote the lines was marked Looks good or commented. Review is counted per merge, never per line</b>
          <span>In your queue</span>
          <b>The change that wrote it is waiting for your review</b>
          <span>Not in a queue</span>
          <b>Lines of a change that is neither reviewed nor waiting in your queue</b>
          <span>Shares</span>
          <b>Exact counts of lines. They cover only merges since Mon 12 Oct 2026: 14 merges are Not measured</b>
        </div>
      </div>
    </CoverageShell>
  );
}
