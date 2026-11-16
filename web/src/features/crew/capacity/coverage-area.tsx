import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage-area`. */
export function CapacityCoverageArea() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Dashboard</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Review coverage</span>
          <Ic n="right" sm />
          <b>auth/</b>
        </div>
        <div className="cx-t14pg">
          <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>
            As of Sat 7 Nov 2026, 17:00 · orbyn-engineering · 1,420 agent-written lines from 9 merges
          </span>
          <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
            <Btn className="cx-btn">Add the 200 lines in no queue to my Review queue</Btn>
            <Btn className="cx-ghost">Open the auth/ changes</Btn>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.9fr) minmax(0,1fr) minmax(0,.8fr) minmax(0,.9fr) minmax(0,1fr)" }}>
            <div className="cx-h">File</div>
            <div className="cx-h">Agent-written lines</div>
            <div className="cx-h">Reviewed</div>
            <div className="cx-h">In your queue</div>
            <div className="cx-h">Not in a queue</div>
            <div><b>auth/redirect.ts</b><small>Use next in the auth redirect · Claude Code · Sonnet 5.5</small></div>
            <div><b>210</b></div>
            <div><b>120</b></div>
            <div><b>90</b></div>
            <div><b>0</b></div>
            <div><b>auth/login.ts</b><small>Add a rate limit to the login route · Codex · GPT-5.5</small></div>
            <div><b>360</b></div>
            <div><b>180</b></div>
            <div><b>140</b></div>
            <div><b>40</b></div>
            <div><b>auth/session.ts</b><small>Move session helpers · Codex · GPT-5.5</small></div>
            <div><b>320</b></div>
            <div><b>250</b></div>
            <div><b>0</b></div>
            <div><b>70</b></div>
            <div><b>auth/oauth.ts</b><small>Handle the OAuth state mismatch · Claude Code · Sonnet 5.5</small></div>
            <div><b>290</b></div>
            <div><b>230</b></div>
            <div><b>0</b></div>
            <div><b>60</b></div>
            <div><b>auth/tokens.ts</b><small>Revoke the old API tokens · Codex · GPT-5.5</small></div>
            <div><b>240</b></div>
            <div><b>130</b></div>
            <div><b>80</b></div>
            <div><b>30</b></div>
            <div><b>auth/</b><small>9 merges since Mon 12 Oct 2026</small></div>
            <div><b>1,420</b></div>
            <div><b>910</b></div>
            <div><b>310</b></div>
            <div><b>200</b></div>
          </div>
          <div className="cx-fbx">
            <h6>Adding to your queue</h6>
            <p>
              The 200 lines are from low-risk changes that weren’t sampled. Adding them puts those changes in your Review queue like any other; it doesn’t change your limit, and it counts toward it only if the change is High or Medium.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
