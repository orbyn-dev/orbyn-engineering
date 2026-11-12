import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-narrow-window`. */
export function EscapeNarrowWindow() {
  return (
    <Frame width={640}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew" />
        <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}>
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
        </div>
        <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: "10px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-t10hd"><b>Escape log</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }}>
            <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
            <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
          </div>
          <div className="cx-t10g cx-t10two">
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes</span>
              <span className="cx-t10b">8%</span>
              <span className="cx-t10s">got past review and checks</span>
              <span className="cx-t10f">3 of 38 merges · 60 days</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Why it got through</span>
              <span className="cx-t10b">2 test gaps</span>
              <span className="cx-t10s">and 1 tooling gap</span>
              <span className="cx-t10f">3 escapes</span>
            </div>
          </div>
          <div className="cx-t14card">
            <b>Rename the legacy returnTo helper</b>
            <div className="cx-t14r"><span className="cx-apchip">Reverted by Crew</span><span className="cx-apchip">Test gap</span></div>
            <small>Sat 7 Nov, 15:34 · merged 12:10</small>
            <small>Check: Added Sat 7 Nov, 16:22</small>
            <small>Counts against Claude Code · Sonnet 5.5 · auth/</small>
          </div>
          <div className="cx-t14card">
            <b>Use next in the auth redirect</b>
            <div className="cx-t14r"><span className="cx-apchip">Found by you</span><span className="cx-apchip">Tooling gap</span></div>
            <small>Sat 7 Nov, 15:02 · merged 14:32</small>
            <small>Check: Being written</small>
            <small>Counts against Claude Code · Sonnet 5.5 · auth/</small>
          </div>
          <div className="cx-t14card">
            <b>Move session helpers to auth/session.ts</b>
            <div className="cx-t14r"><span className="cx-apchip">Reverted by you</span><span className="cx-apchip">Test gap</span></div>
            <small>Tue 3 Nov, 13:15 · merged 09:40</small>
            <small>Check: Added Tue 3 Nov, 17:05</small>
            <small>Counts against Codex · GPT-5.5 · auth/</small>
          </div>
        </div>
      </div>
    </Frame>
  );
}
