import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-check-invalid`. */
export function EscapeCheckInvalid() {
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
          <span style={{ color: "var(--color-muted)" }}>Escape log</span>
          <Ic n="right" sm />
          <b>Move session helpers to auth/session.ts</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Move session helpers to auth/session.ts</b>
            <span className="cx-apchip">Reverted by you</span>
            <span className="cx-apchip">Test gap</span>
            <span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Change the class…</Btn><Btn className="cx-ghost">Not an escape…</Btn></span>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Same day unless a date is shown</span>
              </div>
              <div>
                <span className="cx-t">Tue 3 Nov, 09:40</span>
                <Ic n="git" sm />
                <span><b>Merged to main</b><small>Move session helpers to auth/session.ts · Codex · GPT-5.5 · Medium risk · 3be5a02</small></span>
              </div>
              <div>
                <span className="cx-t">13:15</span>
                <Ic n="restore" sm />
                <span><b>Reverted by you</b><small>People were asked to sign in again after a session refresh. The revert passed its checks.</small></span>
              </div>
              <div>
                <span className="cx-t">13:40</span>
                <Ic n="tree" sm />
                <span><b>Classified: test gap</b><small>A plain input and output that a test could have caught. Crew chose this.</small></span>
              </div>
              <div className="cx-am">
                <span className="cx-t">14:50</span>
                <Ic n="alert" sm />
                <span>
                  <b>The check passes on the bad commit</b>
                  <small>
                    Crew wrote “An expired session refreshes without a new sign-in” and ran it on 3be5a02. It passed, so it would not have caught this. Crew did not add it.
                  </small>
                </span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx cx-amber">
                <h6>The check did not prove itself</h6>
                <p>
                  A check is added when it fails on the bad commit and passes without it. This one passes on both, so adding it would give false comfort. Nothing was merged.
                </p>
                <div style={{ display: "flex", gap: "8px", flexWrap: "wrap", marginTop: "6px" }}>
                  <Btn className="cx-btn">Edit the check</Btn>
                  <Btn className="cx-ghost">Let Crew try again</Btn>
                  <Btn className="cx-ghost">Mark as a tooling gap</Btn>
                  <Btn className="cx-ghost">Not an escape…</Btn>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>What was tried</h6>
                <div className="cx-fkv">
                  <span>Check</span>
                  <b>An expired session refreshes without a new sign-in</b>
                  <span>Where</span>
                  <b>tests/auth/session.test.ts</b>
                  <span>On 3be5a02</span>
                  <b>Passed</b>
                  <span>On the revert</span>
                  <b>Passed</b>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>Still counted</h6>
                <p>The escape counts against Codex · GPT-5.5 · auth/ while you decide: 1 escape in 12 merges for this engine, last 60 days.</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
