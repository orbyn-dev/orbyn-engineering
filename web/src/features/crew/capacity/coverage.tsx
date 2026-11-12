import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage`. */
export function CapacityCoverage() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr"><span style={{ color: "var(--color-muted)" }}>Dashboard</span><Ic n="right" sm /><b>Review coverage</b></div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Review coverage</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={0} items={["Since provenance began"]} />
            </span>
          </div>
          <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>
            As of Sat 7 Nov 2026, 17:00 · 3,660 agent-written lines from 24 merges since Mon 12 Oct 2026
          </span>
          <div className="cx-t15map">
            <div className="cx-t15tile">
              <b className="cx-a">auth/</b>
              <b className="cx-n">510 of 1,420</b>
              <small>lines no person has reviewed · 36%</small>
              <div className="cx-sp3"><span><i>Reviewed</i>910</span><span><i>In your queue</i>310</span><span><i>Not in a queue</i>200</span></div>
              <small>9 merges</small>
            </div>
            <div className="cx-t15tile">
              <b className="cx-a">docs/</b>
              <b className="cx-n">940 of 1,060</b>
              <small>lines no person has reviewed · 89%</small>
              <div className="cx-sp3"><span><i>Reviewed</i>120</span><span><i>In your queue</i>0</span><span><i>Not in a queue</i>940</span></div>
              <small>9 merges</small>
            </div>
            <div className="cx-t15tile">
              <b className="cx-a">web/</b>
              <b className="cx-n">830 of 1,180</b>
              <small>lines no person has reviewed · 70%</small>
              <div className="cx-sp3"><span><i>Reviewed</i>350</span><span><i>In your queue</i>80</span><span><i>Not in a queue</i>750</span></div>
              <small>6 merges</small>
            </div>
          </div>
          <h5 className="cx-t10h5">By area<span className="cx-t10r">Open an area for its files</span></h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.1fr) minmax(0,1.2fr) minmax(0,.9fr) minmax(0,.9fr) minmax(0,1fr) minmax(0,1.1fr)" }}>
            <div className="cx-h">Area</div>
            <div className="cx-h">Agent-written lines</div>
            <div className="cx-h">Reviewed</div>
            <div className="cx-h">In your queue</div>
            <div className="cx-h">Not in a queue</div>
            <div className="cx-h">No person has read</div>
            <div><b>auth/</b><small>9 merges</small></div>
            <div><b>1,420</b></div>
            <div><b>910</b></div>
            <div><b>310</b></div>
            <div><b>200</b></div>
            <div><b>510</b><small>36%</small></div>
            <div><b>docs/</b><small>9 merges</small></div>
            <div><b>1,060</b></div>
            <div><b>120</b></div>
            <div><b>0</b></div>
            <div><b>940</b></div>
            <div><b>940</b><small>89%</small></div>
            <div><b>web/</b><small>6 merges</small></div>
            <div><b>1,180</b></div>
            <div><b>350</b></div>
            <div><b>80</b></div>
            <div><b>750</b></div>
            <div><b>830</b><small>70%</small></div>
            <div><b>All areas</b><small>24 merges</small></div>
            <div><b>3,660</b></div>
            <div><b>1,380</b></div>
            <div><b>390</b></div>
            <div><b>1,890</b></div>
            <div><b>2,280</b><small>62%</small></div>
          </div>
          <h5 className="cx-t10h5">By engine</h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1.2fr) minmax(0,1fr) minmax(0,1.1fr)" }}>
            <div className="cx-h">Engine</div>
            <div className="cx-h">Agent-written lines</div>
            <div className="cx-h">Reviewed</div>
            <div className="cx-h">No person has read</div>
            <div><b>Claude Code · Sonnet 5.5</b></div>
            <div><b>2,300</b></div>
            <div><b>880</b></div>
            <div><b>1,420</b><small>62%</small></div>
            <div><b>Codex · GPT-5.5</b></div>
            <div><b>1,030</b></div>
            <div><b>400</b></div>
            <div><b>630</b><small>61%</small></div>
            <div><b>OpenCode · DeepSeek V4</b></div>
            <div><b>330</b></div>
            <div><b>100</b></div>
            <div><b>230</b><small>70%</small></div>
          </div>
          <div className="cx-fbx">
            <h6>How to read this</h6>
            <div className="cx-fkv">
              <span>Agent-written</span>
              <b>Lines on main today whose last change was merged by an agent, from the note Crew records at merge</b>
              <span>Reviewed</span>
              <b>You pressed Looks good, commented or changed the line, on the change that wrote it</b>
              <span>In your queue</span>
              <b>The change that wrote it is waiting for your review</b>
              <span>Not in a queue</span>
              <b>A low-risk change that wasn’t sampled, or one that merged before you turned sampling on</b>
              <span>Shares</span>
              <b>Exact counts of lines, so they show for every area. They cover only merges since Mon 12 Oct 2026: 14 earlier merges are Not measured</b>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
