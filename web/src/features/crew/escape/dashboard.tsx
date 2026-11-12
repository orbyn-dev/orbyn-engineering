import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-dashboard`. */
export function EscapeDashboard() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={4} items={["Review queue · 7", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Dashboard</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <div className="cx-t10g cx-t10two" style={{ gridTemplateColumns: "minmax(0,1.6fr) minmax(0,1fr)" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes by week</span>
              <span className="cx-t10b">3 in 60 days</span>
              <span className="cx-t10s">reverted or found after the merge</span>
              <div className="cx-t10bars" role="img" aria-label="Merges per week">
                <div title="week 1: 3 merges, 0 highlighted"><i style={{ height: "43%" }}><b style={{ height: "0%" }} /></i><em>3</em></div>
                <div title="week 2: 5 merges, 0 highlighted"><i style={{ height: "71%" }}><b style={{ height: "0%" }} /></i><em>5</em></div>
                <div title="week 3: 4 merges, 1 highlighted"><i style={{ height: "57%" }}><b style={{ height: "25%" }} /></i><em>4</em></div>
                <div title="week 4: 6 merges, 0 highlighted"><i style={{ height: "86%" }}><b style={{ height: "0%" }} /></i><em>6</em></div>
                <div title="week 5: 5 merges, 0 highlighted"><i style={{ height: "71%" }}><b style={{ height: "0%" }} /></i><em>5</em></div>
                <div title="week 6: 4 merges, 0 highlighted"><i style={{ height: "57%" }}><b style={{ height: "0%" }} /></i><em>4</em></div>
                <div title="week 7: 7 merges, 2 highlighted"><i style={{ height: "100%" }}><b style={{ height: "29%" }} /></i><em>7</em></div>
                <div title="week 8: 4 merges, 0 highlighted"><i style={{ height: "57%" }}><b style={{ height: "0%" }} /></i><em>4</em></div>
              </div>
              <span className="cx-t10f">
                Each bar is a week: its height and the figure under it are the merges that week; the filled part is the escapes (week 3: 1 of 4, week 7: 2 of 7). Weeks with fewer than 10 merges are not turned into percentages.
              </span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Escape rate</span>
              <span className="cx-t10b">8%</span>
              <span className="cx-t10s">3 of 38 merges</span>
              <Btn className="cx-ghost" style={{ height: "26px", alignSelf: "flex-start", marginTop: "6px" }}>Open the escape log</Btn>
              <span className="cx-t10f">last 60 days · 2 reverted, 1 not reverted · 2 checks added, 1 being written</span>
            </div>
          </div>
          <h5 className="cx-t10h5">By engine<span className="cx-t10r">Open the escape log</span></h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1fr) minmax(0,1.4fr) minmax(0,1fr)" }}>
            <div className="cx-h">Engine</div>
            <div className="cx-h">Escaped</div>
            <div className="cx-h">How</div>
            <div className="cx-h">Checks from them</div>
            <div><b>Claude Code · Sonnet 5.5</b></div>
            <div><b>2 of 21</b><small>escaped · 10%</small></div>
            <div><b>1 reverted · 1 not reverted</b><small>1 test gap, 1 tooling gap</small></div>
            <div><b>1 added</b><small>1 being written</small></div>
            <div><b>Codex · GPT-5.5</b></div>
            <div><b>1 of 12</b><small>escaped · 8%</small></div>
            <div><b>1 reverted · 0 not reverted</b><small>1 test gap</small></div>
            <div><b>1 added</b><small>0 being written</small></div>
            <div><b>OpenCode · DeepSeek V4</b></div>
            <div><b>0 of 5</b><small>too few for a rate</small></div>
            <div><b>None</b><small>Not measured</small></div>
            <div><b>None</b><small>Not measured</small></div>
          </div>
          <h5 className="cx-t10h5">By area<span className="cx-t10r">Open the escape log</span></h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1fr) minmax(0,1.4fr) minmax(0,1fr)" }}>
            <div className="cx-h">Area</div>
            <div className="cx-h">Escaped</div>
            <div className="cx-h">How</div>
            <div className="cx-h">Checks from them</div>
            <div><b>auth/</b></div>
            <div><b>3 of 15</b><small>escaped · 20%</small></div>
            <div><b>2 reverted · 1 not reverted</b><small>2 test gaps, 1 tooling gap</small></div>
            <div><b>2 added</b><small>1 being written</small></div>
            <div><b>docs/</b></div>
            <div><b>0 of 14</b><small>escaped · 0%</small></div>
            <div><b>None</b><small>14 merges</small></div>
            <div><b>None</b><small>nothing to add</small></div>
            <div><b>web/</b></div>
            <div><b>0 of 9</b><small>too few for a rate</small></div>
            <div><b>None</b><small>Not measured</small></div>
            <div><b>None</b><small>Not measured</small></div>
          </div>
          <div className="cx-fbx">
            <h6>Why some rows have no percentage</h6>
            <p>
              A rate on a handful of merges mostly says luck. Crew shows the escapes themselves until an engine or an area has 10 merges, over the period, for that repo on this device.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
