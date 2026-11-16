import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-error`. */
export function EscapeError() {
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
          <div className="cx-cwbn cx-red" style={{ margin: "0" }}>
            <Ic n="alert" sm />
            <span>Couldn’t read the run history on this device. Nothing was changed, and no escape was counted or removed.</span>
            <span className="cx-acts"><Btn className="cx-ghost">Copy the error</Btn><Btn className="cx-btn">Try again</Btn></span>
          </div>
          <div className="cx-t10g">
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes</span>
              <span className="cx-t10b">—</span>
              <span className="cx-t10f">Not shown while the history can’t be read</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Why it got through</span>
              <span className="cx-t10b">—</span>
              <span className="cx-t10f">Not shown while the history can’t be read</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Checks from escapes</span>
              <span className="cx-t10b">—</span>
              <span className="cx-t10f">Not shown while the history can’t be read</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Counted against</span>
              <span className="cx-t10b">—</span>
              <span className="cx-t10f">Not shown while the history can’t be read</span>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
