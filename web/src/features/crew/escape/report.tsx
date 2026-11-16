import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-report`. */
export function EscapeReport() {
  return (
    <Frame height={880}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={0} items={["Review queue · 7", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-cwtool" style={{ borderTop: "0" }}>
          <span className="cx-fil cx-on">To review 7</span>
          <span className="cx-fil">High 1</span>
          <span className="cx-fil">Overdue 0</span>
          <span className="cx-fil">Reverted 1</span>
          <span className="cx-fil"><Ic n="down" sm />Engine: any</span>
          <span style={{ marginLeft: "auto" }}>Ranked by risk, then by the time you have left</span>
        </div>
        <div style={{ padding: "4px 20px 12px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Add a rate limit to the login route</b>
              <div className="cx-m">
                {"Merged by Crew at 16:10 · Codex · GPT-5.5 · crew/login-limit · auth/login.ts "}
                <span className="cx-t13mk"><Ic n="restore" sm />Auth is on On: 2 reverts in 14 days</span>
              </div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 4 of 4</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 1 vendor</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 1</span>
                <span><Ic n="clock" sm />32k tokens · 14 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov, 16:10 · 23 h left</div>
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
                <span><Ic n="clock" sm />52k tokens · 22 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov, 14:32 · 22 h left</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rlo">Low</span></div>
            <div className="cx-tt">
              <b>Update the login docs</b>
              <div className="cx-m">Merged by Crew at 14:12 · Claude Code · Sonnet 5.5 · docs/login.md</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 4 of 4</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 1 vendor</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 1</span>
                <span><Ic n="clock" sm />32k tokens · 14 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Batch review · no deadline</div>
          </div>
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "600px" }}>
              <h4>Mark this merge as an escape</h4>
              <p>
                Use this when a merge reached main and turned out to have a bug, and you are not reverting it. A revert already counts. Crew counts it against the engine, model and area, and offers to create a check.
              </p>
              <div className="cx-fbx">
                <div className="cx-fkv">
                  <span>Merge</span>
                  <b>Use next in the auth redirect · c7f20b9 · Sat 7 Nov, 14:32</b>
                  <span>Counts against</span>
                  <b>Claude Code · Sonnet 5.5 · auth/</b>
                </div>
              </div>
              <div className="cx-fld2">
                <span style={{ color: "var(--color-muted)" }}>What went wrong</span>
                <span className="cx-inp" style={{ fontFamily: "var(--font-sans)" }}>The legacy login route lost its rate limit.</span>
              </div>
              <div className="cx-fld2">
                <span style={{ color: "var(--color-muted)" }}>How it was found</span>
                <div className="cx-t14opt cx-on"><Ic n="ok" sm /><div><b>Found by me</b><small>You saw it in a review or on main</small></div></div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <div><b>Found by a later check or a scan</b><small>A check, a watcher or a reviewer lane saw it after the merge</small></div>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <div><b>Reported by someone else</b><small>A user, a teammate or an incident</small></div>
                </div>
              </div>
              <div className="cx-fld2">
                <span style={{ color: "var(--color-muted)" }}>{"The fix "}<span className="cx-rtag cx-lo">Optional</span></span>
                <span className="cx-inp" style={{ fontFamily: "var(--font-sans)" }}>Add a rate limit to the login route · merged 16:10</span>
              </div>
              <p style={{ fontSize: "12px" }}>
                It starts counting at once, and shows in the escape log, the dashboard and Autopilot by area. You can remove it later with Not an escape.
              </p>
              <div className="cx-ra"><Btn className="cx-ghost">Cancel</Btn><Btn className="cx-btn">Mark as an escape</Btn></div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
