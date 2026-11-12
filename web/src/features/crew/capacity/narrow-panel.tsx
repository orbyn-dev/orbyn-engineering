import { Frame } from "../shared/Frame";
import { PanelShell } from "../shared/PanelShell";

/** Crew Designs v21, frame `capacity-narrow-panel`. */
export function CapacityNarrowPanel() {
  return (
    <Frame panel>
      <PanelShell on="Review">
        <div style={{ padding: "6px 10px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0", flex: 1 }}>
          <div className="cx-cwseg" style={{ width: "max-content" }}><span className="cx-on">Review</span><span>Merge queue</span><span>Digest</span></div>
          <div className="cx-t14card">
            <b>New low-priority starts are paused</b>
            <div className="cx-t14r"><span className="cx-apchip cx-dim">Paused</span></div>
            <small>
              Your review queue is over its limit: 9 changes are waiting, your limit is 8. Running work, tasks you start and High-priority work carry on.
            </small>
          </div>
          <div className="cx-fbx" style={{ gap: "6px" }}>
            <div style={{ display: "flex", gap: "6px", alignItems: "center", flexWrap: "wrap" }}>
              <span className="cx-aprk cx-rhi">High</span>
              <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>2 h left</span>
            </div>
            <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Rotate the signing key</b>
            <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>Checks 5 of 5 · Not verified: 2</span>
          </div>
          <div className="cx-fbx" style={{ gap: "6px" }}>
            <div style={{ display: "flex", gap: "6px", alignItems: "center", flexWrap: "wrap" }}>
              <span className="cx-aprk cx-rhi">High</span>
              <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>2 h left</span>
            </div>
            <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Revoke the old API tokens</b>
            <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>Checks 4 of 4 · Not verified: 1</span>
          </div>
          <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>7 more Medium changes. Open in Crew shows the whole queue.</span>
        </div>
      </PanelShell>
    </Frame>
  );
}
