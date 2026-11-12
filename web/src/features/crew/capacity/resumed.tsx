import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-resumed`. */
export function CapacityResumed() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 6", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={0} items={["Review queue · 6", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-cwbn">
          <Ic n="info" sm />
          <span>
            <b>New low-priority starts have resumed.</b>
            {" 6 changes are waiting for your review, under your limit of 8. 2 paused tasks began at 16:48."}
          </span>
          <span className="cx-acts"><Btn className="cx-ghost">Open the Board</Btn></span>
        </div>
        <div className="cx-cwtool" style={{ borderTop: "0" }}>
          <span className="cx-fil cx-on">To review 6</span>
          <span className="cx-fil">High 2</span>
          <span className="cx-fil">Overdue 0</span>
          <span className="cx-fil">Reverted 0</span>
          <span className="cx-fil"><Ic n="down" sm />Engine: any</span>
          <span className="cx-t15tag" style={{ marginLeft: "auto" }}>Limit 8</span>
          <span style={{ marginLeft: "12px" }}>Ranked by risk, then by time left</span>
        </div>
        <div style={{ padding: "4px 20px 12px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-apq cx-top">
            <div className="cx-rt"><span className="cx-aprk cx-rhi">High</span></div>
            <div className="cx-tt">
              <b>Rotate the signing key</b>
              <div className="cx-m">Merged by Crew at 15:02 · Claude Code · Opus 5.5 · crew/keys-rotate · keys/rotate.ts, auth/login.ts</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 5 of 5</span>
                <span><Ic n="eye" sm />Reviewed by 3 lanes · 2 vendors</span>
                <span><Ic n="bug" sm />0 open findings</span>
                <span><Ic n="info" sm />Not verified: 2</span>
                <span><Ic n="clock" sm />41 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review within 4 h · by 19:02 · 2 h left. High risk stays at the top, with an overdue alert.</div>
          </div>
          <div className="cx-apq cx-top">
            <div className="cx-rt"><span className="cx-aprk cx-rhi">High</span></div>
            <div className="cx-tt">
              <b>Revoke the old API tokens</b>
              <div className="cx-m">Merged by Crew at 15:20 · Codex · GPT-5.5 · crew/tokens-revoke · auth/tokens.ts</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 4 of 4</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 1 vendor</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 1</span>
                <span><Ic n="clock" sm />14 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review within 4 h · by 19:20 · 2 h left</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Use next in the auth redirect</b>
              <div className="cx-m">Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl · auth/redirect.ts</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 3 of 3</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 2 vendors</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 4</span>
                <span><Ic n="clock" sm />22 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov, 14:32 · 21 h left</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Add a rate limit to the login route</b>
              <div className="cx-m">Merged by Crew at 14:50 · Codex · GPT-5.5 · crew/login-limit · auth/login.ts</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 4 of 4</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 1 vendor</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 1</span>
                <span><Ic n="clock" sm />14 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov, 14:50 · 21 h left</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt"><b>2 more Medium changes</b><div className="cx-m">Merged by Crew between 09:40 and 11:20 · auth/, web/</div></div>
            <div className="cx-ac"><Btn className="cx-ghost">Show all 6</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov · between 09:40 and 11:20</div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
