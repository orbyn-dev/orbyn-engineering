import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-narrow-window`. */
export function CapacityNarrowWindow() {
  return (
    <Frame width={640}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew" />
        <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}>
          <Seg kind="segx" on={1} items={["Flow", "Review · 9", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
        </div>
        <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: "10px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span><b>New low-priority starts are paused: your review queue is over its limit.</b>{" 9 changes are waiting; your limit is 8."}</span>
          </div>
          <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }}>
            <span className="cx-t15tag">To review 9</span>
            <span className="cx-t15tag">Limit 8</span>
            <span className="cx-t15tag">High 2</span>
            <span className="cx-t15tag">Overdue 0</span>
          </div>
          <div className="cx-t14card">
            <b>Rotate the signing key</b>
            <div className="cx-t14r"><span className="cx-aprk cx-rhi">High</span></div>
            <small>Merged by Crew at 15:02 · Claude Code · Opus 5.5</small>
            <small>Checks 5 of 5 · Not verified: 2</small>
            <small>Review within 4 h · 2 h left</small>
          </div>
          <div className="cx-t14card">
            <b>Revoke the old API tokens</b>
            <div className="cx-t14r"><span className="cx-aprk cx-rhi">High</span></div>
            <small>Merged by Crew at 15:20 · Codex · GPT-5.5</small>
            <small>Checks 4 of 4 · Not verified: 1</small>
            <small>Review within 4 h · 2 h left</small>
          </div>
          <div className="cx-t14card">
            <b>Use next in the auth redirect</b>
            <div className="cx-t14r"><span className="cx-aprk cx-rmd">Medium</span></div>
            <small>Merged by Crew at 14:32 · Claude Code · Sonnet 5.5</small>
            <small>Checks 3 of 3 · Not verified: 4</small>
            <small>Review by Sun 8 Nov, 14:32 · 21 h left</small>
          </div>
          <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>6 more Medium changes follow in this list.</span>
        </div>
      </div>
    </Frame>
  );
}
