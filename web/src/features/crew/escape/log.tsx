import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-log`. */
export function EscapeLog() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr"><span style={{ color: "var(--color-muted)" }}>Dashboard</span><Ic n="right" sm /><b>Escape log</b></div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Escape log</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>As of Sat 7 Nov 2026, 17:00</span>
          <div className="cx-t10g">
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes</span>
              <span className="cx-t10b">8%</span>
              <span className="cx-t10s">of merges got past review and checks</span>
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
              <span className="cx-t10f">3 of 38 merges · last 60 days · 2 reverted, 1 found after the merge. Bars: merges per week, filled = escaped</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Why it got through</span>
              <span className="cx-t10b">2 test gaps</span>
              <span className="cx-t10s">and 1 tooling gap</span>
              <span className="cx-t10f">3 escapes · last 60 days</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Checks from escapes</span>
              <span className="cx-t10b">2 added</span>
              <span className="cx-t10s">and 1 being written</span>
              <span className="cx-t10f">3 escapes · last 60 days</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Counted against</span>
              <span className="cx-t10b">Per engine</span>
              <span className="cx-t10s">escapes of the engine’s merges</span>
              <div className="cx-t10e">
                <span><i>Claude Code</i><b>2 of 21</b></span>
                <span><i>Codex</i><b>1 of 12</b></span>
                <span><i>OpenCode</i><b>0 of 5</b></span>
              </div>
              <span className="cx-t10f">Merges per engine · last 60 days</span>
            </div>
          </div>
          <h5 className="cx-t10h5">The escapes<span className="cx-t10r">3 in last 60 days</span></h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,.9fr) minmax(0,1.5fr) minmax(0,.7fr) minmax(0,1.5fr) minmax(0,1fr)" }}>
            <div className="cx-h">When</div>
            <div className="cx-h">What escaped</div>
            <div className="cx-h">Gap</div>
            <div className="cx-h">The check it created</div>
            <div className="cx-h">Counts against</div>
            <div><b>Sat 7 Nov, 15:34</b><small>Reverted by Crew · merged 12:10</small></div>
            <div><b>Rename the legacy returnTo helper</b><small>Old returnTo links stopped redirecting</small></div>
            <div><span className="cx-apchip">Test gap</span></div>
            <div>
              <b>Legacy returnTo links still redirect</b>
              <small>tests/auth/return-to.test.ts</small>
              <span style={{ display: "inline-flex", marginTop: "4px" }}><span className="cx-apchip cx-ok">Added Sat 7 Nov, 16:22</span></span>
            </div>
            <div><b>Claude Code · Sonnet 5.5</b><small>auth/</small></div>
            <div><b>Sat 7 Nov, 15:02</b><small>Found by you · not reverted · merged 14:32</small></div>
            <div><b>Use next in the auth redirect</b><small>The legacy login route lost its rate limit</small></div>
            <div><span className="cx-apchip">Tooling gap</span></div>
            <div>
              <b>Every login route is in the rate-limit inventory</b>
              <small>A repo rule · runs in CI</small>
              <span style={{ display: "inline-flex", marginTop: "4px" }}>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Being written</span>
              </span>
            </div>
            <div><b>Claude Code · Sonnet 5.5</b><small>auth/</small></div>
            <div><b>Tue 3 Nov, 13:15</b><small>Reverted by you · merged 09:40</small></div>
            <div><b>Move session helpers to auth/session.ts</b><small>People were asked to sign in again after a session refresh</small></div>
            <div><span className="cx-apchip">Test gap</span></div>
            <div>
              <b>An expired session refreshes without a new sign-in</b>
              <small>tests/auth/session.test.ts</small>
              <span style={{ display: "inline-flex", marginTop: "4px" }}><span className="cx-apchip cx-ok">Added Tue 3 Nov, 17:05</span></span>
            </div>
            <div><b>Codex · GPT-5.5</b><small>auth/</small></div>
          </div>
          <div className="cx-fbx">
            <h6>What counts as an escape</h6>
            <div className="cx-fkv">
              <span>An escape</span>
              <b>A merge that reached main and was reverted, or turned out to have a bug and was not reverted</b>
              <span>Not an escape</span>
              <b>A revert you chose for another reason, such as a change of direction: mark it Not an escape and it isn’t counted</b>
              <span>A rate</span>
              <b>Shown from 10 merges, with the merges under it. Below that Crew shows the escapes themselves</b>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
