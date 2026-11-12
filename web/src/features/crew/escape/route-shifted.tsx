import { Frame } from "../shared/Frame";
import { Btn, Ic } from "../shared/parts";

/** Crew Designs v20, frame `escape-route-shifted`. */
export function EscapeRouteShifted() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Board</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Add pagination to the list endpoint</span>
          <Ic n="right" sm />
          <b>Route</b>
        </div>
        <div style={{ padding: "12px 20px 0" }}>
          <div className="cx-frh"><b>Why Codex · GPT-5.5 was picked</b><span className="cx-cwpv">Preview</span></div>
        </div>
        <div className="cx-rtc">
          <div className="cx-col">
            <div className="cx-fbx" style={{ boxShadow: "inset 0 0 0 1.5px var(--color-muted)" }}>
              <h6>{"Chosen: Codex · GPT-5.5 · Medium "}<span className="cx-cwpv">Preview</span></h6>
              <p style={{ color: "var(--color-textSoft)" }}>
                Both engines suit the task equally, and the escape counts decided between them: in api/ Claude Code had 4 escapes in 12 merges and Codex had 1 in 14, both over 10 merges, and one rate is at least double the other (Sample data · last 60 days).
              </p>
            </div>
            <div className="cx-fbx">
              <h6>The task, as Crew read it</h6>
              <div className="cx-fkv">
                <span>Kind</span>
                <b>implement</b>
                <span>Touches</span>
                <b>api/list.ts</b>
                <span>Area</span>
                <b>api/</b>
                <span>Needs</span>
                <b>write access, a worktree, a lease</b>
              </div>
            </div>
          </div>
          <div className="cx-col">
            <h5 style={{ margin: "0", fontSize: "12px", fontWeight: 500, color: "var(--color-text)" }}>Escape counts in api/ · last 60 days</h5>
            <div className="cx-rej cx-chosen"><Ic n="ok" sm /><div><b>Codex · GPT-5.5</b><small>1 escape in 14 merges · 7%</small></div></div>
            <div className="cx-rej"><Ic n="info" sm /><div><b>Claude Code · Sonnet 5.5</b><small>4 escapes in 12 merges · 33%</small></div></div>
            <div className="cx-fbx">
              <h6>What this changes</h6>
              <p>
                It decides between equal engines. It doesn’t block Claude Code, doesn’t change the risk tier, and you can pin either engine. The counts are open in the escape log.
              </p>
            </div>
          </div>
        </div>
        <div style={{ display: "flex", gap: "8px", padding: "0 20px 8px" }}>
          <Btn className="cx-ghost">Open the escape log</Btn>
          <Btn className="cx-ghost">Open the rule</Btn>
        </div>
        <div style={{ padding: "0 20px 14px" }}>
          <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.5 }}>
            Rules run before any engine is scored. The escape counts are Crew’s own records on this device, and the decision is in the ledger.
          </p>
          <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
            <Ic n="info" sm />
            <span>Workers run as you, with your machine’s permissions.</span>
          </p>
        </div>
      </div>
    </Frame>
  );
}
