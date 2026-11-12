import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage-error`. */
export function CapacityCoverageError() {
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
          <div className="cx-cwbn cx-red" style={{ margin: "0" }}>
            <Ic n="alert" sm />
            <span>Couldn’t read the provenance notes on this device. Nothing was changed, and no line was counted or removed.</span>
            <span className="cx-acts"><Btn className="cx-ghost">Copy the error</Btn><Btn className="cx-btn">Try again</Btn></span>
          </div>
          <div className="cx-t15map">
            <div className="cx-t15tile"><b className="cx-a">auth/</b><b className="cx-n">—</b><small>Not shown while the notes can’t be read</small></div>
            <div className="cx-t15tile"><b className="cx-a">docs/</b><b className="cx-n">—</b><small>Not shown while the notes can’t be read</small></div>
            <div className="cx-t15tile"><b className="cx-a">web/</b><b className="cx-n">—</b><small>Not shown while the notes can’t be read</small></div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
