import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-create-check`. */
export function EscapeCreateCheck() {
  return (
    <Frame height={860}>
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
          <div className="cx-apq cx-rv">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Rename the legacy returnTo helper</b>
              <div className="cx-m">Merged 12:10 · reverted by Crew at 15:34 · Claude Code · Sonnet 5.5</div>
              <div className="cx-pf">
                <span><Ic n="alert" sm />Main broke at 15:11</span>
                <span><Ic n="ok" sm />Revert checked on the combined state</span>
                <span><Ic n="info" sm />Cause was certain</span>
              </div>
            </div>
            <div className="cx-ac">
              <Btn className="cx-btn">Open the revert</Btn>
              <Btn className="cx-ghost">Create a check</Btn>
              <Btn className="cx-ghost">Comment</Btn>
            </div>
            <div className="cx-rb">Reverted automatically · the fix brief is queued</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Use next in the auth redirect</b>
              <div className="cx-m">Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl</div>
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
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "600px" }}>
              <h4>Create a check from this revert</h4>
              <p>
                “Rename the legacy returnTo helper” was reverted at 15:34 after main broke at 15:11. Crew can write a check that fails on that merge and passes without it, so the same change can’t reach main again.
              </p>
              <div className="cx-fbx">
                <div className="cx-fkv">
                  <span>What escaped</span>
                  <b>Old returnTo links stopped redirecting</b>
                  <span>Merge</span>
                  <b>8d41b7c · Sat 7 Nov, 12:10 · Claude Code · Sonnet 5.5</b>
                  <span>Counts against</span>
                  <b>Claude Code · Sonnet 5.5 · auth/</b>
                </div>
              </div>
              <div className="cx-fld2">
                <span style={{ color: "var(--color-muted)" }}>What kind of gap was it?</span>
                <div className="cx-t14opt cx-on">
                  <Ic n="ok" sm />
                  <div><b>Test gap</b><small>A test can catch this: it is a plain input and output. Suggested by Crew</small></div>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <div><b>Tooling gap</b><small>No cheap test could catch it, so Crew adds a repo rule or a tool step</small></div>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>The check</h6>
                <div className="cx-fkv">
                  <span>Name</span>
                  <b>Legacy returnTo links still redirect</b>
                  <span>Where</span>
                  <b>tests/auth/return-to.test.ts</b>
                  <span>Proof before it is added</span>
                  <b>It must fail on 8d41b7c and pass on the revert. If it doesn’t, Crew doesn’t add it and asks you</b>
                </div>
              </div>
              <p style={{ fontSize: "12px" }}>
                The check is a normal task: it goes through review and the merge queue like any change. Until it merges, nothing about your tests changes.
              </p>
              <div className="cx-ra">
                <Btn className="cx-ghost">Cancel</Btn>
                <Btn className="cx-ghost">Not an escape…</Btn>
                <Btn className="cx-btn">Create the check</Btn>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
