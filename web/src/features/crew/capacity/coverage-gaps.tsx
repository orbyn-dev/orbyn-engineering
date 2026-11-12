import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage-gaps`. */
export function CapacityCoverageGaps() {
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
          <span style={{ color: "var(--color-muted)" }}>Review coverage</span>
          <Ic n="right" sm />
          <b>Not measured</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>14 of 38 merges aren’t in the map.</b>
              {" 12 are from before Mon 12 Oct 2026 and 2 had no note written. Their lines are Not measured, not counted as read or unread."}
            </span>
            <span className="cx-acts"><Btn className="cx-btn">Try the 2 again</Btn></span>
          </div>
          <div className="cx-t15list">
            <div className="cx-hd"><b>Merges with no provenance note</b><span>2 merges</span></div>
            <div className="cx-t15row cx-t15nc">
              <div className="cx-m"><b>Add a retry to the calendar feed</b><small>Merged Sat 31 Oct, 10:20 · Codex · GPT-5.5 · web/</small></div>
              <span className="cx-t15tag">The notes ref couldn’t be updated</span>
            </div>
            <div className="cx-t15row cx-t15nc">
              <div className="cx-m"><b>Update the login docs for returnTo</b><small>Merged Wed 4 Nov, 16:10 · Claude Code · Sonnet 5.5 · docs/</small></div>
              <span className="cx-t15tag">The notes ref couldn’t be updated</span>
            </div>
          </div>
          <div className="cx-t15list">
            <div className="cx-hd"><b>Before provenance began</b><span>12 merges · before Mon 12 Oct 2026</span></div>
            <div className="cx-t15row cx-t15nc">
              <div className="cx-m">
                <b>Not measured</b>
                <small>Crew started recording which lines an agent wrote on Mon 12 Oct 2026. Earlier merges have no record, and Crew doesn’t guess.</small>
              </div>
              <span className="cx-t15tag">Not measured</span>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
