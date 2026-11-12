import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-possible`. */
export function EscapePossible() {
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
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span><b>1 later fix may be an escape.</b>{" It isn’t counted unless you confirm it."}</span>
            <span className="cx-acts"><Btn className="cx-ghost">Open the fix</Btn></span>
          </div>
          <div className="cx-fbx">
            <div className="cx-t14r">
              <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Add a keyboard shortcut to the web client</b>
              <span className="cx-apchip cx-dim">Possible escape</span>
            </div>
            <p>Merged Sat 31 Oct, 10:20 · Codex · GPT-5.5 · web/shortcuts.ts · crew/web-shortcut</p>
            <div className="cx-fkv">
              <span>Why this one</span>
              <b>A later commit by you, “Fix the shortcut opening twice” (Mon 2 Nov, 11:40), changes the same lines and names this merge</b>
              <span>If you confirm</span>
              <b>It joins the log, Crew classifies it, writes a check, and counts it against Codex · GPT-5.5 · web/</b>
              <span>If you don’t</span>
              <b>Nothing is counted and this isn’t offered again for that commit</b>
            </div>
            <div className="cx-t14r" style={{ marginTop: "6px" }}>
              <Btn className="cx-btn">Mark as an escape</Btn>
              <Btn className="cx-ghost">Not an escape</Btn>
            </div>
          </div>
          <h5 className="cx-t10h5">Counted escapes<span className="cx-t10r">3 in last 60 days · 38 merges</span></h5>
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
        </div>
      </div>
    </Frame>
  );
}
