import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { Ic } from "./icons";
import { FlowHead } from "./FlowHead";

/** Crew Designs, frame `flow-narrow-window`: Flow in a narrow window (640 px). */
export function FlowNarrowWindow() {
  return (
    <Frame width={640} height={1200}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · 5 runs" />
            <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}><Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} /></div>
            <div style={{ padding: "8px 14px 0", display: "flex", flexWrap: "wrap", gap: "8px", alignItems: "center", flex: "none" }}>
              <Btn className="cx-ghost" style={{ height: "26px", padding: "0 10px" }}>All tasks</Btn>
              <b style={{ fontWeight: 500, color: "var(--color-text)", fontSize: "15px" }}>Use next in the auth redirect</b>
              <span className="cx-fbadge">Round 1</span>
              <span className="cx-fbadge">verifying</span>
              <span className="cx-end" style={{ marginLeft: "auto", display: "flex", gap: "8px" }}><Btn className="cx-ghost" style={{ height: "26px", padding: "0 10px", color: "var(--color-muted)" }} title="Available when the write route ships">Cancel</Btn><Btn className="cx-ghost" style={{ height: "26px", padding: "0 10px", color: "var(--color-muted)" }} title="Available when the write route ships">Re-run checks</Btn></span>
            </div>
            <div style={{ padding: "6px 14px 0", color: "var(--color-muted)", fontSize: "11.5px", flex: "none" }}>Cancel and Re-run checks: Available when the write route ships</div>
            <div className="cx-fsm cx-c" style={{ flex: "none" }}>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="brief" sm />
                <b>Brief</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />Written</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="gate" sm />
                <b>Route</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />Routed</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="wrench" sm />
                <b>Build</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />Done</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="check2" sm />
                <b>Checks</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />3 of 3</span></span>
              </div>
              <div className="cx-row">
                <Ic n="down" sm />
                <Ic n="eye" sm />
                <b>Code review</b>
                <span className="cx-n"><span className="cx-st cx-info"><i className="cx-spin" role="img" aria-label="Working" />Reviewing</span></span>
              </div>
              <div className="cx-sub">
                <div className="cx-it"><span style={{ minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Correctness · Codex</span><span className="cx-n">No findings</span></div>
                <div className="cx-it"><span style={{ minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Performance · OpenCode</span><span className="cx-n">Reviewing</span></div>
                <div className="cx-it"><span style={{ minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Docs · OpenCode</span><span className="cx-n">1 unconfirmed</span></div>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="shield" sm />
                <b>Security review</b>
                <span className="cx-n"><span className="cx-st cx-info"><i className="cx-spin" role="img" aria-label="Working" />Reviewing</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="scan" sm />
                <b>Finding check</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Waiting</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="branch" sm />
                <b>Merge queue</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Waiting</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="git" sm />
                <b>Main</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Waiting</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="user" sm />
                <b>You</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Waiting</span></span>
              </div>
            </div>
            <div style={{ padding: "12px 14px 16px", display: "flex", flexDirection: "column", gap: "12px", flex: "none", minWidth: "0" }}>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(260px,1fr))", gap: "12px", minWidth: "0" }}>
                <section style={{ borderRadius: "10px", background: "var(--color-sunken)", padding: "10px 12px", display: "flex", flexDirection: "column", gap: "6px", minWidth: "0" }}>
                  <b style={{ fontWeight: 500, color: "var(--color-text)", fontSize: "12px" }}>Brief</b>
                  <div style={{ display: "grid", gridTemplateColumns: "96px minmax(0,1fr)", gap: "4px 10px", fontSize: "12px", color: "var(--color-muted)" }}>
                    <span>Purpose</span>
                    <span style={{ color: "var(--color-text)", overflowWrap: "anywhere" }}>Use next when it points inside the app</span>
                    <span>Files</span>
                    <span style={{ color: "var(--color-text)", overflowWrap: "anywhere" }}>auth/redirect.ts, auth/redirect.test.ts</span>
                    <span>Budget</span>
                    <span style={{ color: "var(--color-text)", overflowWrap: "anywhere" }}>40 turns · 30 min</span>
                  </div>
                </section>
              </div>
            </div>
      </div>
    </Frame>
  );
}
