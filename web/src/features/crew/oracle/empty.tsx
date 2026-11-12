import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-empty`. */
export function OracleEmpty() {
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
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-docs<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
            <b>No independent answer keys in the last 60 days</b>
            <p>
              Documentation-only merges have no code to mutate, so no key was written. When a task that changes code has a key written by another engine, its gate result appears here.
            </p>
            <Btn className="cx-ghost" style={{ height: "28px" }}>Open Settings › Crew › Independent oracle</Btn>
          </div>
        </div>
      </div>
    </Frame>
  );
}
