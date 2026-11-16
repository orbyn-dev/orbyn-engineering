import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-detail`. */
export function EscapeDetail() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Dashboard</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Escape log</span>
          <Ic n="right" sm />
          <b>Rename the legacy returnTo helper</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Rename the legacy returnTo helper</b>
            <span className="cx-apchip">Reverted by Crew</span>
            <span className="cx-apchip">Test gap</span>
            <span className="cx-apchip cx-ok">Check added</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Change the class…</Btn><Btn className="cx-ghost">Not an escape…</Btn></span>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Same day unless a date is shown</span>
              </div>
              <div>
                <span className="cx-t">Sat 7 Nov, 12:10</span>
                <Ic n="git" sm />
                <span>
                  <b>Merged to main</b>
                  <small>Rename the legacy returnTo helper · Claude Code · Sonnet 5.5 · Medium risk · reviewed by 2 lanes, 2 vendors · 8d41b7c</small>
                </span>
              </div>
              <div>
                <span className="cx-t">15:11</span>
                <Ic n="alert" sm />
                <span><b>Main broke</b><small>Watch on main saw the login tests fail on this commit.</small></span>
              </div>
              <div>
                <span className="cx-t">15:34</span>
                <Ic n="restore" sm />
                <span><b>Reverted by Crew</b><small>The revert passed its checks on the combined state. The cause was certain.</small></span>
              </div>
              <div>
                <span className="cx-t">15:41</span>
                <Ic n="tree" sm />
                <span><b>Classified: test gap</b><small>A plain input and output that a test could have caught. Crew chose this. You can change it.</small></span>
              </div>
              <div>
                <span className="cx-t">16:05</span>
                <Ic n="check2" sm />
                <span><b>Check written and proven</b><small>It fails on 8d41b7c and passes on the revert, which is required before it is added.</small></span>
              </div>
              <div>
                <span className="cx-t">16:22</span>
                <Ic n="ok" sm />
                <span><b>Check merged</b><small>Through the merge queue, like any change. It runs on every change that touches auth/.</small></span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx">
                <h6>The check</h6>
                <div className="cx-fkv">
                  <span>Name</span>
                  <b>Legacy returnTo links still redirect</b>
                  <span>Where</span>
                  <b>tests/auth/return-to.test.ts</b>
                  <span>It asserts</span>
                  <b>A login request that carries the old returnTo parameter redirects to that path</b>
                  <span>It runs</span>
                  <b>On every change that touches auth/</b>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>Counted against</h6>
                <div className="cx-fkv">
                  <span>Engine and model</span>
                  <b>Claude Code · Sonnet 5.5</b>
                  <span>Area</span>
                  <b>auth/</b>
                  <span>This engine, last 60 days</span>
                  <b>2 escapes in 21 merges · 10%</b>
                  <span>In auth/, by this engine</span>
                  <b>2 escapes in 9 merges · too few for a rate</b>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>Where the count is used</h6>
                <p>
                  The Route card, as a tie-break between engines that suit a task equally, and Autopilot by area, where auth/ is on On after 2 reverts in 14 days.
                </p>
                <div className="cx-t13row" style={{ marginTop: "4px" }}>
                  <Btn className="cx-ghost">Open Autopilot by area</Btn>
                  <Btn className="cx-ghost">Open the revert</Btn>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
