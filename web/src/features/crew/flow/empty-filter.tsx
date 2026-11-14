import { Frame } from "../shared/Frame";
import { Seg } from "../shared/parts";
import { Ic } from "./icons";
import { FlowHead } from "./FlowHead";

/** Crew Designs, frame `flow-empty-filter`: Flow: no task matches the filter. */
export function FlowEmptyFilter() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · 31 tasks" acts />
            <div className="cx-cwtabs"><Seg kind="segx" items={["Flow", "Review", "Map", "Board", "Lanes", "Inbox", "Report"]} on={0} /></div>
            <div style={{ display: "flex", alignItems: "center", gap: "8px", padding: "12px 20px 0", color: "var(--color-muted)", fontSize: "12px" }}><span>Crew</span><span style={{ height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)", color: "var(--color-text)", fontSize: "12px", display: "inline-flex", alignItems: "center", gap: "6px", boxSizing: "border-box", width: "360px" }}>Login redirect crew · 31 tasks, 7 finished<Ic n="down" sm style={{ marginLeft: "auto", color: "var(--color-muted)" }} /></span></div>
            <div className="cx-cwtool">
              <span className="cx-srch"><Ic n="search" sm /><span style={{ color: "var(--color-text)" }}>rate limit</span></span>
              <span className="cx-fil">All 31</span>
              <span className="cx-fil"><Ic n="alert" sm />Needs you 2</span>
              <span className="cx-fil">Reviewing 5</span>
              <span className="cx-fil">Failed 2</span>
              <span className="cx-fil cx-on">Passed 5</span>
              <span style={{ height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)", color: "var(--color-text)", fontSize: "12px", display: "inline-flex", alignItems: "center", gap: "6px", boxSizing: "border-box" }}>Engine: any<Ic n="down" sm style={{ marginLeft: "auto", color: "var(--color-muted)" }} /></span>
              <span className="cx-end" style={{ marginLeft: "auto" }}>0 of 0 shown</span>
            </div>
            <div style={{ padding: "16px 20px", color: "var(--color-muted)" }}>
              <p style={{ margin: "0", color: "var(--color-text)", fontSize: "14px", fontWeight: 500 }}>No tasks match</p>
            </div>
      </div>
    </Frame>
  );
}
