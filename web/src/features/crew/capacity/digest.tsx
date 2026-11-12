import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-digest`. */
export function CapacityDigest() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 4", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={2} items={["Review queue · 4", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-flcr"><span style={{ color: "var(--color-muted)" }}>Digest</span><Ic n="right" sm /><b>Sat 7 Nov 2026</b></div>
        <div style={{ padding: "16px 20px", display: "flex", flexDirection: "column", gap: "14px" }}>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3,minmax(0,1fr))", gap: "12px" }}>
            <div style={{ borderRadius: "12px", background: "var(--color-sunken)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: "4px", minWidth: "0" }}>
              <span style={{ fontSize: "26px", fontWeight: 500, color: "var(--color-text)", lineHeight: 1.1 }}>9</span>
              <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Merged</b>
              <small style={{ color: "var(--color-muted)", fontSize: "12px" }}>1 High · 5 Medium · 3 Low · 3 h 40 min of agent time</small>
              <div style={{ marginTop: "6px" }}>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rhi">High</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Rotate the signing key</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>15:02</span>
                </div>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rmd">Medium</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Use next in the auth redirect</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>14:32</span>
                </div>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rlo">Low</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Update the login docs</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>14:12</span>
                </div>
              </div>
            </div>
            <div style={{ borderRadius: "12px", background: "var(--color-sunken)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: "4px", minWidth: "0" }}>
              <span style={{ fontSize: "26px", fontWeight: 500, color: "var(--color-text)", lineHeight: 1.1 }}>4</span>
              <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Waiting for your review</b>
              <small style={{ color: "var(--color-muted)", fontSize: "12px" }}>0 overdue · the oldest is due Sat 7 Nov, 19:02 · limit 8</small>
              <div style={{ marginTop: "6px" }}>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rhi">High</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Rotate the signing key</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>top</span>
                </div>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rmd">Medium</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Use next in the auth redirect</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>22 h</span>
                </div>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rlo">Low</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Fix a typo in the redirect error</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>sampled</span>
                </div>
              </div>
            </div>
            <div style={{ borderRadius: "12px", background: "var(--color-sunken)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: "4px", minWidth: "0" }}>
              <span style={{ fontSize: "26px", fontWeight: 500, color: "var(--color-text)", lineHeight: 1.1 }}>2</span>
              <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Not sampled</b>
              <small style={{ color: "var(--color-muted)", fontSize: "12px" }}>Low-risk changes you didn’t have to review · all checks passed</small>
              <div style={{ marginTop: "6px" }}>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rlo">Low</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Update the login docs</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>14:12</span>
                </div>
                <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)", fontSize: "12px", color: "var(--color-textSoft)" }}>
                  <span className="cx-aprk cx-rlo">Low</span>
                  <span style={{ minWidth: "0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Rename a test helper</span>
                  <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>10:22</span>
                </div>
              </div>
            </div>
          </div>
          <div className="cx-fbx">
            <h6>Also today</h6>
            <p>
              1 merge was reverted (Rename the legacy returnTo helper, 15:34). Sampling is on: 1 of 3 low-risk changes was sampled for you. The 2 not sampled are listed here, and you can add any of them to your Review queue. Main broke once and was healthy again after 23 minutes.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
