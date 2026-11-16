import { Frame } from "../shared/Frame";
import { PanelShell } from "../shared/PanelShell";

/** Crew Designs v20, frame `escape-narrow-panel`. */
export function EscapeNarrowPanel() {
  return (
    <Frame panel>
      <PanelShell on="Review">
        <div style={{ padding: "6px 10px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0", flex: 1 }}>
          <div className="cx-t10hd"><b style={{ fontSize: "13px" }}>Escape log</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div className="cx-t14card">
            <b>3 of 38 merges · 8%</b>
            <small>Escaped in the last 60 days: 2 reverted, 1 found after the merge. 2 checks added, 1 being written.</small>
          </div>
          <div className="cx-t14card">
            <b>Rename the legacy returnTo helper</b>
            <div className="cx-t14r"><span className="cx-apchip">Reverted by Crew</span><span className="cx-apchip">Test gap</span></div>
            <small>Sat 7 Nov, 15:34</small>
            <small>Check: Added Sat 7 Nov, 16:22</small>
            <small>Counts against Claude Code · Sonnet 5.5 · auth/</small>
          </div>
          <div className="cx-t14card">
            <b>Use next in the auth redirect</b>
            <div className="cx-t14r"><span className="cx-apchip">Found by you</span><span className="cx-apchip">Tooling gap</span></div>
            <small>Sat 7 Nov, 15:02</small>
            <small>Check: Being written</small>
            <small>Counts against Claude Code · Sonnet 5.5 · auth/</small>
          </div>
          <div className="cx-t14card">
            <b>Move session helpers to auth/session.ts</b>
            <div className="cx-t14r"><span className="cx-apchip">Reverted by you</span><span className="cx-apchip">Test gap</span></div>
            <small>Tue 3 Nov, 13:15</small>
            <small>Check: Added Tue 3 Nov, 17:05</small>
            <small>Counts against Codex · GPT-5.5 · auth/</small>
          </div>
        </div>
      </PanelShell>
    </Frame>
  );
}
