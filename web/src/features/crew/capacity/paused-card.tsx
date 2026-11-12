import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-paused-card`. */
export function CapacityPausedCard() {
  return (
    <Frame height={800}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={3} items={["Flow", "Review · 9", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwboard">
          <div className="cx-cwcol">
            <div className="cx-cwch"><span className="cx-st"><i className="cx-sd" />Queued</span><span className="cx-n">3</span></div>
            <div className="cx-cwcards">
              <div className="cx-cwcard">
                <div className="cx-k"><b>docs</b>· Lead · 1m ago</div>
                <div className="cx-t">Update the login docs for the new redirect rule</div>
                <div className="cx-e"><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Not routed yet</b><span className="cx-mdl" /></div>
                <div className="cx-ln"><Ic n="lock" sm />docs/login.md</div>
              </div>
              <div className="cx-cwcard">
                <div className="cx-k"><b>research</b>· Lead</div>
                <div className="cx-t">Map every use of returnTo in the app</div>
                <div className="cx-e">
                  <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Not routed yet</b>
                  <span className="cx-mdl">Low priority · cleanup</span>
                </div>
                <div className="cx-ln"><Ic n="clock" sm />Paused since 16:52 · review queue over its limit</div>
                <div className="cx-ca"><Btn className="cx-ghost">Start it now</Btn><Btn className="cx-ghost">Open</Btn></div>
              </div>
              <div className="cx-cwcard">
                <div className="cx-k"><b>test</b>· Lead</div>
                <div className="cx-t">Check from the escape: //evil.com</div>
                <div className="cx-e"><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Not routed yet</b><span className="cx-mdl">Low priority</span></div>
                <div className="cx-ln"><Ic n="clock" sm />Paused since 16:52 · review queue over its limit</div>
                <div className="cx-ca"><Btn className="cx-ghost">Start it now</Btn><Btn className="cx-ghost">Open</Btn></div>
              </div>
            </div>
          </div>
          <div className="cx-cwcol">
            <div className="cx-cwch"><span className="cx-st"><i className="cx-sd" />Routed</span><span className="cx-n">1</span></div>
            <div className="cx-cwcards">
              <div className="cx-cwcard">
                <div className="cx-k"><b>implement</b>· Lead · now</div>
                <div className="cx-t">Rename returnTo to next in the session code</div>
                <div className="cx-e"><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Claude Code</b><span className="cx-mdl">Sonnet 5.5 · Low</span></div>
                <div className="cx-ck"><div className="cx-bar"><i style={{ width: "0%" }} /></div>0/2 checks</div>
                <div className="cx-ln"><Ic n="lock" sm />auth/session.ts</div>
              </div>
            </div>
          </div>
          <div className="cx-cwcol">
            <div className="cx-cwch">
              <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Running</span>
              <span className="cx-n">2</span>
            </div>
            <div className="cx-cwcards">
              <div className="cx-cwcard">
                <div className="cx-k"><b>implement</b>· Lead · now</div>
                <div className="cx-t">Use next in the auth redirect when it points inside the app</div>
                <div className="cx-e">
                  <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Claude Code</b>
                  <span className="cx-mdl">Sonnet 5.5 · Medium</span>
                </div>
                <div className="cx-ck"><div className="cx-bar"><i style={{ width: "33%" }} /></div>1/3 checks</div>
                <div className="cx-ln"><Ic n="lock" sm />auth/redirect.ts</div>
              </div>
              <div className="cx-cwcard">
                <div className="cx-k"><b>review</b>· Lead · 12s ago</div>
                <div className="cx-t">Review the diff for open redirects</div>
                <div className="cx-e"><b style={{ fontWeight: 500, color: "var(--color-text)" }}>OpenCode</b><span className="cx-mdl">DeepSeek V4 · High</span></div>
                <div className="cx-ck"><div className="cx-bar"><i style={{ width: "0%" }} /></div>0/2 checks</div>
                <div className="cx-ln"><Ic n="lock" sm />read only</div>
              </div>
            </div>
          </div>
          <div className="cx-cwcol">
            <div className="cx-cwch">
              <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Verifying</span>
              <span className="cx-n">1</span>
            </div>
            <div className="cx-cwcards">
              <div className="cx-cwcard">
                <div className="cx-k"><b>test</b>· Lead · now</div>
                <div className="cx-t">Add tests for the redirect, including //evil.com</div>
                <div className="cx-e"><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Codex</b><span className="cx-mdl">GPT-5.5 · Medium</span></div>
                <div className="cx-ck"><div className="cx-bar"><i style={{ width: "66%" }} /></div>2/3 checks</div>
                <div className="cx-ln"><Ic n="lock" sm />auth/*.test.ts</div>
              </div>
            </div>
          </div>
          <div className="cx-cwcol">
            <div className="cx-cwch cx-amber"><span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span><span className="cx-n">1</span></div>
            <div className="cx-cwcards">
              <div className="cx-cwcard cx-cneeds">
                <div className="cx-k"><b>review</b>· Lead · 1m ago</div>
                <div className="cx-t">Which target wins when both next and returnTo are set?</div>
                <div className="cx-e"><b style={{ fontWeight: 500, color: "var(--color-text)" }}>OpenCode</b><span className="cx-mdl">DeepSeek V4 · High</span></div>
                <div className="cx-ck"><div className="cx-bar"><i style={{ width: "0%" }} /></div>0/2 checks</div>
                <div className="cx-ln"><Ic n="lock" sm />read only</div>
                <div className="cx-ln cx-amb">Asked you a question</div>
                <div className="cx-ca"><Btn className="cx-btn">Answer</Btn><Btn className="cx-ghost">Open</Btn></div>
              </div>
            </div>
          </div>
          <div className="cx-cwcol cx-crail"><span className="cx-num">3</span><Ic n="down" sm /><span className="cx-nmv">Passed</span></div>
          <div className="cx-cwcol cx-crail"><span className="cx-num">2</span><Ic n="down" sm /><span className="cx-nmv">Stopped · Cancelled</span></div>
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "600px" }}>
              <h4>Paused: your review queue is over its limit</h4>
              <p>
                “Map every use of returnTo in the app” is a Low-priority task that Crew started itself, as a cleanup. New Low-priority starts wait while more changes are waiting for your review than your limit.
              </p>
              <div className="cx-fbx">
                <div className="cx-fkv">
                  <span>Waiting for you</span>
                  <b>9 changes (2 High, 7 Medium) · your limit is 8</b>
                  <span>It starts when</span>
                  <b>The queue is at 8 or fewer: 1 change to review</b>
                  <span>Paused since</span>
                  <b>16:52 · it was queued at 16:41</b>
                  <span>Still running</span>
                  <b>3 tasks, none affected</b>
                </div>
              </div>
              <p style={{ fontSize: "12px" }}>
                Starting it now is your decision and is recorded in the ledger. It doesn’t change your limit, and the queue stays over it until you review.
              </p>
              <div className="cx-ra">
                <Btn className="cx-ghost">Cancel the task</Btn>
                <Btn className="cx-ghost">Start it now</Btn>
                <Btn className="cx-btn">Open the Review queue</Btn>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
