import { Frame } from "../shared/Frame";
import { Seg } from "../shared/parts";
import { FlowHead } from "./FlowHead";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-gates`: Flow: who decided each gate. */
export function FlowGates() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · Task: Use next in the auth redirect · 5 runs" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m<span className="cx-cwpv" title="Planned: no Flow route reports usage yet">Planned</span></span>
            </div>
            <div style={{ padding: "20px 28px", display: "flex", flexDirection: "column", gap: "16px" }}>
              <div style={{ display: "flex", gap: "16px", flexWrap: "wrap" }}>
                <div className="cx-fbx" style={{ width: "340px", gap: "8px" }}>
                  <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                    <span className="cx-fgt cx-done" style={{ position: "static" }}><Ic n="cpu" sm /></span>
                    <h6>Checks → Code review: passed</h6>
                  </div>
                  <p style={{ color: "var(--color-textSoft)" }}>3 of 3 checks ran on commit a41c9e2 and exited 0.</p>
                  <div className="cx-fkv"><span>Decided by</span><b>Code. No model decided this gate.</b></div>
                </div>
                <div className="cx-fbx cx-red" style={{ width: "340px", gap: "8px" }}>
                  <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                    <span className="cx-fgt cx-done" style={{ position: "static" }}><Ic n="bug" sm /></span>
                    <h6>Finding check → Build: blocks</h6>
                  </div>
                  <p style={{ color: "var(--color-textSoft)" }}>A finding was reproduced with a test that fails (exit 1) on a41c9e2.</p>
                  <div className="cx-fkv"><span>Decided by</span><b>A confirmed finding. Nothing blocks without one.</b></div>
                </div>
                <div className="cx-fbx" style={{ width: "340px", gap: "8px" }}>
                  <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                    <span className="cx-fgt cx-done" style={{ position: "static" }}><Ic n="user" sm /></span>
                    <h6>Finding check → Merge queue: you approved</h6>
                  </div>
                  <p style={{ color: "var(--color-textSoft)" }}>With Autopilot On (a High risk change) or Off (any change), it waited for you here, and you approved it at 14:32. Under Full nothing waits here.</p>
                  <div className="cx-fkv"><span>Decided by</span><b>You. Under Full, the queue decides by code.</b></div>
                </div>
              </div>
              <div className="cx-fbx" style={{ maxWidth: "1040px" }}>
                <h6>Gate states</h6>
                <div style={{ display: "flex", gap: "22px", flexWrap: "wrap", color: "var(--color-textSoft)", paddingTop: "6px" }}>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}><i className="cx-fgt cx-done" style={{ position: "static" }}><Ic n="cpu" sm /></i>Decided</span>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}><i className="cx-fgt cx-live" style={{ position: "static" }}><Ic n="cpu" sm /></i>Deciding now</span>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}><i className="cx-fgt cx-pend" style={{ position: "static" }}><Ic n="cpu" sm /></i>Not reached</span>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}><i className="cx-fgt cx-block" style={{ position: "static" }}><Ic n="cpu" sm /></i>Blocked: failed check or confirmed finding</span>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}><i className="cx-fgt cx-need" style={{ position: "static" }}><Ic n="cpu" sm /></i>Needs you</span>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}><i className="cx-fgt cx-skip" style={{ position: "static" }}><Ic n="cpu" sm /></i>Skipped</span>
                </div>
              </div>
              <div className="cx-fbx" style={{ maxWidth: "1040px" }}>
                <h6>What a model can and can’t do at a gate</h6>
                <p>A model’s review is shown as a finding. It never passes or fails a gate. A finding blocks after a separate check confirms it: a code repro, or an independent second run. Unconfirmed findings stay visible, marked “unconfirmed”.</p>
              </div>
            </div>
      </div>
    </Frame>
  );
}
