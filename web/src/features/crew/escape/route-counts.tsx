import { Frame } from "../shared/Frame";
import { Btn, Ic } from "../shared/parts";

/** Crew Designs v20, frame `escape-route-counts`. */
export function EscapeRouteCounts() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Board</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Add a keyboard shortcut to the web client</span>
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
                It is signed in, the rules allow it, and it can be steered live. It passed this kind of task before in this repo (Sample data: 9 of 11 passed · last 30 days). Escape counts did not decide this: see the right column.
              </p>
            </div>
            <div className="cx-fbx">
              <h6>The task, as Crew read it</h6>
              <div className="cx-fkv">
                <span>Kind</span>
                <b>implement</b>
                <span>Touches</span>
                <b>web/shortcuts.ts</b>
                <span>Area</span>
                <b>web/</b>
                <span>Needs</span>
                <b>write access, a worktree, a lease</b>
              </div>
            </div>
          </div>
          <div className="cx-col">
            <h5 style={{ margin: "0", fontSize: "12px", fontWeight: 500, color: "var(--color-text)" }}>Escape counts in web/ · last 60 days</h5>
            <div className="cx-rej cx-chosen"><Ic n="ok" sm /><div><b>Codex · GPT-5.5</b><small>0 escapes in 4 merges · too few for a rate</small></div></div>
            <div className="cx-rej"><Ic n="info" sm /><div><b>Claude Code · Sonnet 5.5</b><small>0 escapes in 5 merges · too few for a rate</small></div></div>
            <div className="cx-fbx">
              <h6>How the router uses them</h6>
              <p>
                Escape counts break a tie between engines that suit a task equally. They count when both engines have at least 10 merges in the area and one rate is at least double the other (Proposed). Here neither engine has 10, so the counts don’t change the choice.
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
