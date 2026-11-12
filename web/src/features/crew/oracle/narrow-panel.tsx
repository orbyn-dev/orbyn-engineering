import { Frame } from "../shared/Frame";
import { PanelShell } from "../shared/PanelShell";

/** Crew Designs v22, frame `oracle-narrow-panel`. */
export function OracleNarrowPanel() {
  return (
    <Frame panel>
      <PanelShell on="Report">
        <div style={{ padding: "6px 10px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0", flex: 1 }}>
          <div className="cx-t10hd"><b style={{ fontSize: "13px" }}>Answer key</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div className="cx-t14card">
            <b>Add a rate limit to the login route</b>
            <div className="cx-t14r"><span className="cx-apchip">Independent</span><span className="cx-st cx-ok"><i className="cx-sd" />Gate passed</span></div>
            <small>Built by Codex · GPT-5.5</small>
            <small>Key by Claude Code · Sonnet 5.5</small>
            <small>Brief and contract facts only, not the code</small>
          </div>
          <div className="cx-t14card">
            <b>Mutation gate</b>
            <small>15 of 18 mutants killed · 3 survived</small>
            <small>Stryker · TypeScript</small>
            <small>A passed gate shows the checks can catch a break, not that the change is right.</small>
          </div>
          <div className="cx-t14card">
            <b>3 mutants survived</b>
            <small>auth/rate-limit.ts:41 · the edge of the limit</small>
            <small>auth/rate-limit.ts:58 · the reset time</small>
            <small>auth/login.ts:33 · the Retry-After value</small>
          </div>
          <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>The 6 checks and the timeline are behind Open in Crew.</span>
        </div>
      </PanelShell>
    </Frame>
  );
}
