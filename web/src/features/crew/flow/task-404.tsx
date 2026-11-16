import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { Ic } from "./icons";
import { FlowHead } from "./FlowHead";

/** Crew Designs, frame `flow-task-404`: Flow: a task that isn’t in Flow. */
export function FlowTask404() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · 31 tasks" acts />
            <div className="cx-cwtabs"><Seg kind="segx" items={["Flow", "Review", "Map", "Board", "Lanes", "Inbox", "Report"]} on={0} /></div>
            <div style={{ display: "flex", alignItems: "center", gap: "8px", padding: "12px 20px 0", color: "var(--color-muted)", fontSize: "12px" }}><span>Crew</span><span style={{ height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)", color: "var(--color-text)", fontSize: "12px", display: "inline-flex", alignItems: "center", gap: "6px", boxSizing: "border-box", width: "360px" }}>Login redirect crew · 31 tasks, 7 finished<Ic n="down" sm style={{ marginLeft: "auto", color: "var(--color-muted)" }} /></span></div>
            <div style={{ padding: "12px 20px 0" }}><Btn className="cx-ghost" style={{ height: "26px", padding: "0 10px" }}>All tasks</Btn></div>
            <div style={{ padding: "16px 20px", color: "var(--color-muted)" }}>
              <p style={{ margin: "0", color: "var(--color-text)", fontSize: "14px", fontWeight: 500 }}>This task isn’t in Flow</p>
            </div>
      </div>
    </Frame>
  );
}
