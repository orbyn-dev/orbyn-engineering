import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { Ic } from "./icons";
import { FlowHead } from "./FlowHead";

/** Crew Designs, frame `flow-truncated`: Flow: a crew with more tasks than one read covers. */
export function FlowTruncated() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · 500 tasks" acts />
            <div className="cx-cwtabs"><Seg kind="segx" items={["Flow", "Review", "Map", "Board", "Lanes", "Inbox", "Report"]} on={0} /></div>
            <div style={{ display: "flex", alignItems: "center", gap: "8px", padding: "12px 20px 0", color: "var(--color-muted)", fontSize: "12px" }}><span>Crew</span><span style={{ height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)", color: "var(--color-text)", fontSize: "12px", display: "inline-flex", alignItems: "center", gap: "6px", boxSizing: "border-box", width: "360px" }}>Login redirect crew · 500 tasks, 481 finished<Ic n="down" sm style={{ marginLeft: "auto", color: "var(--color-muted)" }} /></span></div>
            <div className="cx-cwtool">
              <span className="cx-srch"><Ic n="search" sm />Find a task</span>
              <span className="cx-fil cx-on">All 500</span>
              <span className="cx-fil"><Ic n="alert" sm />Needs you 2</span>
              <span className="cx-fil">Reviewing 5</span>
              <span className="cx-fil">Failed 12</span>
              <span className="cx-fil">Passed 481</span>
              <span style={{ height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)", color: "var(--color-text)", fontSize: "12px", display: "inline-flex", alignItems: "center", gap: "6px", boxSizing: "border-box" }}>Engine: any<Ic n="down" sm style={{ marginLeft: "auto", color: "var(--color-muted)" }} /></span>
              <span className="cx-end" style={{ marginLeft: "auto" }}>6 of 500 shown</span>
            </div>
            <div className="cx-cwbn" style={{ margin: "12px 20px 0" }}><span role="note">This crew has more tasks than one read covers; the newest are shown.</span></div>
            <div style={{ padding: "8px 0 0" }}>
              <div className="cx-fta">
                <div className="cx-r cx-h" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <span>Task</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Brief</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Route</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Build</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Checks</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Code</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Security</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Finding</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Merge</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Main</span>
                  <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>You</span>
                  <span style={{ textAlign: "center" }}>Round</span>
                </div>
                <div className="cx-r cx-sel" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <div><b>Use next in the auth redirect</b><small>implement · Claude Code · owner Lead</small></div>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />3/3</span>
                  <span className="cx-fc cx-run"><i className="cx-spin" role="img" aria-label="Working" />{" ×3"}</span>
                  <span className="cx-fc cx-run"><i className="cx-spin" role="img" aria-label="Working" />{" ×1"}</span>
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-run" style={{ minWidth: "64px" }}>Round 1</span>
                </div>
                <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <div><b>Add tests for the redirect</b><small>test · Codex</small></div>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-run"><i className="cx-spin" role="img" aria-label="Working" />2/3</span>
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-run" style={{ minWidth: "64px" }}>Round 1</span>
                </div>
                <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <div><b>Review the diff for open redirects</b><small>review · OpenCode</small></div>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />2/2</span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×2"}</span>
                  <span className="cx-fc cx-need"><Ic n="alert" sm />{" ×1"}</span>
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc" style={{ minWidth: "64px" }}>Round 1</span>
                </div>
                <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <div><b>Move session helpers to auth/session.ts</b><small>implement · Claude Code</small></div>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />4/4</span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×2"}</span>
                  <span className="cx-fc cx-skip">–</span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-rev"><Ic n="eye" sm /></span>
                  <span className="cx-fc" style={{ minWidth: "64px" }}>Round 1</span>
                </div>
                <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <div><b>Rename the legacy returnTo helper</b><small>implement · Claude Code</small></div>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-fail"><Ic n="alert" sm />2/3</span>
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-pend" />
                  <span className="cx-fc cx-fail" style={{ minWidth: "64px" }}>Round 3</span>
                </div>
                <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                  <div><b>Update the login docs</b><small>docs · Codex</small></div>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />2/2</span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×2"}</span>
                  <span className="cx-fc cx-skip">–</span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                  <span className="cx-fc cx-rev"><Ic n="eye" sm /></span>
                  <span className="cx-fc" style={{ minWidth: "64px" }}>Round 1</span>
                </div>
              </div>
            </div>
            <div style={{ display: "flex", justifyContent: "center", padding: "14px 20px" }}><Btn className="cx-ghost" style={{ height: "26px", padding: "0 12px" }}>+494 more</Btn></div>
      </div>
    </Frame>
  );
}
